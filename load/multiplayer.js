// Protocol-level load test for the MULTIPLAYER API surface.
//
// k6 has no browser and no cookie jar: every request carries the session cookie
// explicitly (see `cookieHeader`). `setup()` logs the four bot accounts in,
// provisions any that are missing, resolves one playable activeGameId, and
// hands every VU the same compact fixture.
//
// Wire shapes (verified against the Rust backend, see
// client/backend/server/src/routers/active_game.rs):
//   POST /api/trpc/<proc>  body {"0": input}  ->  [{result:{data}}] | [{error:{message}}]
//   activeGame.get          {id}   -> {gameId, game:{questions:[{number,len,direction,rootX,rootY}]}, actions:[...]}
//   activeGame.start        {gameId} -> {id}
//   activeGame.join         {id}   -> {id, joined}
//   activeGame.addActions   {id, actions:[{cordX,cordY,state}]} -> {actions:[{cordX,cordY,state,previousState,...}], solved, filled, ...}
//   activeGame.publishPresence {id, number?, direction?} -> {ok:true}
//   GET /api/grids/<gameId> -> {questions:[{number, direction, answer}]}

import http from 'k6/http';
import { Counter, Rate, Trend } from 'k6/metrics';
import { check, sleep } from 'k6';
import ws from 'k6/ws';

// ---------------------------------------------------------------------------
// Configuration. Every knob is an env var with a smoke-sized default so that
// `k6 run multiplayer.js` is safe against a live host out of the box.
// ---------------------------------------------------------------------------
const BASE_URL = (__ENV.E2E_BASE_URL || 'https://crosswords-staging.casazza.io').replace(/\/+$/, '');
const DURATION = __ENV.SOAK_DURATION || '30s';
const VUS = num(__ENV.VUS, 4);
const JOIN_VUS = num(__ENV.JOIN_VUS, 4);
const JOIN_DURATION = __ENV.JOIN_DURATION || '15s';
const PRESENCE_VUS = num(__ENV.PRESENCE_VUS, 2);
const READ_VUS = num(__ENV.READ_VUS, 2);
const WS_VUS = num(__ENV.WS_VUS, 2);
// How long one VU holds its subscription socket open before closing it.
const WS_HOLD_MS = num(__ENV.WS_HOLD_MS, 10000);
const THINK_TIME = num(__ENV.THINK_TIME, 1);
const PRESENCE_THINK = num(__ENV.PRESENCE_THINK, 3);
const READ_THINK = num(__ENV.READ_THINK, 2);

// Letters submitted per addActions call (one "player keystroke batch").
const BATCH_SIZE = num(__ENV.BATCH_SIZE, 5);

// Share of the sampled addActions batches whose cells are forced onto crossing
// cells. 1.0 = maximum contention on the shared intersections.
const CROSSING_SHARE = num(__ENV.CROSSING_SHARE, 0.8);

const WS_ERROR_RATE_MAX = num(__ENV.WS_ERROR_RATE_MAX, 0.05);
const FAILED_RATE_MAX = num(__ENV.FAILED_RATE_MAX, 0.01);
const CONFLICT_RATE_MAX = num(__ENV.CROSSING_CONFLICT_MAX, 0.05);

const SUMMARY_JSON = __ENV.SUMMARY_JSON || 'load-summary.json';

const ACCOUNTS = [
  { email: __ENV.E2E_EMAIL, password: __ENV.E2E_PASSWORD },
  { email: __ENV.E2E_EMAIL_2, password: __ENV.E2E_PASSWORD_2 },
  { email: __ENV.E2E_EMAIL_3, password: __ENV.E2E_PASSWORD_3 },
  { email: __ENV.E2E_EMAIL_4, password: __ENV.E2E_PASSWORD_4 },
];

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

// Share of addActions responses where the cell state the server reported it
// read before the write was not the letter this VU last submitted for that
// cell — i.e. another player (or a stale snapshot) had it. See README.
const crossingConflictRate = new Rate('crossing_conflict_rate');
const trpcErrorRate = new Rate('trpc_error_rate');
const addActionsDuration = new Trend('crossword_add_actions_duration', true);
const joinDuration = new Trend('crossword_join_duration', true);
// Subscription frames the server pushed down /api/trpc-ws, tagged by the
// subscription path they arrived on (`proc`).
const broadcastReceived = new Counter('crossword_broadcast_received');
const wsErrorRate = new Rate('crossword_ws_error_rate');

export const options = {
  scenarios: {
    join: {
      executor: 'constant-vus',
      vus: JOIN_VUS,
      duration: JOIN_DURATION,
      exec: 'joinScenario',
      tags: { scenario: 'join' },
    },
    actions: {
      executor: 'constant-vus',
      vus: VUS,
      duration: DURATION,
      exec: 'actionsScenario',
      tags: { scenario: 'actions' },
    },
    presence: {
      executor: 'constant-vus',
      vus: PRESENCE_VUS,
      duration: DURATION,
      exec: 'presenceScenario',
      tags: { scenario: 'presence' },
    },
    read: {
      executor: 'constant-vus',
      vus: READ_VUS,
      duration: DURATION,
      exec: 'readScenario',
      tags: { scenario: 'read' },
    },
    subscriptions: {
      executor: 'constant-vus',
      vus: WS_VUS,
      duration: DURATION,
      exec: 'subscriptionsScenario',
      tags: { scenario: 'subscriptions' },
    },
  },
  thresholds: {
    http_req_failed: [`rate<${FAILED_RATE_MAX}`],
    crossing_conflict_rate: [`rate<${CONFLICT_RATE_MAX}`],
    trpc_error_rate: [`rate<${CONFLICT_RATE_MAX}`],
    crossword_ws_error_rate: [`rate<${WS_ERROR_RATE_MAX}`],
  },
  discardResponseBodies: false,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function num(raw, fallback) {
  const n = Number(raw);
  return Number.isFinite(n) ? n : fallback;
}

function cookieHeader(cookie) {
  return { Cookie: `next-auth.session-token=${cookie}` };
}

function trpc(cookie, proc, input) {
  const res = http.post(
    `${BASE_URL}/api/trpc/${proc}`,
    JSON.stringify({ 0: input === undefined ? {} : input }),
    {
      headers: Object.assign(
        { 'Content-Type': 'application/json' },
        cookieHeader(cookie)
      ),
      tags: { proc },
    }
  );
  return { res, data: trpcData(res) };
}

// The tRPC HTTP envelope is `[{result:{data}}]` or `[{error:{message}}]`.
function trpcData(res) {
  let body;
  try {
    body = res.json();
  } catch (e) {
    return null;
  }
  const first = Array.isArray(body) ? body[0] : body;
  if (!first) return null;
  if (first.error) return null;
  return first.result ? first.result.data : null;
}

function trpcError(res) {
  let body;
  try {
    body = res.json();
  } catch (e) {
    return res.status;
  }
  const first = Array.isArray(body) ? body[0] : body;
  if (first && first.error && first.error.message) return first.error.message;
  return `http_${res.status}`;
}

// POST form-encoded credentials login; returns the session cookie or null.
function login(email, password) {
  const res = http.post(
    `${BASE_URL}/api/auth/callback/credentials`,
    { email: email, password: password, callbackUrl: '/' },
    { headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, tags: { proc: 'auth.login' } }
  );
  const cookies = res.headers['Set-Cookie'] || res.headers['set-cookie'] || [];
  const list = Array.isArray(cookies) ? cookies : [cookies];
  for (const raw of list) {
    const match = /next-auth\.session-token=([^;]+)/.exec(String(raw));
    if (match) return match[1];
  }
  return null;
}

// `user.signup` is the only non-admin provisioning path; "already exists" is
// success, so this is idempotent.
function provision(account, index) {
  const username = `crosswords-bot-${index + 1}`;
  const name = `Crosswords Bot ${index + 1}`;
  const res = http.post(
    `${BASE_URL}/api/trpc/user.signup`,
    JSON.stringify({
      0: {
        email: account.email,
        name: name,
        username: username,
        password: account.password,
      },
    }),
    { headers: { 'Content-Type': 'application/json' }, tags: { proc: 'user.signup' } }
  );
  const err = trpcError(res);
  if (res.status >= 400 && !/exist/i.test(String(err))) {
    throw new Error(`user.signup failed for account ${index + 1}: ${err}`);
  }
}

// Every cell an answer occupies, walked from its root the same way the backend
// scores it: ACROSS advances x, DOWN advances y.
function answerCells(question) {
  const answer = question.answer || '';
  const cells = [];
  for (let i = 0; i < answer.length; i += 1) {
    const letter = answer[i];
    if (!letter || letter === ' ') continue;
    if (question.direction === 'ACROSS') {
      cells.push({ x: question.rootX + i, y: question.rootY, letter: letter });
    } else {
      cells.push({ x: question.rootX, y: question.rootY + i, letter: letter });
    }
  }
  return cells;
}

function cellKey(x, y) {
  return `${x},${y}`;
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

// k6 does NOT share module state between the VU/setup runtime and
// handleSummary, and it does NOT expose tag sub-metrics there either. What it
// does expose is `data.setup_data` — the object setup() returned — so the
// summary reads the ids from there.

export function setup() {
  const cookies = [];
  for (let i = 0; i < ACCOUNTS.length; i += 1) {
    const account = ACCOUNTS[i];
    if (!account.email || !account.password) {
      throw new Error(
        `account ${i + 1} is not configured: set E2E_EMAIL${i === 0 ? '' : '_' + (i + 1)} / E2E_PASSWORD${i === 0 ? '' : '_' + (i + 1)}`
      );
    }
    let cookie = login(account.email, account.password);
    if (!cookie) {
      provision(account, i);
      cookie = login(account.email, account.password);
    }
    if (!cookie) {
      throw new Error(`could not obtain a session cookie for account ${i + 1}`);
    }
    cookies.push(cookie);
  }

  // Resume an in-progress game if one exists, otherwise start one.
  const list = trpc(cookies[0], 'gameList.get', {});
  const rows = Array.isArray(list.data) ? list.data : [];
  let activeGameId = null;
  let gameId = null;
  for (const row of rows) {
    if (row.type === 'ActiveGame' && row.id) {
      activeGameId = row.id;
      gameId = row.gameId;
      break;
    }
  }
  if (!activeGameId) {
    // `gameList.get` returns a MIX: `type` is only ever `ActiveGame` or
    // `CompletedGame` — never "Game", so this used to select
    // `row.type === 'Game'` and matched nothing at all. It only surfaced when
    // the caller had no live ActiveGame left to resume, which is exactly what
    // happens once every game has been completed or abandoned.
    //
    // The parent puzzle id is `gameId` on EVERY row; `id` is the caller's own
    // attempt. Start a fresh game on the first non-ActiveGame parent.
    const published = rows.find((row) => row.type !== 'ActiveGame' && row.gameId);
    if (!published) {
      throw new Error('gameList.get returned no published game to start');
    }
    const started = trpc(cookies[0], 'activeGame.start', { gameId: published.gameId });
    if (!started.data || !started.data.id) {
      throw new Error(`activeGame.start failed: ${trpcError(started.res)}`);
    }
    activeGameId = started.data.id;
    // The PARENT puzzle id, which is what `/api/grids/<gameId>` is keyed by.
    // `published.id` is the caller's own attempt, not the parent.
    gameId = published.gameId;
  }

  // Everyone joins before the scenarios begin, so `actions` and `presence` are
  // never measuring the membership gate instead of the write path.
  for (let i = 0; i < cookies.length; i += 1) {
    const joined = trpc(cookies[i], 'activeGame.join', { id: activeGameId });
    if (!joined.data) {
      throw new Error(`activeGame.join failed for account ${i + 1}: ${trpcError(joined.res)}`);
    }
  }

  const state = trpc(cookies[0], 'activeGame.get', { id: activeGameId });
  const game = state.data && state.data.game;
  if (!game || !Array.isArray(game.questions)) {
    throw new Error(`activeGame.get returned no questions: ${trpcError(state.res)}`);
  }
  if (!gameId) gameId = state.data.gameId;

  // The answer key, so VUs submit REAL letters (the server derives the verdict).
  const gridRes = http.get(`${BASE_URL}/api/grids/${gameId}`, {
    headers: cookieHeader(cookies[0]),
    tags: { proc: 'grids.answerKey' },
  });
  const grid = gridRes.status === 200 ? gridRes.json() : null;
  if (!grid || !Array.isArray(grid.questions)) {
    throw new Error(`could not read the answer key for ${gameId}`);
  }

  const byNumber = {};
  for (const q of grid.questions) {
    byNumber[`${q.number}:${q.direction}`] = q.answer || '';
  }

  // Walk every answer into concrete cells, then split them into "plain" cells
  // and "crossing" cells (a cell more than one answer passes through — the
  // contention case).
  const plainCells = [];
  const crossingCells = [];
  for (const clue of game.questions) {
    const answer = byNumber[`${clue.number}:${clue.direction}`];
    if (!answer) continue;
    const cells = answerCells({
      answer: answer,
      direction: clue.direction,
      rootX: clue.rootX,
      rootY: clue.rootY,
    });
    for (const cell of cells) {
      plainCells.push(cell);
    }
  }
  const counts = {};
  for (const clue of game.questions) {
    const answer = byNumber[`${clue.number}:${clue.direction}`];
    if (!answer) continue;
    for (const cell of answerCells({
      answer: answer,
      direction: clue.direction,
      rootX: clue.rootX,
      rootY: clue.rootY,
    })) {
      const key = cellKey(cell.x, cell.y);
      counts[key] = (counts[key] || 0) + 1;
    }
  }
  for (const cell of plainCells) {
    if (counts[cellKey(cell.x, cell.y)] > 1) crossingCells.push(cell);
  }
  const plainOnly = plainCells.filter((c) => counts[cellKey(c.x, c.y)] === 1);
  if (plainOnly.length + crossingCells.length === 0) {
    throw new Error('the answer key produced no playable cells');
  }
  if (crossingCells.length === 0) {
    // No intersections on this grid: the contention scenario has nothing to
    // contend over. Say so rather than silently measuring nothing.
    console.warn('WARN: this grid has no crossing cells; contention is untestable on it');
  }
  // The letters that can LEGITIMATELY ever appear in a crossing cell's
  // `previousState`: the true answer, plus the per-account rotation each of
  // the four writers will submit. Computed ONCE here because k6 VUs run in
  // separate JS runtimes and share no state — a VU cannot know what another
  // VU wrote, so the membership test has to be handed down as data.
  //
  // This is what makes the metric meaningful. Asking "does previousState
  // equal what I last wrote" is false by design once writes serialise, and
  // read 0.40-0.50 both before and after the DEF-281 fix. Asking "is
  // previousState a letter some writer could have committed" is the real
  // invariant: a value outside this set is a state no write ever produced.
  const legitByCell = {};
  for (const cell of crossingCells) {
    const set = [cell.letter];
    for (let acct = 0; acct < cookies.length; acct += 1) set.push(rotate(cell.letter, acct));
    legitByCell[cellKey(cell.x, cell.y)] = set;
  }

  return {
    cookies: cookies,
    activeGameId: activeGameId,
    gameId: gameId,
    answers: {
      // Compact: just the letter cells the VUs will submit, split by kind.
      cells: plainOnly.map((c) => [c.x, c.y, c.letter]),
      crossing: crossingCells.map((c) => [c.x, c.y, c.letter]),
      // Clue numbers, so publishPresence can name a real clue.
      numbers: game.questions.map((q) => ({ number: q.number, direction: q.direction })),
    },
    legitByCell: legitByCell,
    baseUrl: BASE_URL,
    batchSize: BATCH_SIZE,
    crossingShare: CROSSING_SHARE,
  };
  return fixture;
}

// Each VU owns exactly one account: VU 1 -> account 1, VU 2 -> account 2, …
// With VUS=4 that is one writer per bot, which is what makes concurrent writes
// to the SAME crossing cell happen.
function accountIndex(data) {
  return (__VU - 1) % data.cookies.length;
}

// The four bot accounts each write a DIFFERENT letter to the same crossing
// cell (see `rotate`), so the value the server reports as `previousState` is
// normally some OTHER account's letter, not this one's.
//
// The metric therefore cannot ask "does previousState equal what I last
// wrote" — that is false by design the moment writes serialise correctly, and
// asking it made this gate read 0.40-0.50 both BEFORE and AFTER the DEF-281
// fix landed. The invariant that actually distinguishes correct
// serialisation from a lost update is:
//
//   previousState is either empty (nothing was there) or a letter that some
//   writer actually committed to that cell.
//
// Anything else means the server described a state no write ever produced.
// Verified directly against staging 0.1.85: eight concurrent distinct writes
// to one cell returned eight previousStates, every one a letter from that same
// batch — the chain, intact.
//

function authHeaders(data) {
  return Object.assign(
    { 'Content-Type': 'application/json' },
    cookieHeader(data.cookies[accountIndex(data)])
  );
}

// Crossing cells get a per-account letter rotation. If every VU submitted
// the true answer letter, all four would write the SAME character to the same
// cell and `previousState` would always equal what the caller last wrote —
// the conflict metric could never observe anything. Rotating by account makes
// each writer's letter distinct, so a lost or stale write is actually visible.
const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';

function rotate(letter, offset) {
  const idx = ALPHABET.indexOf(String(letter).toUpperCase());
  if (idx < 0) return letter;
  return ALPHABET[(idx + 1 + (offset % 25)) % 26];
}

function pickBatch(data) {
  const wantCrossing =
    data.answers.crossing.length > 0 && Math.random() < data.crossingShare;
  const source = wantCrossing ? data.answers.crossing : data.answers.cells;
  if (source.length === 0) {
    return data.answers.cells.concat(data.answers.crossing).slice(0, data.batchSize);
  }
  const offset = accountIndex(data);
  const batch = [];
  const seen = {};
  const size = Math.min(data.batchSize, Math.max(1, source.length));
  let guard = 0;
  while (batch.length < size && guard < size * 10) {
    guard += 1;
    const cell = source[Math.floor(Math.random() * source.length)];
    const key = cellKey(cell[0], cell[1]);
    if (seen[key]) continue;
    seen[key] = true;
    batch.push({
      cordX: cell[0],
      cordY: cell[1],
      state: wantCrossing ? rotate(cell[2], offset) : cell[2],
    });
  }
  return batch;
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

export function joinScenario(data) {
  const res = http.post(
    `${data.baseUrl}/api/trpc/activeGame.join`,
    JSON.stringify({ 0: { id: data.activeGameId } }),
    { headers: authHeaders(data), tags: { proc: 'activeGame.join' } }
  );
  joinDuration.add(res.timings.duration);
  const ok = check(res, {
    'join: 200': (r) => r.status === 200,
    'join: joined': () => !!trpcData(res),
  });
  trpcErrorRate.add(!ok);
  sleep(THINK_TIME);
}

export function actionsScenario(data) {
  const batch = pickBatch(data);
  if (batch.length === 0) {
    sleep(THINK_TIME);
    return;
  }
  const res = http.post(
    `${data.baseUrl}/api/trpc/activeGame.addActions`,
    JSON.stringify({ 0: { id: data.activeGameId, actions: batch } }),
    { headers: authHeaders(data), tags: { proc: 'activeGame.addActions' } }
  );
  addActionsDuration.add(res.timings.duration);

  const payload = trpcData(res);
  const created = payload && Array.isArray(payload.actions) ? payload.actions : null;
  const ok = check(res, {
    'addActions: 200': (r) => r.status === 200,
    'addActions: actions returned': () => !!created,
  });
  trpcErrorRate.add(!ok);

  if (created) {
    let conflicts = 0;
    let checked = 0;
    for (const action of created) {
      const key = cellKey(action.cordX, action.cordY);
      const serverSaw = typeof action.previousState === 'string' ? action.previousState : '';
      if (serverSaw === '') continue; // nothing was there: always legitimate

      checked += 1;
      const legit = data.legitByCell[key];
      // A cell we know nothing about cannot be judged; only crossing cells
      // carry a legitimate-letter set.
      if (legit && legit.indexOf(serverSaw) === -1) conflicts += 1;
    }
    // One sample per addActions response: 1 when any cell came back with a
    // state no write ever produced.
    crossingConflictRate.add(checked > 0 && conflicts > 0);
  }

  sleep(THINK_TIME);
}

export function presenceScenario(data) {
  const clues = data.answers.numbers;
  const clue = clues.length > 0 ? clues[Math.floor(Math.random() * clues.length)] : null;
  const res = http.post(
    `${data.baseUrl}/api/trpc/activeGame.publishPresence`,
    JSON.stringify({
      0: clue
        ? { id: data.activeGameId, number: clue.number, direction: clue.direction }
        : { id: data.activeGameId, number: null },
    }),
    { headers: authHeaders(data), tags: { proc: 'activeGame.publishPresence' } }
  );
  const ok = check(res, {
    'presence: 200': (r) => r.status === 200,
    'presence: ok': () => !!trpcData(res),
  });
  trpcErrorRate.add(!ok);
  // Humans click a clue every few seconds, not every few milliseconds.
  sleep(PRESENCE_THINK);
}

export function readScenario(data) {
  const board = http.post(
    `${data.baseUrl}/api/trpc/activeGame.get`,
    JSON.stringify({ 0: { id: data.activeGameId } }),
    { headers: authHeaders(data), tags: { proc: 'activeGame.get' } }
  );
  check(board, {
    'get: 200': (r) => r.status === 200,
    'get: has game': () => {
      const payload = trpcData(board);
      return !!(payload && payload.game);
    },
  });

  const grid = http.get(`${data.baseUrl}/api/grids/${data.gameId}`, {
    headers: cookieHeader(data.cookies[accountIndex(data)]),
    tags: { proc: 'grids.read' },
  });
  check(grid, { 'grids: 200': (r) => r.status === 200 });

  sleep(READ_THINK);
}

// The push side of multiplayer: /api/trpc-ws carries the next-auth cookie on
// the upgrade (k6 sends it via Params.headers — there is no cookie jar), then
// subscribes to both broadcasts the fan-out emits. Every `data` frame is
// counted, per subscription path.
export function subscriptionsScenario(data) {
  const wsUrl =
    data.baseUrl.replace(/^http/, 'ws') + '/api/trpc-ws';
  const params = { headers: cookieHeader(data.cookies[accountIndex(data)]) };
  const subscribed = {};
  let failed = false;

  ws.connect(wsUrl, params, function (socket) {
    socket.on('open', function () {
      socket.send(
        JSON.stringify({
          id: 1,
          method: 'subscription',
          params: { path: 'activeGame.onAddActions', input: { activeGameId: data.activeGameId } },
        })
      );
      socket.send(
        JSON.stringify({
          id: 2,
          method: 'subscription',
          params: { path: 'activeGame.onPresence', input: { activeGameId: data.activeGameId } },
        })
      );
      setTimeout(function () {
        socket.close();
      }, WS_HOLD_MS);
    });
    socket.on('message', function (raw) {
      let frame;
      try {
        frame = JSON.parse(String(raw));
      } catch (e) {
        return;
      }
      const result = frame && frame.result;
      if (!result || result.type !== 'data') return;
      const path = frame.id === 1 ? 'activeGame.onAddActions' : 'activeGame.onPresence';
      subscribed[path] = true;
      broadcastReceived.add(1, { proc: path });
    });
    socket.on('error', function () {
      failed = true;
    });
  });

  wsErrorRate.add(failed);
  check(subscribed, {
    'ws: addActions subscription pushed frames': (s) => !!s['activeGame.onAddActions'],
    'ws: presence subscription pushed frames': (s) => !!s['activeGame.onPresence'],
  });
}

// ---------------------------------------------------------------------------
// Summary: human text + machine JSON + Prometheus text exposition on stdout.
// ---------------------------------------------------------------------------

function metricValue(metric, field) {
  if (!metric || !metric.values) return undefined;
  return metric.values[field];
}

function broadcastSeries(metrics, label) {
  const metric = metrics.crossword_broadcast_received;
  return [
    `definitely_not_crosswords_load_broadcast_received_total{${label}} ${
      metricValue(metric, 'count') || 0
    }`,
  ];
}

function prometheus(data_, fixture_) {
  const metrics = data_.metrics;
  const lines = [];
  const push = (name, help, type, samples) => {
    lines.push(`# HELP ${name} ${help}`);
    lines.push(`# TYPE ${name} ${type}`);
    for (const line of samples) lines.push(line);
  };

  const label = `base_url="${fixture_.baseUrl}",active_game_id="${fixture_.activeGameId}"`;

  // handleSummary exposes no tag sub-metrics (verified against k6 2.0.0), so
  // there is no per-proc breakdown to emit — this is the run total.
  push(
    'definitely_not_crosswords_load_http_requests_total',
    'Total HTTP requests issued by the multiplayer load test.',
    'counter',
    [
      `definitely_not_crosswords_load_http_requests_total{${label}} ${
        metricValue(metrics.http_reqs, 'count') || 0
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_http_requests_failed_ratio',
    'Share of HTTP requests k6 counted as failed.',
    'gauge',
    [
      `definitely_not_crosswords_load_http_requests_failed_ratio{${label}} ${
        metricValue(metrics.http_req_failed, 'rate') || 0
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_http_duration_p95_seconds',
    'p95 HTTP request duration, in seconds.',
    'gauge',
    [
      `definitely_not_crosswords_load_http_duration_p95_seconds{${label}} ${
        (metricValue(metrics.http_req_duration, 'p(95)') || 0) / 1000
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_http_duration_p99_seconds',
    'p99 HTTP request duration, in seconds.',
    'gauge',
    [
      `definitely_not_crosswords_load_http_duration_p99_seconds{${label}} ${
        (metricValue(metrics.http_req_duration, 'p(99)') || 0) / 1000
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_crossing_conflict_rate',
    'Share of addActions responses whose pre-write cell state did not match the letter this VU last submitted.',
    'gauge',
    [
      `definitely_not_crosswords_load_crossing_conflict_rate{${label}} ${
        metricValue(metrics.crossing_conflict_rate, 'rate') || 0
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_crossing_conflicts_total',
    'addActions responses that reported at least one crossing-cell mismatch.',
    'counter',
    [
      `definitely_not_crosswords_load_crossing_conflicts_total{${label}} ${
        metricValue(metrics.crossing_conflict_rate, 'passes') || 0
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_trpc_error_rate',
    'Share of multiplayer calls that did not return a tRPC result envelope.',
    'gauge',
    [
      `definitely_not_crosswords_load_trpc_error_rate{${label}} ${
        metricValue(metrics.trpc_error_rate, 'rate') || 0
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_ws_error_ratio',
    'Share of subscription sockets that errored.',
    'gauge',
    [
      `definitely_not_crosswords_load_ws_error_ratio{${label}} ${
        metricValue(metrics.crossword_ws_error_rate, 'rate') || 0
      }`,
    ]
  );
  const checks = metrics.checks ? metrics.checks.values : {};
  let checkPass = 0;
  let checkTotal = 0;
  Object.keys(checks).forEach((name) => {
    checkPass += checks[name].passes || 0;
    checkTotal += (checks[name].passes || 0) + (checks[name].fails || 0);
  });
  push(
    'definitely_not_crosswords_load_checks_ratio',
    'Share of k6 checks that passed.',
    'gauge',
    [
      `definitely_not_crosswords_load_checks_ratio{${label}} ${
        checkTotal > 0 ? checkPass / checkTotal : 1
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_iterations_total',
    'Total VU iterations.',
    'counter',
    [
      `definitely_not_crosswords_load_iterations_total{${label}} ${
        metricValue(metrics.iterations, 'count') || 0
      }`,
    ]
  );
  push(
    'definitely_not_crosswords_load_vus_max',
    'Peak concurrent VUs.',
    'gauge',
    [`definitely_not_crosswords_load_vus_max{${label}} ${metricValue(metrics.vus_max, 'max') || 0}`]
  );
  push(
    'definitely_not_crosswords_load_broadcast_received_total',
    'WebSocket subscription frames received from /api/trpc-ws, by subscription path.',
    'counter',
    broadcastSeries(metrics, label)
  );
  return lines.join('\n') + '\n';
}

function humanText(data_, fixture_) {
  const metrics = data_.metrics;
  const rows = [];
  const interesting = [
    'count',
    'rate',
    'avg',
    'min',
    'med',
    'max',
    'p(90)',
    'p(95)',
    'p(99)',
  ];
  Object.keys(metrics)
    .sort()
    .forEach((name) => {
      const values = metrics[name].values || {};
      Object.keys(values)
        .sort()
        .forEach((field) => {
          if (interesting.indexOf(field) === -1) return;
          rows.push(`  ${name}[${field}] = ${values[field]}`);
        });
    });
  return [
    '=== multiplayer load test summary ===',
    `base url:     ${fixture_.baseUrl}`,
    `active game:  ${fixture_.activeGameId}`,
    `game:         ${fixture_.gameId}`,
    `http failures: ${metricValue(metrics.http_req_failed, 'rate') || 0}`,
    `crossing conflicts: ${metricValue(metrics.crossing_conflict_rate, 'rate') || 0}`,
    'metrics:',
  ]
    .concat(rows)
    .join('\n');
}

export function handleSummary(data_) {
  const target = data_.setup_data || {};
  const prom = prometheus(data_, target);
  console.log("\n" + humanText(data_, target) + "\n");
  console.log(prom);

  const json = {
    baseUrl: target.baseUrl,
    activeGameId: target.activeGameId,
    gameId: target.gameId,
    httpFailedRate: metricValue(data_.metrics.http_req_failed, 'rate') || 0,
    crossingConflictRate: metricValue(data_.metrics.crossing_conflict_rate, 'rate') || 0,
    broadcastReceived: metricValue(data_.metrics.crossword_broadcast_received, 'count') || 0,
    httpRequests: metricValue(data_.metrics.http_reqs, 'count') || 0,
    iterations: metricValue(data_.metrics.iterations, 'count') || 0,
    thresholds: {},
    metrics: data_.metrics,
  };
  Object.keys(data_.metrics).forEach((name) => {
    if (data_.metrics[name].thresholds) {
      json.thresholds[name] = data_.metrics[name].thresholds;
    }
  });

  return {
    [SUMMARY_JSON]: JSON.stringify(json, null, 2),
    stdout: prom,
  };
}