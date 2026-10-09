//! Crossword gameplay screen — Rust/Dioxus port of the Nuxt `pages/game/[id]`.
//!
//! Ports the Pinia `activeGame` store + `GameBoard`/`ActiveClueCard`/`QuestionsList`
//! Vue components into a single panel-kit workspace with two panels: Board and
//! Clues. `ActiveClueCard` is no longer a panel of its own — it renders inline
//! inside Clues, expanded on whichever row is selected, so the clue you are
//! answering and the list you picked it from are the same surface. All board
//! math comes from `crossword_core::game`.
//!
//! State model (per advisor): `questions` + `actions` are the source of truth;
//! `answer_maps`/`board_size`/`board` are derived via `use_memo`. The live
//! subscription only pushes into `actions`, so everything recomputes for free.
//! Selection snapshots a mutable `game_action_data` (the in-progress word).

use crossword_core::game::{
    board_size as compute_board_size, board_state_from_actions, compute_answer_map, ActionType,
    Cell, Direction, GameAction, Question, QuestionWithAnswerMap,
};
use dioxus::prelude::*;
use futures::channel::mpsc;
use futures::StreamExt;
use gloo_timers::future::{IntervalStream, TimeoutFuture};
use panel_kit::loading::ProgressBar;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use crate::components::identicon::Identicon;
use crate::net;
use crate::store::use_app_state;
use crate::Route;

/// Identity key for a question (number + direction; numbers are reused across
/// across/down so direction is part of the key).
type QKey = (i32, Direction);

/// Mounted handles for the grid's letter cells, keyed by coordinate, so the
/// roving cell can take DOM focus when the cursor moves.
type CellNodes = Rc<RefCell<HashMap<(i32, i32), Rc<MountedData>>>>;

fn qkey(q: &Question) -> QKey {
    (q.number, q.direction)
}

/// Does this word run through `(x, y)`? The single covering test the keyboard
/// layer uses to pick which word a coordinate belongs to — it must match
/// `select_coordinates`' own notion of "covers", or a keyboard move and a
/// click on the same cell would resolve to different clues.
fn covers_cell(m: &QuestionWithAnswerMap, x: i32, y: i32) -> bool {
    m.answer_map.iter().any(|c| c.cord_x == x && c.cord_y == y)
}

fn dir_str(d: Direction) -> &'static str {
    match d {
        Direction::Across => "ACROSS",
        Direction::Down => "DOWN",
    }
}

fn action_type_str(a: ActionType) -> &'static str {
    match a {
        ActionType::Placeholder => "placeholder",
        ActionType::CorrectGuess => "correctGuess",
        ActionType::IncorrectGuess => "incorrectGuess",
    }
}

/// One in-progress letter slot for the selected word (mirror of the TS
/// `gameActionData` entries). `state` is the currently-typed letter.
#[derive(Clone, PartialEq)]
struct ActionSlot {
    cord_x: i32,
    cord_y: i32,
    previous_state: String,
    state: String,
}

/// A co-op member of this game, from `activeGame.get`'s `gameMembers`.
#[derive(Clone, PartialEq)]
struct MemberInfo {
    user_id: String,
    user_name: String,
    is_owner: bool,
}

/// Another player's live selection, stamped with our local tick so stale
/// entries (a closed tab never broadcasts a clear) can be pruned.
#[derive(Clone, PartialEq)]
struct PresenceEntry {
    name: String,
    selection: Option<QKey>,
    tick: u64,
    sequence: i64,
}

/// A remote player's focused word, projected onto the board as a colored border.
#[derive(Clone, PartialEq)]
struct RemoteSelection {
    key: QKey,
    color: String,
    name: String,
    /// The socket is reconnecting, so this ring is a last-known position rather
    /// than a live one. Rendered dimmed instead of dropped (DEF-175 §4).
    stale: bool,
}

/// Presence entries older than this many seconds stop rendering.
const PRESENCE_TTL_SECS: u64 = 45;

/// Presence palette. These land in a `box-shadow: inset 0 0 0 2px …` ring on
/// the board cell, so they are `var()` references rather than literals — the
/// dark pastels sit at ~1.3:1 on a light cell, i.e. an invisible ring. The
/// per-theme values live in `styles.rs`. Yellow is reserved for the local
/// player (matching the existing selection styling); red stays exclusive to
/// incorrect guesses.
const SELF_COLOR: &str = "var(--pastel-yellow)";
const REMOTE_COLORS: [&str; 4] = [
    "var(--presence-1)",
    "var(--presence-2)",
    "var(--presence-3)",
    "var(--presence-4)",
];

/// Deterministic per-player color: remote players hash into the palette.
fn player_color(user_id: &str, my_id: Option<&str>) -> String {
    if Some(user_id) == my_id {
        return SELF_COLOR.to_string();
    }
    let h: usize = user_id.bytes().map(|b| b as usize).sum();
    REMOTE_COLORS[h % REMOTE_COLORS.len()].to_string()
}

/// "ACROSS" → Direction::Across (the wire format is UPPERCASE).
fn direction_from_str(s: &str) -> Option<Direction> {
    match s {
        "ACROSS" => Some(Direction::Across),
        "DOWN" => Some(Direction::Down),
        _ => None,
    }
}

/// Build the mutation input + optimistic local actions for a batch of slots.
/// Shared by placeholder saves and guess submissions — the only difference
/// is the action type stamped on each letter.
fn build_action_batch(
    slots: &[ActionSlot],
    game_id: &str,
    at: ActionType,
) -> (Value, Vec<GameAction>) {
    let at_str = action_type_str(at);
    let payload: Vec<Value> = slots
        .iter()
        .map(|s| {
            json!({
                "activeGameId": game_id,
                "cordX": s.cord_x,
                "cordY": s.cord_y,
                "actionType": at_str,
                "previousState": s.previous_state,
                "state": s.state,
            })
        })
        .collect();
    let now = js_now_iso();
    let local: Vec<GameAction> = slots
        .iter()
        .map(|s| GameAction {
            action_type: at,
            cord_x: s.cord_x,
            cord_y: s.cord_y,
            state: s.state.clone(),
            submitted_at: now.clone(),
        })
        .collect();
    (json!({ "id": game_id, "actions": payload }), local)
}

/// Write feedback for typed letters. Both writers (placeholder saves and
/// guesses) were optimistic with no visible pending/failed state, so a
/// dropped packet read exactly like "my partner just hasn't answered yet".
/// `Saved` carries the server's per-letter verdict count for the word when
/// the reply has one — the textual answer to "did this land?", which the
/// board's red/green wash alone left to inference.
#[derive(Clone, PartialEq)]
pub(crate) enum SyncState {
    Idle,
    Saving,
    /// (letters the server agreed with, letters in the word)
    Saved(Option<(usize, usize)>),
    Failed,
}

/// The one writer for typed letters. The wire action type is `placeholder`
/// either way — a guess is a placeholder save the server answers with
/// verdicts (DEF-243) — so `guess` only decides whether the saved line
/// reports the verdict count. Owns the sync lifecycle and the retry batch,
/// writes the optimistic local letters (so an unsaved word no longer vanishes
/// from the clue bubbles until the event lands), and reconciles the server's
/// verdicts on reply. Returns the reply plus this word's verdict map so the
/// caller can decide word-solved and grid-complete.
async fn commit_actions(
    game_id: &str,
    slots: &[ActionSlot],
    guess: bool,
    mut actions: Signal<Vec<GameAction>>,
    mut sync: Signal<SyncState>,
    mut failed_batch: Signal<Option<(Vec<ActionSlot>, bool)>>,
) -> Option<(Value, HashMap<(i32, i32), bool>)> {
    let (input, new_local) = build_action_batch(slots, game_id, ActionType::Placeholder);
    let mut next_actions = actions.peek().clone();
    next_actions.extend(new_local);
    actions.set(next_actions);
    sync.set(SyncState::Saving);

    let res = match net::mutation("activeGame.addActions", Some(input)).await {
        Ok(res) => res,
        Err(_) => {
            sync.set(SyncState::Failed);
            failed_batch.set(Some((slots.to_vec(), guess)));
            return None;
        }
    };

    // Adopt the server's verdicts in place of the optimistic placeholder
    // batch, so the grid colours from what the server actually decided.
    let mut from_reply: HashMap<(i32, i32), bool> = HashMap::new();
    if let Some(returned) = res.get("actions").and_then(|v| v.as_array()) {
        let reconciled: Vec<GameAction> = returned
            .iter()
            .filter_map(|a| serde_json::from_value::<GameAction>(a.clone()).ok())
            .collect();
        for a in &reconciled {
            from_reply.insert(
                (a.cord_x, a.cord_y),
                a.action_type == ActionType::CorrectGuess,
            );
        }
        if !reconciled.is_empty() {
            let mut merged = actions.peek().clone();
            merged.retain(|a| {
                !reconciled
                    .iter()
                    .any(|r| (r.cord_x, r.cord_y) == (a.cord_x, a.cord_y))
            });
            merged.extend(reconciled);
            actions.set(merged);
        }
    }

    let verdicts = if guess {
        let right = slots
            .iter()
            .filter(|s| from_reply.get(&(s.cord_x, s.cord_y)) == Some(&true))
            .count();
        Some((right, slots.len()))
    } else {
        None
    };
    sync.set(SyncState::Saved(verdicts));
    failed_batch.set(None);
    // The saved line answers "did that land?" — once answered it is history,
    // and the board already shows the letters. Two and a half seconds is
    // about how long it takes to glance at it after clicking Guess.
    let mut sync_clear = sync;
    gloo_timers::callback::Timeout::new(2_500, move || sync_clear.set(SyncState::Idle)).forget();
    Some((res, from_reply))
}

/// Parse the `gameMembers` array from `activeGame.get`. `userName` is absent
/// on pre-coop backends — fall back to the leaderboard's placeholder.
fn parse_members(data: &Value) -> Vec<MemberInfo> {
    data.get("gameMembers")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(MemberInfo {
                        user_id: v.get("userId")?.as_str()?.to_string(),
                        user_name: v
                            .get("userName")
                            .and_then(|n| n.as_str())
                            .unwrap_or("Anonymous Player")
                            .to_string(),
                        is_owner: v.get("isOwner").and_then(|b| b.as_bool()).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Pull the board's source of truth out of an `activeGame.get` response.
fn parse_board(data: &Value) -> (Vec<Question>, Vec<GameAction>) {
    let questions: Vec<Question> = data
        .get("game")
        .and_then(|g| g.get("questions"))
        .and_then(|q| serde_json::from_value(q.clone()).ok())
        .unwrap_or_default();
    let actions: Vec<GameAction> = data
        .get("actions")
        .and_then(|a| serde_json::from_value(a.clone()).ok())
        .unwrap_or_default();
    (questions, actions)
}

/// Should this connection-state transition trigger a reconcile?
///
/// Split out from the subscription setup so the *sequence* is testable without
/// a live socket — the bug this guards (DEF-235) was not one call behaving
/// wrongly, it was a whole class of reconnect never re-announcing presence.
///
/// The rule: every `Live` after the first one is a reconnect that has just
/// repaired its subscriptions, so it must reconcile. The first-ever `Live` is
/// deliberately skipped — there is nothing to reconcile, and re-fetching would
/// discard the optimistic local merge the player has not submitted yet.
/// Anything that is not `Live` (connecting, reconnecting, offline) does not
/// reconcile on its own.
fn is_reconcile_trigger(state: &net::ConnectionState, seen_live: &mut bool) -> bool {
    if state != &net::ConnectionState::Live {
        return false;
    }
    // `mem::replace` rather than `mem::take`: return the *prior* value, then
    // mark it seen. The first `Live` returns `false` (skip the redundant
    // reconcile), and every `Live` after it returns `true` — so a second `Live`
    // in the same tick is still a genuine reconnect.
    std::mem::replace(seen_live, true)
}

/// Replace `questions` / `actions` / `members` wholesale from `activeGame.get`,
/// then re-announce our own selection.
///
/// The reconcile step of a reconnect (DEF-175 §3). Every action taken while
/// the socket was down is missing from the local append-only `actions` list, and
/// nothing else can recover it — so the only correct repair is to re-read the
/// server's state and overwrite. All three are already signals, so this is a
/// `set`, not a re-render of the page.
///
/// The re-announce is the part that was missing (DEF-235). Presence is
/// ephemeral and fire-and-forget: the other players' picture of us lives only as
/// long as we keep publishing. A dropped socket takes every publish we made over
/// the dead connection with it, and nothing republished — so the partner kept
/// rendering us as "working on nothing" until we happened to select a *different*
/// clue. One reconnect was enough to make a live co-op player go invisible on
/// their partner's board, which is what `demo.spec.ts`'s presence-ring assertion
/// kept failing on with a 20s timeout and no ring anywhere in the DOM.
///
/// Republishing the current selection (including `None`) is the whole repair,
/// and it is safe to do here specifically: the reconnect that drove this call has
/// already re-established the subscription, so the broadcast lands on a live
/// socket rather than dying with the old one. `selected` is read after the fetch
/// so the announce reflects where the player is now, not where they were when
/// the socket dropped. It is the publisher's own `joined` guard (inside
/// `publish_presence`) that keeps a non-member from publishing.
fn reconcile_game(
    id: &str,
    questions: Signal<Vec<Question>>,
    actions: Signal<Vec<GameAction>>,
    members: Signal<Vec<MemberInfo>>,
    selected: Signal<Option<QKey>>,
    reannounce: impl Fn(Option<QKey>) + 'static,
) {
    let id = id.to_string();
    let mut questions = questions;
    let mut actions = actions;
    let mut members = members;
    spawn_local(async move {
        let Ok(data) = net::query("activeGame.get", Some(json!({ "id": id }))).await else {
            // Leave the board alone: a failed reconcile is strictly better
            // than an empty one, and the next reconnect will try again.
            return;
        };
        let (qs, acts) = parse_board(&data);
        questions.set(qs);
        actions.set(acts);
        members.set(parse_members(&data));
        reannounce(*selected.peek());
    });
}

/// Should stale presence be pruned right now?
///
/// The 45s TTL clears players who closed their tab without a goodbye. While the
/// socket is only reconnecting, that is exactly what it cannot tell apart from
/// "the network dropped" — the rings are stale but not absent, so they are kept
/// and rendered dimmed (DEF-175 §4). `Connecting` is grouped with
/// `Reconnecting` because it is the same situation one step earlier: nothing has
/// arrived yet to say anyone left. `Offline` does prune — with the connection
/// gone, a roster of ghosts is worse than an empty bar.
fn retains_presence(state: &net::ConnectionState) -> bool {
    matches!(
        state,
        net::ConnectionState::Connecting | net::ConnectionState::Reconnecting { .. }
    )
}

/// Is every cell of `word` known to hold the right letter?
///
/// `verdicts` maps `cord -> is that cell right`, and a cell with no entry is
/// *unknown*: an unknown cell makes the answer `false`, so a word is solved on
/// positive evidence only. That is the whole point — the client cannot read the
/// answer off its own letters, because DEF-243 moved the key to the server.
///
/// `word` is the snapshot of the open word taken when the guess was sent, not
/// the live editor state: the player can select another clue mid-flight, and the
/// reply belongs to the word it was asked about.
///
/// The reply covers `word` exactly. `build_action_batch` sends every slot of the
/// open word and `addActions` returns one row per submitted action, so the
/// verdicts for a guess are complete for that word and nothing else.
fn word_solved(word: &[(i32, i32)], verdicts: &HashMap<(i32, i32), bool>) -> bool {
    !word.is_empty()
        && word
            .iter()
            .all(|c| verdicts.get(c).copied().unwrap_or(false))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
enum PanelId {
    Board,
    Clues,
}

impl panel_kit::PanelKind for PanelId {
    fn title(self) -> &'static str {
        match self {
            PanelId::Board => "Board",
            PanelId::Clues => "Clues",
        }
    }
}

/// First-mount geometry, computed as viewport proportions. Only used when no
/// saved layout exists (fresh profile / incognito / CI) — panel-kit persists
/// to localStorage and re-scales floating geometry on window resize. Fixed
/// pixel defaults only ever fit the one canvas they were tuned on.
fn default_layout() -> Vec<panel_kit::PanelWin<PanelId>> {
    let (vw, vh) = viewport();
    const M: f64 = 16.0; // workspace margin / inter-panel gap
    const CHROME: f64 = 34.0; // panel border + the inset title row
    const PLAYERS_H: f64 = 46.0; // `.cw-players` strip above the board
    let left_w = (vw * 0.42).clamp(360.0, 900.0);
    // The grid is square — `.cw-board-area` centres it and caps it at the
    // smaller axis — so board height follows *width*. Taking a fraction of
    // the viewport instead just parked dead space above and below the grid.
    let board_h = (left_w + PLAYERS_H + CHROME).min(vh - 2.0 * M);
    let mut b = panel_kit::LayoutBuilder::new();
    vec![
        b.at(PanelId::Board, M, M, left_w, board_h),
        b.at(
            PanelId::Clues,
            2.0 * M + left_w,
            M,
            (vw - left_w - 3.0 * M).max(360.0),
            vh - 2.0 * M,
        ),
    ]
}

/// Live window size with a desktop-ish fallback (SSR/headless first tick).
fn viewport() -> (f64, f64) {
    web_sys::window()
        .and_then(|w| {
            Some((
                w.inner_width().ok()?.as_f64()?,
                w.inner_height().ok()?.as_f64()?,
            ))
        })
        .unwrap_or((1440.0, 900.0))
}

/// The play screen's pre-board state: one `activeGame.get` in flight.
///
/// `role`/`aria-busy` are set once, here, on a node that mounts once — see
/// [`PlayStatusError`] for why this is a component and not a branch.
#[component]
fn PlayStatusLoading() -> Element {
    rsx! {
        div { class: "gp-status",
            div { class: "app-card gp-status-card",
                role: "status",
                // Polite: nothing is wrong yet. One announcement, on appearance
                // — the card leaves the DOM when the board mounts, and a
                // removal from a live region is not announced, so there is no
                // churn behind the board.
                aria_live: "polite",
                aria_busy: "true",
                h1 { class: "gp-status-title", "Loading game…" }
                div { class: "gp-status-body",
                    // `fraction: None` is panel-kit's honest indeterminate state:
                    // one round trip reports no progress, so the bar animates
                    // and no percentage is fabricated. The reduced-motion guard
                    // is panel-kit's own (assets/panel-kit.css), which holds a
                    // static 40% sliver — a full-width bar would claim
                    // completion, which is a lie the user cannot see through.
                    ProgressBar { fraction: None, label: "Loading game…" }
                }
                // Always in the DOM, never in the tab order: `.gp-status-actions`
                // is hidden by the `[aria-hidden]` rule, and `visibility: hidden`
                // takes the link out of the focus order with it. That is what
                // keeps this card non-interactive AND reserves the 44px the
                // error state's recovery row needs, so the card cannot resize
                // when the state flips.
                div { class: "gp-status-actions", aria_hidden: "true",
                    Link { to: Route::Games {}, class: "app-btn", "Back to games" }
                }
            }
        }
    }
}

/// The play screen's pre-board failure. `message` is whatever `activeGame.get`
/// reported — "Game not found", or a mapped transport status (net.rs:49).
///
/// A component rather than a second branch of `GamePlay` for one reason: the
/// role has to be SET ONCE, ON MOUNT. Dioxus patches a same-shaped subtree, so
/// two branches returning the same element skeleton would swap `role="status"`
/// for `role="alert"` in place, and DEF-183 D2 measured that swapping a live
/// region's role re-announces it. Two component types at one position in the
/// tree means the old card unmounts and this one mounts.
///
/// Focus deliberately does NOT move here. The user may be mid-keyboard-
/// navigation when a slow request fails, and yanking focus on an async
/// page-level error is hostile; the alert role announces it instead. The card
/// has exactly one tab stop, the recovery link.
#[component]
fn PlayStatusError(message: String) -> Element {
    rsx! {
        div { class: "gp-status",
            div { class: "app-card gp-status-card",
                // Assertive: this one is worth interrupting for.
                role: "alert",
                aria_busy: "false",
                h1 { class: "gp-status-title", "Couldn't load game" }
                div { class: "gp-status-body",
                    // `--text-secondary`, not `--color-error`: the heading
                    // already says what happened and the alert role carries the
                    // urgency, so the body is the same 6.9:1 dark / 7.2:1 light
                    // secondary ink as every other dim line in the app.
                    p { class: "gp-status-detail", "{message}" }
                }
                div { class: "gp-status-actions",
                    // `.app-btn`, whose border is --text-secondary (6.9:1 dark /
                    // 7.2:1 light) after the DEF-188 D1 fix — a control boundary
                    // needs 3:1 and --border-app is 1.19:1 on the dark card.
                    Link { to: Route::Games {}, class: "app-btn", "Back to games" }
                }
            }
        }
    }
}

#[component]
pub fn GamePlay(id: String) -> Element {
    // --- source-of-truth state ---
    let questions = use_signal(Vec::<Question>::new);
    let actions = use_signal(Vec::<GameAction>::new);
    let loading = use_signal(|| true);
    let load_error = use_signal(|| Option::<String>::None);

    // selection / input state
    let mut selected = use_signal(|| Option::<QKey>::None);
    let mut selected_direction = use_signal(|| Option::<Direction>::Some(Direction::Across));
    let mut game_action_data = use_signal(Vec::<ActionSlot>::new);
    let mut focused_index = use_signal(|| Option::<usize>::None);
    // Which surface owns DOM focus: the grid or the inline clue editor. Both
    // focus drivers key off `focused_index`, and both move that cursor, so
    // without this split an arrow press on the grid would yank focus into the
    // Clues panel (the board lives inside the workspace, so the editor is a
    // sibling surface, not an ancestor).
    let mut board_active = use_signal(|| false);
    // Mounted handles for the grid cells, so the roving cell can take DOM
    // focus when the cursor moves. Deliberately NOT a signal: one cell mounts
    // per grid square, and writing 225 of them reactively on every render is
    // pure churn. Nothing renders from this map; it is only read by the focus
    // effect below.
    let cell_nodes =
        use_hook(|| Rc::new(RefCell::new(HashMap::<(i32, i32), Rc<MountedData>>::new())));

    // co-op state: roster, live remote selections, join/invite UI
    let state = use_app_state();
    let mut members = use_signal(Vec::<MemberInfo>::new);
    let presence = use_signal(HashMap::<String, PresenceEntry>::new);
    let clock = use_signal(|| 0u64);
    let mut invite_copied = use_signal(|| false);
    let mut joining = use_signal(|| false);
    let mut join_error = use_signal(String::new);

    // One ordered, coalescing writer for ephemeral presence state.
    //
    // Two independent fire-and-forget mutations originally raced: a direction
    // toggle queued clear(None), then clue selection queued focus(Some), and a
    // stale clear could arrive last. Serialising removed the reorder, but
    // replaying every historical focus/clear after 24 UI solves built a queue
    // of stale states; the final focus could sit behind them for 90s.
    //
    // Presence is STATE, not an audit log. Drain everything already pending
    // and send only the newest state. A clear immediately followed by focus
    // becomes focus; a user who genuinely clears and stops still sends clear.
    // One request remains in flight at a time, so wire order stays defined.
    let presence_tx = use_hook(|| {
        let (tx, mut rx) = mpsc::unbounded::<Value>();
        spawn_local(async move {
            while let Some(mut latest) = rx.next().await {
                // VUs/components share no state, but one component owns this
                // receiver; draining here is the single coalescing point.
                while let Ok(Some(newer)) = rx.try_next() {
                    latest = newer;
                }
                let _ = net::mutation("activeGame.publishPresence", Some(latest)).await;
            }
        });
        tx
    });

    // Broadcast our selection to the other members (no-op until we join).
    //
    // Hoisted above the load/subscribe block because the reconnect reconcile
    // needs it: presence is fire-and-forget, so a socket that dropped and came
    // back has to re-announce or the other player renders us as idle for the
    // rest of the session (DEF-235). The send is ordered/coalesced by the one
    // presence writer above, so a stale clear cannot overtake a newer focus.
    let id_for_presence = id.clone();
    let presence_tx_for_publish = presence_tx.clone();
    let publish_presence = move |selection: Option<QKey>| {
        let joined = state
            .user()
            .map(|u| members.peek().iter().any(|m| m.user_id == u.id))
            .unwrap_or(false);
        if !joined {
            return;
        }
        let input = match selection {
            Some((n, d)) => json!({ "id": id_for_presence, "number": n, "direction": dir_str(d) }),
            None => json!({ "id": id_for_presence, "number": null }),
        };
        let _ = presence_tx_for_publish.unbounded_send(input);
    };
    // Same publisher under its own handle, for the reconnect reconcile.
    let publish_for_reconcile = publish_presence.clone();

    // per-letter input mount handles, so we can drive focus without web-sys.
    let mut input_refs = use_signal(Vec::<Option<Rc<MountedData>>>::new);

    // Write feedback for the last placeholder save / guess (audit P1), plus
    // the batch a failed write kept so the Retry control can resend it.
    let sync = use_signal(|| SyncState::Idle);
    let failed_batch = use_signal(|| Option::<(Vec<ActionSlot>, bool)>::None);

    // keep subscription handles alive for the component lifetime.
    let _subs =
        use_signal(|| Option::<(net::Subscription, net::Subscription, net::Subscription)>::None);

    // Completion redirect, staged through a signal: the onGameCompleted
    // subscription callback runs on a raw wasm task (no Dioxus runtime scope),
    // so it must not call `nav.push` itself — the router couldn't resolve its
    // history context and would panic. The effect below navigates in-runtime.
    let mut completed_redirect = use_signal(|| Option::<String>::None);

    // Connection state for the co-op socket (DEF-175). One state for the whole
    // screen, not one per subscription: all three emitters share it, so they
    // cannot render three disagreeing badges. Taken here, in a `use_hook`, so
    // the root-scoped signal behind it is created while a Dioxus runtime is
    // live rather than from a bare task.
    let conn = use_hook(net::connection);

    let id_for_load = id.clone();

    // --- load + subscribe (once) ---
    let conn_for_load = conn.clone();
    use_hook(move || {
        let mut questions = questions;
        let mut actions = actions;
        let mut loading = loading;
        let mut load_error = load_error;
        let mut subs = _subs;
        let mut members = members;
        let mut presence = presence;
        let mut clock = clock;
        let id = id_for_load.clone();
        spawn_local(async move {
            match net::query("activeGame.get", Some(json!({ "id": id }))).await {
                Ok(Value::Null) => {
                    load_error.set(Some("Game not found".into()));
                    loading.set(false);
                }
                Ok(data) => {
                    let (qs, acts) = parse_board(&data);
                    questions.set(qs);
                    actions.set(acts);
                    members.set(parse_members(&data));
                    loading.set(false);
                }
                Err(e) => {
                    load_error.set(Some(e));
                    loading.set(false);
                }
            }
        });

        // Every subscription reports into the same connection state, and the
        // first `Live` after a drop is where a dropped socket gets repaired.
        //
        // ORDERING IS LOAD-BEARING (DEF-175 §3). `net::supervise` reports
        // `Live` only after the re-subscription's start frame has landed, so by
        // the time this runs the sockets are already live again and the fetch
        // below writes a superset of anything they delivered — overwriting
        // cannot lose an action. Fetch-then-subscribe would drop every action
        // taken in the gap between the two, which is the same class of bug as
        // the `booted` latch in DEF-148.
        //
        // The first-ever `Live` is deliberately skipped: there is nothing to
        // reconcile, and re-fetching would discard the optimistic local merge
        // the player has not submitted yet.
        let seen_live = Rc::new(RefCell::new(false));
        let on_state = Rc::new(RefCell::new({
            let questions = questions;
            let actions = actions;
            let members = members;
            let seen_live = seen_live.clone();
            let id = id_for_load.clone();
            move |s: net::ConnectionState| {
                if !is_reconcile_trigger(&s, &mut seen_live.borrow_mut()) {
                    return;
                }
                reconcile_game(
                    &id,
                    questions,
                    actions,
                    members,
                    selected,
                    publish_for_reconcile.clone(),
                );
            }
        }));
        let on_state_for = move |s: net::ConnectionState| on_state.borrow_mut()(s);

        // Subscriptions: both emitters are global (no input). We filter by
        // activeGameId off the raw JSON before merging / navigating.
        let id_actions = id_for_load.clone();
        let on_actions = net::subscribe(
            "activeGame.onAddActions",
            None,
            move |data: Value| {
                // payload is an array of GameAction; each carries activeGameId.
                let arr = match data.as_array() {
                    Some(a) => a,
                    None => return,
                };
                let mut incoming: Vec<GameAction> = Vec::new();
                for v in arr {
                    let belongs = v
                        .get("activeGameId")
                        .and_then(|x| x.as_str())
                        .map(|s| s == id_actions)
                        .unwrap_or(true);
                    if !belongs {
                        continue;
                    }
                    if let Ok(a) = serde_json::from_value::<GameAction>(v.clone()) {
                        incoming.push(a);
                    }
                }
                if !incoming.is_empty() {
                    // Stagger application so remote letters land one-by-one (≈90ms
                    // apart) instead of the whole word flashing in at once. The
                    // viewer sees the other player "typing" even though the wire
                    // protocol only carries the submitted guess.
                    //
                    // Deliberately NOT used on the reconcile path: there the whole
                    // list is replaced at once, so the board snaps to correct
                    // instead of replaying the outage. DEF-175 §3.
                    let mut actions_sig = actions.clone();
                    spawn_local(async move {
                        for (i, a) in incoming.into_iter().enumerate() {
                            if i > 0 {
                                TimeoutFuture::new(90).await;
                            }
                            let mut cur = actions_sig.peek().clone();
                            cur.push(a);
                            actions_sig.set(cur);
                        }
                    });
                }
            },
            on_state_for.clone(),
        );

        let id_done = id_for_load.clone();
        let on_done = net::subscribe(
            "activeGame.onGameCompleted",
            None,
            move |data: Value| {
                let active = data.get("activeGameId").and_then(|x| x.as_str());
                let completed = data.get("completedGameId").and_then(|x| x.as_str());
                if active == Some(id_done.as_str()) {
                    if let Some(cid) = completed {
                        // Idempotent on purpose. A reconnect replays the
                        // completion event, and `completed_redirect` drives a
                        // navigation — setting it twice would double-navigate.
                        // DEF-175 §3.
                        if completed_redirect.peek().is_none() {
                            completed_redirect.set(Some(cid.to_string()));
                        }
                    }
                }
            },
            on_state_for.clone(),
        );

        // Presence: other members' live clue selections. Global emitter like
        // the others — filter by activeGameId (and drop our own echoes).
        let id_pres = id_for_load.clone();
        let on_presence = net::subscribe(
            "activeGame.onPresence",
            None,
            move |data: Value| {
                if data.get("activeGameId").and_then(|x| x.as_str()) != Some(id_pres.as_str()) {
                    return;
                }
                let uid = match data.get("userId").and_then(|x| x.as_str()) {
                    Some(u) => u.to_string(),
                    None => return,
                };
                if state.user().map(|u| u.id == uid).unwrap_or(false) {
                    return;
                }
                let selection = match (
                    data.get("number").and_then(|v| v.as_i64()),
                    data.get("direction")
                        .and_then(|x| x.as_str())
                        .and_then(direction_from_str),
                ) {
                    (Some(n), Some(d)) => Some((n as i32, d)),
                    _ => None,
                };
                let name = data
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("Anonymous Player")
                    .to_string();
                let sequence = data.get("sequence").and_then(|x| x.as_i64()).unwrap_or(0);
                if presence
                    .peek()
                    .get(&uid)
                    .map(|e| sequence < e.sequence)
                    .unwrap_or(false)
                {
                    return;
                }
                presence.write().insert(
                    uid,
                    PresenceEntry {
                        name,
                        selection,
                        tick: *clock.peek(),
                        sequence,
                    },
                );
            },
            on_state_for,
        );

        // Prune stale presence every 5s — a closed tab never sends a clear.
        let conn_for_prune = conn_for_load.clone();
        spawn(async move {
            let mut ticks = IntervalStream::new(5_000);
            while ticks.next().await.is_some() {
                let now = *clock.peek() + 5;
                clock.set(now);
                // Do NOT prune while the socket is merely reconnecting
                // (DEF-175 §4). The TTL exists to clear players who closed
                // their tab without a goodbye; during a reconnect these rings
                // are last-known rather than absent, and dropping them makes a
                // network blip read as "everyone left the game". The board
                // renders them dimmed instead.
                if retains_presence(&conn_for_prune.state()) {
                    continue;
                }
                let any_stale = presence
                    .peek()
                    .values()
                    .any(|e| now.saturating_sub(e.tick) > PRESENCE_TTL_SECS);
                if any_stale {
                    presence
                        .write()
                        .retain(|_, e| now.saturating_sub(e.tick) <= PRESENCE_TTL_SECS);
                }
            }
        });

        subs.set(Some((on_actions, on_done, on_presence)));
    });

    // Perform the completion redirect inside the runtime (see the signal's
    // declaration for why the subscription callback can't push directly).
    use_effect(move || {
        if let Some(cid) = completed_redirect.read().clone() {
            navigator().push(Route::GameCompleted { id: cid });
        }
    });

    // --- derived state (recomputes when questions/actions change) ---
    let answer_maps = use_memo(move || {
        let acts = actions.read();
        let maps: Vec<QuestionWithAnswerMap> = questions
            .read()
            .iter()
            .map(|q| compute_answer_map(q, &acts))
            .collect();
        Rc::new(maps)
    });

    let board = use_memo(move || {
        let maps = answer_maps.read().clone();
        let size = compute_board_size(&maps);
        let acts = actions.read();
        let grid = board_state_from_actions(size, &acts, &maps);
        Rc::new((size, grid))
    });

    // The single cell that carries `tabindex="0"`. Normally that is the
    // derived cursor (`focused_coord`); before a clue has been picked there is
    // no cursor, so the roving tabindex parks on the first non-block cell in
    // row-major order — which gives Tab exactly one stop to land on, and gives
    // the first keypress somewhere to seed from.
    let roving_coord = move || -> Option<(i32, i32)> {
        let maps = answer_maps.read();
        if let (Some(k), Some(i)) = (*selected.peek(), *focused_index.peek()) {
            return maps
                .iter()
                .find(|m| qkey(&m.question) == k)
                .and_then(|m| m.answer_map.get(i))
                .map(|c| (c.cord_x, c.cord_y));
        }
        drop(maps);
        first_playable(&board.read().1)
    };

    // filtered (across/down/all) clue list, by the toggle
    let filtered: Vec<QuestionWithAnswerMap> = {
        let maps = answer_maps.read();
        let dir = *selected_direction.read();
        maps.iter()
            .filter(|m| dir.map(|d| m.question.direction == d).unwrap_or(true))
            .cloned()
            .collect()
    };

    // --- focus driver: focus the input matching focused_index ---
    //     (keeping the board on screen while typing is CSS — see the
    //     scroll-margin-top rule in GAME_CSS.)
    //
    //     `board_active` is read with `peek`, never `read`: this effect must
    //     depend on the cursor alone. Subscribing to the focus surface would
    //     re-run it on every grid/editor handoff and hand focus to the editor
    //     precisely when the player is on the grid.
    use_effect(move || {
        let idx = *focused_index.read();
        let refs = input_refs.read();
        if let Some(i) = idx {
            if let Some(Some(node)) = refs.get(i) {
                let node = node.clone();
                spawn_local(async move {
                    if !*board_active.peek() {
                        let _ = node.set_focus(true).await;
                    }
                });
            }
        }
    });

    // --- focus driver: keep DOM focus on the roving grid cell ---
    //     `roving_coord` is the same cell that gets `.cw-focused`, so the
    //     yellow cursor and the focus ring can never disagree. Read with
    //     `peek` for the same reason as above.
    let cell_nodes_focus = cell_nodes.clone();
    use_effect(move || {
        if !*board_active.peek() {
            return;
        }
        let Some((x, y)) = roving_coord() else { return };
        let node = cell_nodes_focus.borrow().get(&(x, y)).cloned();
        if let Some(node) = node {
            spawn_local(async move {
                let _ = node.set_focus(true).await;
            });
        }
    });

    // --- selection helpers ---------------------------------------------------

    // Refresh the current focus before its 45s TTL expires.
    //
    // Presence is ephemeral and intentionally not persisted. One missed
    // websocket delta therefore leaves a peer missing forever if the player
    // keeps working the same clue: no selection change produces another event.
    // A 10s heartbeat makes the latest state self-healing (and repairs a
    // reconnect that missed the original focus) without turning presence into
    // durable history. The coalescing writer ensures a clear still wins — this
    // only sends while a selection actually exists.
    let heartbeat_tx = presence_tx.clone();
    let heartbeat_id = id.clone();
    let heartbeat_selected = selected;
    use_hook(move || {
        spawn_local(async move {
            let mut ticks = IntervalStream::new(10_000);
            while ticks.next().await.is_some() {
                if let Some((number, direction)) = *heartbeat_selected.peek() {
                    let _ = heartbeat_tx.unbounded_send(json!({
                        "id": heartbeat_id,
                        "number": number,
                        "direction": dir_str(direction),
                    }));
                }
            }
        });
    });
    // Snapshot the in-progress word for a question, pre-filling current letters.
    let publish_for_select = publish_presence.clone();
    let select_question = move |key: QKey| {
        let maps = answer_maps.peek();
        if let Some(m) = maps.iter().find(|m| qkey(&m.question) == key) {
            let slots: Vec<ActionSlot> = m
                .answer_map
                .iter()
                .map(|cell| {
                    let cur = cell
                        .modifications
                        .first()
                        .map(|md| md.state.clone())
                        .unwrap_or_default();
                    ActionSlot {
                        cord_x: cell.cord_x,
                        cord_y: cell.cord_y,
                        previous_state: cur.clone(),
                        state: cur,
                    }
                })
                .collect();
            let n = slots.len();
            selected_direction.set(Some(m.question.direction));
            game_action_data.set(slots);
            input_refs.set(vec![None; n]);
            selected.set(Some(key));
            focused_index.set(Some(0));
            publish_for_select(Some(key));
        }
    };

    // Submit the current word as a placeholder save (used by unselect).
    // Write feedback, retry batch and the optimistic local letters live in
    // `commit_actions` — shared with the guess path.
    let id_for_save = id.clone();
    let submit_placeholder = move || {
        // Only persist letters the player actually changed. Saving the whole
        // prefilled word would restamp every cell — including correct guesses
        // — as a placeholder, hiding them behind placeholder styling.
        let slots: Vec<ActionSlot> = game_action_data
            .peek()
            .iter()
            .filter(|s| s.state != s.previous_state)
            .cloned()
            .collect();
        if slots.is_empty() {
            return;
        }
        // commit_actions already merges the optimistic local letters. The id
        // is cloned into a local: an `async move` block that borrows a
        // capture moves it, which would make this closure FnOnce and break
        // every caller that unselects twice.
        let game_id = id_for_save.clone();
        spawn_local(async move {
            commit_actions(&game_id, &slots, false, actions, sync, failed_batch).await;
        });
    };

    // Clear the active selection: drop the in-progress word and stop
    // broadcasting presence. `save_progress` first persists the typed letters
    // as a placeholder (unselect / direction-toggle); a correct guess already
    // persisted its letters, so it clears without saving.
    let submit_placeholder_for_clear = submit_placeholder.clone();
    let publish_for_clear = publish_presence.clone();
    let clear_selection = move |save_progress: bool| {
        let was_selected = selected.peek().is_some();
        let any_typed = game_action_data.peek().iter().any(|s| !s.state.is_empty());
        if save_progress && was_selected && any_typed {
            submit_placeholder_for_clear();
        }
        selected.set(None);
        game_action_data.set(Vec::new());
        focused_index.set(None);
        if was_selected {
            publish_for_clear(None);
        }
    };

    let mut clear_for_unselect = clear_selection.clone();
    let unselect = move |_| clear_for_unselect(true);

    // Click a board cell → select a covering question (current dir first).
    let mut select_question_for_coords = select_question.clone();
    let select_coordinates = move |x: i32, y: i32| {
        let maps = answer_maps.peek();
        let dir = *selected_direction.peek();
        let covers =
            |m: &QuestionWithAnswerMap| m.answer_map.iter().any(|c| c.cord_x == x && c.cord_y == y);
        let found = maps
            .iter()
            .find(|m| dir.map(|d| m.question.direction == d).unwrap_or(false) && covers(m))
            .or_else(|| maps.iter().find(|m| covers(m)));
        if let Some(m) = found {
            let key = qkey(&m.question);
            select_question_for_coords(key);
        }
    };

    // --- draft letter write, shared by the inline editor and the board --------
    // The single place a letter enters the local draft. Both the editor's
    // `onchange` and a board keystroke go through it, so "type a letter" means
    // the same thing (same casing, same advance) on either surface. Still a
    // local draft: Guess is the only thing that commits.
    let write_letter_for_editor = move |index: usize, raw: String| {
        let val = raw
            .chars()
            .last()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();
        {
            let mut g = game_action_data.write();
            if let Some(slot) = g.get_mut(index) {
                slot.state = val.clone();
            }
        }
        let len = game_action_data.peek().len();
        if !val.is_empty() && index + 1 < len {
            focused_index.set(Some(index + 1));
        }
    };
    let handle_letter_input = write_letter_for_editor.clone();
    let mut write_letter = write_letter_for_editor;

    // --- board keyboard layer ------------------------------------------------
    // A thin input surface over the selection model that already exists, so
    // there is still exactly one cursor (the `focused_coord` `render_board`
    // derives) and no parallel state store to drift from it.
    //
    // Three things this has to get right that a click does not:
    //
    //  1. The keyboard resolves a target cell to a `(clue, index)` pair with
    //     `select_at`, NOT with `select_coordinates` — see there. A click is
    //     allowed to land on the word start; an arrow press is not.
    //  2. `select_question` always starts a word at index 0, so every path
    //     that crosses into another word re-points `focused_index` at the cell
    //     actually moved to.
    //  3. panel-kit's workspace handler owns bare arrows (pan the window) and
    //     Tab (move panel focus), and the grid sits INSIDE that workspace. So
    //     every branch below stops propagation; without it, each cursor move
    //     would also pan the workspace.

    // The cell the cursor is on, or None before a clue is picked.
    let cursor_coord = move || -> Option<(i32, i32)> {
        let maps = answer_maps.peek();
        match (*selected.peek(), *focused_index.peek()) {
            (Some(k), Some(i)) => maps
                .iter()
                .find(|m| qkey(&m.question) == k)
                .and_then(|m| m.answer_map.get(i))
                .map(|c| (c.cord_x, c.cord_y)),
            _ => None,
        }
    };

    // Coordinate -> (clue, index) resolver, used ONLY by the keyboard layer.
    //
    // It cannot go through `select_coordinates`: that calls `select_question`,
    // which unconditionally sets `focused_index = Some(0)`, so every cursor
    // move would land on the FIRST letter of whatever word covers the target
    // and arrows would be unusable. (The click path has the same quirk — a
    // click lands on the word start, not the clicked cell — but that is
    // pre-existing and `select_question`/`select_coordinates` are left
    // byte-identical here, per spec.)
    //
    // Covering-clue preference is deliberately the same order
    // `select_coordinates` uses — the working direction first, then any
    // direction — so a keyboard move into a crossing cell picks the same clue a
    // click on that cell would.
    let mut select_question_for_at = select_question.clone();
    let select_at = move |x: i32, y: i32| {
        let maps = answer_maps.peek();
        let dir = *selected_direction.peek();
        let covers =
            |m: &QuestionWithAnswerMap| m.answer_map.iter().any(|c| c.cord_x == x && c.cord_y == y);
        let found = maps
            .iter()
            .find(|m| dir.map(|d| m.question.direction == d).unwrap_or(false) && covers(m))
            .or_else(|| maps.iter().find(|m| covers(m)));
        let Some(m) = found else { return };
        let key = qkey(&m.question);
        let Some(idx) = m
            .answer_map
            .iter()
            .position(|c| c.cord_x == x && c.cord_y == y)
        else {
            return;
        };
        if *selected.peek() == Some(key) {
            // Already on this word: just slide the cursor. Going through
            // `select_question` again would throw the in-progress draft away
            // for nothing.
            focused_index.set(Some(idx));
            return;
        }
        drop(maps);
        select_question_for_at(key);
        // `select_question` parks the cursor at index 0; the arrow is asking
        // for THIS cell, so override it.
        focused_index.set(Some(idx));
    };

    // `select_at` is a closure over another closure, so it is `Clone` but not
    // `Copy`. Hand every consumer below its own copy, otherwise a `move`
    // capture gives it to exactly one of them and the rest fail to compile.
    let mut sa_focus = select_at.clone();
    let mut sa_seed = select_at.clone();
    let mut sa_switch = select_at.clone();

    // Point the cursor at a cell, the way an arrow press means it: land on the
    // cell itself, crossing into the covering word when the target is outside
    // the one being typed.
    let mut focus_coord = move |x: i32, y: i32| sa_focus(x, y);

    // First non-block cell in row-major order, selected as a clue. This is the
    // "no clue picked yet" entry point for Tab and for the first keystroke.
    let seed_cursor = move || {
        if let Some((x, y)) = first_playable(&board.peek().1) {
            sa_seed(x, y);
        }
    };

    // Arrow/letter/space all need to seed; each gets its own copy for the
    // same reason as above.
    let mut seed_for_arrows = seed_cursor.clone();
    let mut seed_for_keys = seed_cursor.clone();
    let mut seed_for_focus = seed_cursor.clone();

    // Arrows step along the ray until they land on a playable cell, so a block
    // is skipped rather than swallowed. A blocked first step means the cursor
    // is at the edge in that direction: no-op.
    //
    // With no cursor yet there is nothing to step FROM, so seed on the first
    // playable cell and then apply the key to it — an arrow on an unseeded
    // board should still end up on a cell, not swallow the press. Seeding
    // always lands on `focused_coord` afterwards, so this only ever happens
    // once per puzzle.
    let mut move_cursor = move |dx: i32, dy: i32| {
        let from = match cursor_coord() {
            Some(f) => f,
            None => {
                seed_for_arrows();
                // Seeding can still be a no-op on an empty/all-block board.
                let Some(f) = cursor_coord() else { return };
                f
            }
        };
        if let Some(target) = step_to_playable(&board.peek().1, from, dx, dy) {
            focus_coord(target.0, target.1);
        }
    };

    // Next/previous clue in the working direction, wrapping. Tab is trapped in
    // the grid on purpose (see the Escape branch), so this is the only way to
    // walk the puzzle from the keyboard.
    //
    // `select_question` already leaves `focused_index` at 0, which is exactly
    // where a click on that clue row puts it. There is deliberately no
    // "first empty slot" seek and no draft save: the spec asks for parity with
    // the mouse path, and the mouse path does neither.
    let mut select_for_step = select_question.clone();
    let mut step_clue = move |forward: bool| {
        let dir = selected_direction.peek().unwrap_or(Direction::Across);
        let keys: Vec<QKey> = answer_maps
            .peek()
            .iter()
            .filter(|m| m.question.direction == dir)
            .map(|m| qkey(&m.question))
            .collect();
        if keys.is_empty() {
            return;
        }
        let pos = selected
            .peek()
            .and_then(|k| keys.iter().position(|c| *c == k));
        let next = match pos {
            Some(p) if forward => (p + 1) % keys.len(),
            Some(p) => (p + keys.len() - 1) % keys.len(),
            None if forward => 0,
            None => keys.len() - 1,
        };
        select_for_step(keys[next]);
    };

    // Space switches to the word running the OTHER way through the cell under
    // the cursor, and leaves the cursor on that same cell (its index in the new
    // word) — the behaviour a click on that clue row produces, including the
    // consequence that the outgoing word's unsaved draft is dropped.
    //
    // Deliberately NO save here. `clear_selection(true)` would persist the
    // typed letters as a placeholder, which is what the mouse's direction TAB
    // does; but the mouse's own cross-word click does not save, and the spec
    // asks for parity with that path. Saving here would make Space the one
    // keyboard action that quietly writes to the server.
    let mut switch_direction = move |new_dir: Direction| {
        let Some((x, y)) = cursor_coord() else {
            // No cursor to switch around: flip the clue-list filter, which is
            // all the signal means on its own.
            selected_direction.set(Some(new_dir));
            return;
        };
        let maps = answer_maps.peek();
        let covering = maps
            .iter()
            .find(|m| m.question.direction == new_dir && covers_cell(m, x, y));
        let Some(m) = covering else {
            // No word crosses this cell in the other direction — no-op, and the
            // cursor stays put.
            return;
        };
        let key = qkey(&m.question);
        let Some(idx) = m
            .answer_map
            .iter()
            .position(|c| c.cord_x == x && c.cord_y == y)
        else {
            return;
        };
        drop(maps);
        if *selected.peek() == Some(key) {
            focused_index.set(Some(idx));
            return;
        }
        sa_switch(x, y);
        focused_index.set(Some(idx));
    };

    // Consume a key the grid has handled. `prevent_default` stops the browser's
    // own behaviour (Tab focus movement, Space scrolling the page) and
    // `stop_propagation` stops panel-kit's workspace handler from also panning
    // or moving panel focus.
    let absorb = |e: &KeyboardEvent| {
        e.prevent_default();
        e.stop_propagation();
    };

    let mut clear_for_escape = clear_selection.clone();
    let handle_board_key = move |e: KeyboardEvent| {
        // The clue editor owns Backspace and the arrows while it has focus, and
        // a letter typed into a letter box must not also move the grid cursor.
        // Same guard workspace.rs:140 uses.
        if panel_kit::input::is_editing() {
            return;
        }
        let key = e.key();
        match key {
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                let (dx, dy) = match key {
                    Key::ArrowLeft => (-1, 0),
                    Key::ArrowRight => (1, 0),
                    Key::ArrowUp => (0, -1),
                    _ => (0, 1),
                };
                move_cursor(dx, dy);
                absorb(&e);
            }
            Key::Backspace => {
                // Same two-step as the editor's own Backspace: clear the cell
                // under the cursor, and if it was already empty step back and
                // clear that. An answer map is a run of non-block cells, so
                // index-1 IS the previous cell along the direction.
                let cursor_at = *focused_index.peek();
                if let Some(i) = cursor_at {
                    let cur_empty = game_action_data
                        .peek()
                        .get(i)
                        .map(|s| s.state.trim().is_empty())
                        .unwrap_or(true);
                    let target = if cur_empty && i > 0 { i - 1 } else { i };
                    let mut g = game_action_data.write();
                    if let Some(s) = g.get_mut(target) {
                        s.state = String::new();
                    }
                    drop(g);
                    if target != i {
                        focused_index.set(Some(target));
                    }
                }
                absorb(&e);
            }
            Key::Character(c) if c == " " => {
                let next = match *selected_direction.peek() {
                    Some(Direction::Across) => Direction::Down,
                    _ => Direction::Across,
                };
                switch_direction(next);
                absorb(&e);
            }
            Key::Character(c) if c.chars().any(|ch| ch.is_ascii_alphabetic()) => {
                // Seed before applying, so a letter typed before any click
                // still lands somewhere instead of vanishing.
                if cursor_coord().is_none() {
                    seed_for_keys();
                }
                let ch = c
                    .chars()
                    .find(|ch| ch.is_ascii_alphabetic())
                    .unwrap_or(' ')
                    .to_ascii_uppercase();
                // The SAME local-draft write the editor's onchange makes, so
                // the grid and the inline editor cannot drift in what a
                // keystroke means. It also does the advance (next index in
                // range), which is what a player expects from a crossword
                // rather than a "seek the next empty slot" jump.
                if let Some(i) = *focused_index.peek() {
                    write_letter(i, ch.to_string());
                }
                absorb(&e);
            }
            Key::Tab => {
                step_clue(!e.modifiers().shift());
                absorb(&e);
            }
            // Escape is the way out of the grid. Tab and Shift+Tab both stay
            // inside it (a crossword has no natural "next stop"), so without
            // this the grid would be a keyboard trap — and the clue editor has
            // been advertising "ESC to clear" on its Cancel button all along.
            Key::Escape => {
                clear_for_escape(true);
                board_active.set(false);
                absorb(&e);
            }
            // Reserved: Enter is not bound in this increment.
            _ => {}
        }
    };

    // A cell taking focus is what marks the grid as the active surface, and is
    // also the moment to seed a cursor that does not exist yet.
    let on_cell_focus = move || {
        board_active.set(true);
        if cursor_coord().is_none() {
            seed_for_focus();
        }
    };

    // Direction toggles (click active → null = show all).
    let mut clear_for_toggle = clear_selection.clone();
    let toggle_dir = move |d: Direction| {
        let cur = *selected_direction.peek();
        if cur == Some(d) {
            selected_direction.set(None);
        } else {
            // unselect (saving progress) then set the new filter
            clear_for_toggle(true);
            selected_direction.set(Some(d));
        }
    };

    // --- guess submission ----------------------------------------------------
    let id_for_guess = id.clone();
    let submit_guess = move || {
        let slots = game_action_data.peek().clone();
        if slots.is_empty() {
            return;
        }
        // DEF-243: the client no longer holds the answer, so it cannot decide
        // whether this word is right or whether the grid is finished. It sends
        // the letters and lets the server say: `addActions` returns each
        // action with the verdict *it* derived, plus the grid's real
        // `solved`/`correct`/`total`. `commit_actions` owns that exchange —
        // optimistic local write, sync feedback, retry batch, verdict merge —
        // and hands back the reply plus this word's verdict map.
        let id_complete = id_for_guess.clone();
        let mut clear_for_guess = clear_selection.clone();
        // The word this guess is about, snapshotted now. A `QKey` would be
        // wrong: the player can select another clue while the request is in
        // flight, and the reply belongs to the word it was asked about.
        let word_for_reply: Vec<(i32, i32)> = game_action_data
            .peek()
            .iter()
            .map(|s| (s.cord_x, s.cord_y))
            .collect();
        let nav = navigator();
        let game_id = id_for_guess.clone();
        // Dioxus `spawn`: `nav.push` below needs the runtime scope (raw
        // spawn_local panics resolving the history context).
        spawn(async move {
            let Some((res, from_reply)) =
                commit_actions(&game_id, &slots, true, actions, sync, failed_batch).await
            else {
                return;
            };
            // A right guess ends the word, so the editor closes. This is what
            // `a46ff5c` (DEF-243) dropped: it computed `is_correct` against an
            // answer the client was still being sent, and when the answer moved
            // to the server the clear went with it and nothing replaced it. A
            // correct word in a puzzle with others left therefore kept its
            // entry row, and the letters the player had just submitted stayed
            // editable over the now-green cells. Only the server's verdict can
            // say this.
            if word_solved(&word_for_reply, &from_reply) {
                clear_for_guess(false);
            }
            if !res.get("solved").and_then(|v| v.as_bool()).unwrap_or(false) {
                return;
            }
            // The grid is genuinely finished. The server has already refused an
            // unsolved `complete`, so this is the only path that navigates.
            if let Ok(done) =
                net::mutation("activeGame.complete", Some(json!({ "id": id_complete }))).await
            {
                if let Some(cid) = done.get("id").and_then(|x| x.as_str()) {
                    clear_for_guess(false);
                    nav.push(Route::GameCompleted {
                        id: cid.to_string(),
                    });
                }
            }
        });
    };

    // Retry the last failed write. The batch remembers its intent so a failed
    // GUESS retries as a guess (with the verdict line), not a silent save.
    let id_for_retry = id.clone();
    let retry_failed = move || {
        let Some((slots, guess)) = failed_batch.peek().clone() else {
            return;
        };
        // The closure's own copy — `Signal::set` routes through DerefMut, so
        // it needs a mutable binding here even though the outer `sync` is
        // only ever read.
        let mut sync = sync;
        sync.set(SyncState::Saving);
        let game_id = id_for_retry.clone();
        spawn_local(async move {
            commit_actions(&game_id, &slots, guess, actions, sync, failed_batch).await;
        });
    };

    // --- keyboard handling for the active clue (input auto-advance, etc.) -----
    let handle_key = move |index: usize, key: Key| match key {
        Key::Backspace => {
            let cur_empty = game_action_data
                .peek()
                .get(index)
                .map(|s| s.state.is_empty())
                .unwrap_or(true);
            if cur_empty && index > 0 {
                {
                    let mut g = game_action_data.write();
                    if let Some(s) = g.get_mut(index - 1) {
                        s.state = String::new();
                    }
                }
                focused_index.set(Some(index - 1));
            } else {
                let mut g = game_action_data.write();
                if let Some(s) = g.get_mut(index) {
                    s.state = String::new();
                }
            }
        }
        Key::ArrowLeft if index > 0 => focused_index.set(Some(index - 1)),
        Key::ArrowRight => {
            let len = game_action_data.peek().len();
            if index + 1 < len {
                focused_index.set(Some(index + 1));
            }
        }
        _ => {}
    };

    // A letter box taking focus hands the surface back to the editor, so the
    // grid's focus driver stops pulling focus out from under the caret.
    let editor_focus = move || {
        board_active.set(false);
    };

    // --- join / invite -------------------------------------------------------

    // Join the roster, refresh it, then announce ourselves with an (empty)
    // presence broadcast so the other players' strips light up immediately.
    let id_for_join = id.clone();
    let publish_for_join = publish_presence.clone();
    let join_game = move |_| {
        joining.set(true);
        join_error.set(String::new());
        let id = id_for_join.clone();
        let publish = publish_for_join.clone();
        spawn_local(async move {
            match net::mutation("activeGame.join", Some(json!({ "id": id }))).await {
                Ok(_) => {
                    if let Ok(data) = net::query("activeGame.get", Some(json!({ "id": id }))).await
                    {
                        members.set(parse_members(&data));
                    }
                    publish(None);
                }
                Err(e) => join_error.set(e),
            }
            joining.set(false);
        });
    };

    // Copy the invite URL via the JS clipboard API (no extra Rust deps).
    let id_for_invite = id.clone();
    let copy_invite = move |_| {
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default();
        let url = format!("{origin}/game/{id_for_invite}");
        let script = format!(
            "navigator.clipboard && navigator.clipboard.writeText({})",
            serde_json::to_string(&url).unwrap_or_default()
        );
        dioxus::document::eval(&script);
        invite_copied.set(true);
        spawn_local(async move {
            TimeoutFuture::new(2_000).await;
            invite_copied.set(false);
        });
    };

    // "Retry now", on the offline pill. Re-enters the retry ladder with the
    // attempt counter reset and no backoff wait (DEF-175 §2).
    let retry_now = {
        let conn = conn.clone();
        move |_: Event<MouseData>| conn.retry_now()
    };

    // ------------------------------------------------------------------------
    if *loading.read() {
        return rsx! { PlayStatusLoading {} };
    }
    if let Some(e) = load_error.read().clone() {
        return rsx! { PlayStatusError { message: e } };
    }

    let ws = use_workspace_local();
    crate::store::sync_panel_mode(ws.snapshot);

    // selected question, looked up fresh for the clue panel render
    let selected_q: Option<QuestionWithAnswerMap> = selected.read().and_then(|k| {
        answer_maps
            .read()
            .iter()
            .find(|m| qkey(&m.question) == k)
            .cloned()
    });

    let board_data = board.read().clone();
    let (size, grid) = (*board_data).clone();

    // Read with `read()`, not `peek()`: this is the render dependency that
    // makes the connection pill follow the socket.
    let conn_state = conn.signal();

    let body = move |kind: PanelId, _max: bool| -> Element {
        match kind {
            PanelId::Board => {
                let me = state.user();
                let my_id = me.as_ref().map(|u| u.id.clone());
                let mems = members.read().clone();
                let is_member = my_id
                    .as_deref()
                    .map(|id| mems.iter().any(|m| m.user_id == id))
                    .unwrap_or(false);
                let tick = *clock.read();
                // Live remote selections → colored focus borders on the board.
                // `stale_rings` dims them rather than dropping them while the
                // socket is only reconnecting.
                let stale_rings = retains_presence(&conn_state.read());
                let remote: Vec<RemoteSelection> = presence
                    .read()
                    .iter()
                    .filter(|(_, e)| tick.saturating_sub(e.tick) <= PRESENCE_TTL_SECS)
                    .filter_map(|(uid, e)| {
                        e.selection.map(|q| RemoteSelection {
                            key: q,
                            color: player_color(uid, my_id.as_deref()),
                            name: e.name.clone(),
                            stale: stale_rings,
                        })
                    })
                    .collect();
                let maps = answer_maps.read().clone();
                rsx! {
                    div { class: "cw-board-col",
                        {render_players_strip(
                            &mems,
                            &presence.read(),
                            my_id.as_deref(),
                            tick,
                            *invite_copied.read(),
                            copy_invite.clone(),
                            *conn_state.read(),
                            stale_rings,
                            retry_now.clone(),
                        )}
                        div { class: "cw-board-area",
                            {render_board(
                                &grid,
                                size,
                                &maps,
                                &selected_q,
                                &game_action_data.read(),
                                *focused_index.read(),
                                &remote,
                                select_coordinates.clone(),
                                roving_coord(),
                                handle_board_key.clone(),
                                on_cell_focus.clone(),
                                cell_nodes.clone(),
                            )}
                            if !is_member && !state.is_loading() {
                                {render_join_overlay(
                                    state.user().is_some(),
                                    *joining.read(),
                                    &join_error.read(),
                                    join_game.clone(),
                                )}
                            }
                        }
                    }
                }
            }
            PanelId::Clues => render_clues(
                &filtered,
                *selected.read(),
                *selected_direction.read(),
                &game_action_data.read(),
                *focused_index.read(),
                input_refs,
                select_question.clone(),
                toggle_dir.clone(),
                handle_letter_input,
                handle_key,
                editor_focus,
                unselect.clone(),
                submit_guess.clone(),
                sync.read().clone(),
                retry_failed.clone(),
            ),
        }
    };

    rsx! {
        style { {GAME_CSS} }
        {crate::workspace::render_workspace(&ws, body, &[])}
    }
}

/// Host-owned workspace with a stable storage key for this screen.
fn use_workspace_local() -> crate::workspace::PanelWorkspace<PanelId> {
    // "_v3": the Active Clue panel was merged into Clues. A persisted `_v2`
    // layout carries geometry for a panel that no longer exists — and now that
    // the `Clue` variant is gone its saved layout won't deserialize at all, so
    // the old key would silently fall back to defaults on every load anyway.
    crate::workspace::use_panel_workspace("crossword_game_play_v3", default_layout)
}

// ---------------------------------------------------------------------------
// rendering helpers
// ---------------------------------------------------------------------------

fn cell_number(maps: &[QuestionWithAnswerMap], cell: &Cell) -> Option<i32> {
    maps.iter()
        .find(|m| m.question.root_x == cell.cord_x && m.question.root_y == cell.cord_y)
        .map(|m| m.question.number)
}

/// First playable cell in row-major order. This is where the roving tabindex
/// rests before a clue has been picked, and where Tab/arrow seeding starts.
fn first_playable(grid: &[Vec<Cell>]) -> Option<(i32, i32)> {
    grid.iter()
        .flatten()
        .find(|c| !c.is_block())
        .map(|c| (c.cord_x, c.cord_y))
}

/// Step along `(dx, dy)` from `from` and return the first playable cell landed
/// on. Blocks are skipped rather than swallowed, so the cursor crosses a run of
/// them in one press instead of stopping on the first.
///
/// `None` means the ray left the grid without reaching a playable cell —
/// including the first step being off-grid, which is the edge case where the
/// cursor must not move at all.
fn step_to_playable(grid: &[Vec<Cell>], from: (i32, i32), dx: i32, dy: i32) -> Option<(i32, i32)> {
    let (mut x, mut y) = from;
    loop {
        let (nx, ny) = (x + dx, y + dy);
        // DEF-243 made a block square the `(-1, -1)` sentinel instead of a cell
        // at its own coordinate with an empty answer, so a block is no longer
        // findable by coordinate — the old `find(..cord_x == nx && cord_y == ny)?`
        // missed on the first block and one press stopped dead there, instead of
        // walking the run this function exists to skip.
        //
        // The grid is dense: `board_state_from_actions`, the only thing that
        // builds it, emits one entry per square. So index by position, and let
        // `is_block` — the sentinel — decide what to skip. Leaving the grid ends
        // the ray, which is the `None` this documents.
        let cell = match (usize::try_from(ny), usize::try_from(nx)) {
            (Ok(row), Ok(col)) => grid.get(row).and_then(|r| r.get(col))?,
            _ => return None,
        };
        if !cell.is_block() {
            return Some((nx, ny));
        }
        x = nx;
        y = ny;
    }
}

/// Accessible name for a letter cell, in the order a player reads the square:
/// clue number, then the letter (or "empty"), then its state — "14, R, correct"
/// / "7, empty".
///
/// Built from what is actually DRAWN (`display`), so a letter sitting in the
/// local draft but not yet committed is announced instead of reading as empty.
///
/// DEF-215: when a remote player's presence projects a ring onto this cell, the
/// label also carries who and whether that position is live or last-known. The
/// ring's `title` says the same thing, but `title` never wins over an explicit
/// `aria-label` — so without this, a screen-reader user on a stale ring hears
/// the letter and the correctness and nothing about the staleness.
fn cell_aria_label(
    num: Option<i32>,
    display: &str,
    action_type: Option<ActionType>,
    presence: Option<(&str, bool)>,
) -> String {
    let mut label = String::new();
    if let Some(n) = num {
        label.push_str(&format!("{n}, "));
    }
    if display.trim().is_empty() {
        label.push_str("empty");
    } else {
        label.push_str(display.trim());
        label.push_str(", ");
        label.push_str(match action_type {
            Some(ActionType::CorrectGuess) => "correct",
            Some(ActionType::IncorrectGuess) => "incorrect",
            // A typed-but-uncommitted letter and a saved placeholder are both
            // "in progress" as far as the board is concerned.
            _ => "in progress",
        });
    }
    // Appending presence must not drop the state suffix above (DEF-215 A14):
    // "is this wrong?" is still answerable after "whose cell is this?".
    if let Some((name, stale)) = presence {
        label.push_str(", ");
        label.push_str(name);
        if stale {
            label.push_str(" (last known — reconnecting)");
        }
    }
    label
}

/// The class list for one grid cell.
///
/// Focus, selection and the guess outcome are three *independent* facts about a
/// cell, so all of them are emitted. This used to be an
/// `if / else if / else` chain, which made the outcome class unreachable in
/// exactly the state where it matters most: an incorrect guess deliberately
/// KEEPS the word selected so the player can fix it in place, so every cell of
/// the just-guessed word took the `cw-selected` branch and the red never
/// rendered at all. The placeholders and the green had the same hole.
///
/// Precedence lives in CSS, in two independently ordered channels — the outcome
/// owns the wash/ink, selection owns the border, and `.cw-focused`'s cursor
/// geometry is unconditional. See the GAME_CSS block that consumes these.
fn cell_classes(focused: bool, selected: bool, action_type: Option<ActionType>) -> String {
    let mut classes = String::from("cw-cell cw-letter");
    if focused {
        classes.push_str(" cw-focused");
    }
    if selected {
        classes.push_str(" cw-selected");
    }
    match action_type {
        Some(ActionType::Placeholder) => classes.push_str(" cw-placeholder"),
        Some(ActionType::IncorrectGuess) => classes.push_str(" cw-incorrect"),
        Some(ActionType::CorrectGuess) => classes.push_str(" cw-correct"),
        None => {}
    }
    classes
}

/// Live letter state for a clue's bubble: prefer in-progress slots when selected.
fn bubble_state(
    key: QKey,
    cell: &Cell,
    selected: Option<QKey>,
    slots: &[ActionSlot],
) -> (String, &'static str) {
    if selected == Some(key) {
        if let Some(s) = slots
            .iter()
            .find(|s| s.cord_x == cell.cord_x && s.cord_y == cell.cord_y)
        {
            let at = cell
                .modifications
                .first()
                .map(|m| action_type_str(m.action_type))
                .unwrap_or("placeholder");
            return (s.state.clone(), if s.state.is_empty() { "" } else { at });
        }
    }
    match cell.modifications.first() {
        Some(m) => (m.state.clone(), action_type_str(m.action_type)),
        None => (String::new(), ""),
    }
}

/// One clue's letter bubbles, coloured by the guess outcome of each letter.
///
/// Rendered on EVERY row, including the one being edited. It used to be the
/// `else` of `if editing`, so the selected row swapped its bubbles for the
/// editor — and since the editor is exactly the state a wrong guess leaves the
/// player in, the clue list went silent at the same moment as the grid. The
/// list and the board must give the same answer to "what did I get wrong".
fn render_bubbles(
    m: &QuestionWithAnswerMap,
    key: QKey,
    selected: Option<QKey>,
    slots: &[ActionSlot],
) -> Element {
    rsx! {
        div {
            class: "cw-bubbles",
            for cell in m.answer_map.iter() {
                {
                    let (letter, at) = bubble_state(key, cell, selected, slots);
                    let mut bcls = String::from("cw-bubble");
                    if letter.is_empty() {
                        bcls.push_str(" cw-bubble-empty");
                    } else {
                        match at {
                            "incorrectGuess" => bcls.push_str(" cw-incorrect"),
                            "correctGuess" => bcls.push_str(" cw-correct"),
                            _ => bcls.push_str(" cw-placeholder"),
                        }
                    }
                    rsx! {
                        div { class: "{bcls}", "{letter}" }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_board(
    grid: &[Vec<Cell>],
    size: crossword_core::game::Coord,
    maps: &[QuestionWithAnswerMap],
    selected_q: &Option<QuestionWithAnswerMap>,
    slots: &[ActionSlot],
    focused_index: Option<usize>,
    remote: &[RemoteSelection],
    select_coordinates: impl FnMut(i32, i32) + Clone + 'static,
    roving_coord: Option<(i32, i32)>,
    handle_board_key: impl FnMut(KeyboardEvent) + Clone + 'static,
    on_cell_focus: impl FnMut() + Clone + 'static,
    cell_nodes: CellNodes,
) -> Element {
    let cols = size.x.max(1);
    let rows = size.y.max(1);
    // `.cw-board-area` is a `container-type: size` context, so cqw/cqh measure
    // the panel area itself (not the viewport). Width is the largest size that
    // fits BOTH axes — derived from the height cap via the grid ratio — and
    // aspect-ratio derives height from it. No cqh guessing, no JS zoom hacks.
    //
    // `--cw-cell` publishes the resulting exact cell edge so descendants can
    // size type as a fraction of the CELL rather than of the container. Sizing
    // off the container (the old `clamp(10px, 4.5cqw, 26px)`) was wrong twice
    // over: `cqw` ignores the column count, so the same 4.5cqw was ~68% of a
    // cell at 15 columns and ~22% at 5, and the clamp bounds were hard
    // discontinuities — at the floor the glyph stopped shrinking while the cell
    // kept shrinking, which is how letters spilled out of their cells on zoom.
    //
    // The gap is a fraction of the board rather than a fixed 3px (a hairline on
    // a big board, a quarter of a cell on a tiny one). Deriving the cell from
    // the gap — not the reverse — keeps the arithmetic exact:
    // cols*cell + (cols-1)*gap == the board edge, at every size.
    const GAP_FRAC: f64 = 0.004;
    let cell_frac = (1.0 - GAP_FRAC * (cols - 1) as f64) / cols as f64;
    let edge = format!("min(100cqw, 100cqh * {cols} / {rows})");
    let style = format!(
        "grid-template-columns: repeat({cols}, 1fr); \
         grid-template-rows: repeat({rows}, 1fr); \
         aspect-ratio: {cols} / {rows}; \
         width: {edge}; \
         --cw-gap: calc({edge} * {GAP_FRAC}); \
         --cw-cell: calc({edge} * {cell_frac:.6});",
    );

    // focused coord (the cell currently being typed in)
    let focused_coord: Option<(i32, i32)> = match (selected_q, focused_index) {
        (Some(m), Some(i)) => m.answer_map.get(i).map(|c| (c.cord_x, c.cord_y)),
        _ => None,
    };
    let is_in_selected = |x: i32, y: i32| -> bool {
        selected_q
            .as_ref()
            .map(|m| m.answer_map.iter().any(|c| c.cord_x == x && c.cord_y == y))
            .unwrap_or(false)
    };
    let typed_at = |x: i32, y: i32| -> String {
        slots
            .iter()
            .find(|s| s.cord_x == x && s.cord_y == y)
            .map(|s| s.state.clone())
            .unwrap_or_default()
    };

    rsx! {
        div { class: "cw-board-wrap",
            div { class: "cw-board", style: "{style}",
                role: "grid",
                "aria-label": "Crossword grid",
                "aria-rowcount": "{rows}",
                "aria-colcount": "{cols}",
                onkeydown: handle_board_key,
                // The row wrappers below are `display: contents`, so they exist
                // only to give the grid a `row` structure for assistive tech —
                // they add no box and leave the cell placement untouched.
                for row in grid.iter() {
                    div { class: "cw-row", role: "row",
                        for cell in row.iter() {
                            {
                                let is_letter = !cell.is_block();
                                let x = cell.cord_x;
                                let y = cell.cord_y;
                                if !is_letter {
                                    // A block is not a cell: no role, no
                                    // tabindex, so it is neither announced nor
                                    // reachable, and it can never swallow the
                                    // cursor.
                                    rsx! { div { class: "cw-cell cw-block" } }
                                } else {
                                    let selected = is_in_selected(x, y);
                                    let focused = focused_coord == Some((x, y));
                                    let num = cell_number(maps, cell);
                                    let action_type = cell.modifications.first().map(|m| m.action_type);
                                    let mut classes = cell_classes(focused, selected, action_type);
                                    // A remote player's focused word gets a colored ring
                                    // (inset shadow — no layout shift) + a hover tooltip.
                                    let remote_hit = remote.iter().find(|r| {
                                        maps.iter().any(|m| {
                                            qkey(&m.question) == r.key
                                                && m.answer_map
                                                    .iter()
                                                    .any(|c| c.cord_x == x && c.cord_y == y)
                                        })
                                    });
                                    let (ring, ring_title, ring_stale, ring_presence) = match remote_hit {
                                        Some(r) => {
                                            // Last-known, not live: the socket is
                                            // reconnecting, so dim the ring rather
                                            // than let the TTL prune it and read as
                                            // "they left". DEF-175 §4.
                                            //
                                            // DEF-188 D1b: the dimming lives in the
                                            // RING, never on the cell. `opacity`
                                            // composites the whole element, so the
                                            // old `.cw-ring-stale { opacity: .55 }`
                                            // took the confirmed letter down with
                                            // it — 3.94:1 on --bg-cell-letter in
                                            // light mode, under AA. The position
                                            // is last-known; the letter is not.
                                            let ring = if r.stale {
                                                format!(
                                                    "box-shadow: inset 0 0 0 2px {c}, inset 0 0 0 4px color-mix(in srgb, {c} 55%, transparent);",
                                                    c = &r.color
                                                )
                                            } else {
                                                format!("box-shadow: inset 0 0 0 2px {};", r.color)
                                            };
                                            (
                                                ring,
                                                format!(
                                                    "{} is working here{}",
                                                    r.name,
                                                    if r.stale { " (last known — reconnecting)" } else { "" }
                                                ),
                                                r.stale,
                                                Some((r.name.as_str(), r.stale)),
                                            )
                                        }
                                        None => (String::new(), String::new(), false, None),
                                    };
                                    // A state hook for the e2e suite, not a style: the
                                    // visual treatment is the ring above.
                                    if ring_stale {
                                        classes.push_str(" cw-ring-stale");
                                    }
                                    let display = if selected {
                                        typed_at(x, y)
                                    } else {
                                        cell.modifications.first().map(|m| m.state.clone()).unwrap_or_default()
                                    };
                                    let aria_label =
                                        cell_aria_label(num, &display, action_type, ring_presence);
                                    // Roving tabindex: exactly one cell in the
                                    // grid is a Tab stop, so Tab enters the
                                    // board once instead of 225 times.
                                    let tabindex = if roving_coord == Some((x, y)) { "0" } else { "-1" };
                                    let mut sc = select_coordinates.clone();
                                    let mut of = on_cell_focus.clone();
                                    let nodes = cell_nodes.clone();
                                    rsx! {
                                        div {
                                            key: "{x}-{y}",
                                            class: "{classes}",
                                            "data-x": "{x}",
                                            "data-y": "{y}",
                                            style: "{ring}",
                                            title: "{ring_title}",
                                            role: "gridcell",
                                            tabindex: "{tabindex}",
                                            "aria-label": "{aria_label}",
                                            onclick: move |_| sc(x, y),
                                            onfocus: move |_| of(),
                                            onmounted: move |e: Event<MountedData>| {
                                                nodes.borrow_mut().insert((x, y), e.data());
                                            },
                                            if let Some(n) = num {
                                                span { class: "cw-num", "{n}" }
                                            }
                                            span { class: "cw-char", "{display}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The clue list, and — on whichever row is selected — the editor for it.
///
/// The selected row swaps its read-only bubbles for the real letter inputs, so
/// the clue you are answering and the list you picked it from are one surface.
/// Everything from `focused_index` onward exists only to serve that inline
/// editor; unselected rows never touch it.
#[allow(clippy::too_many_arguments)]
fn render_clues(
    filtered: &[QuestionWithAnswerMap],
    selected: Option<QKey>,
    selected_direction: Option<Direction>,
    slots: &[ActionSlot],
    focused_index: Option<usize>,
    mut input_refs: Signal<Vec<Option<Rc<MountedData>>>>,
    select_question: impl FnMut(QKey) + Clone + 'static,
    mut toggle_dir: impl FnMut(Direction) + Clone + 'static,
    handle_letter_input: impl FnMut(usize, String) + Clone + 'static,
    handle_key: impl FnMut(usize, Key) + Clone + 'static,
    editor_focus: impl FnMut() + Clone + 'static,
    unselect: impl FnMut(Event<MouseData>) + Clone + 'static,
    submit_guess: impl FnMut() + Clone + 'static,
    sync: SyncState,
    on_retry: impl FnMut() + Clone + 'static,
) -> Element {
    let mut toggle_a = toggle_dir.clone();
    let mut retry = on_retry.clone();
    let across_active = selected_direction == Some(Direction::Across);
    let down_active = selected_direction == Some(Direction::Down);
    // The saved line. Keyed off the write state, so what a screen reader
    // hears and what the panel shows cannot disagree. A failed write keeps
    // the retry affordance beside it; everything else is text only.
    let (sync_text, sync_cls): (&str, &str) = match &sync {
        SyncState::Saving => ("Saving…", "cw-sync cw-sync-saving"),
        SyncState::Saved(None) => ("Saved", "cw-sync cw-sync-saved"),
        SyncState::Saved(Some((right, total))) => {
            if right == total {
                ("Word solved", "cw-sync cw-sync-saved")
            } else {
                ("Not yet", "cw-sync cw-sync-saved")
            }
        }
        SyncState::Failed => ("Not saved", "cw-sync cw-sync-failed"),
        SyncState::Idle => ("", "cw-sync"),
    };
    let count_line: Option<String> = match &sync {
        SyncState::Saved(Some((right, total))) if right != total => {
            Some(format!("{right} of {total} letters correct"))
        }
        _ => None,
    };
    let sync_line = match &count_line {
        Some(line) => format!("{sync_text} — {line}"),
        None => sync_text.to_string(),
    };

    rsx! {
        div { class: "cw-clues",
            div { class: "cw-clues-head",
                h2 { "Clues" }
                div { class: "cw-head-right",
                    span { class: "{sync_cls}", role: "status", "{sync_line}" }
                    if matches!(sync, SyncState::Failed) {
                        button {
                            class: "cw-sync-retry",
                            onclick: move |_| retry(),
                            "Retry"
                        }
                    }
                        div { class: "cw-tabs",
                            button {
                                class: if across_active { "cw-tab cw-tab-active-across" } else { "cw-tab" },
                                onclick: move |_| toggle_a(Direction::Across),
                                "Across"
                            }
                            button {
                                class: if down_active { "cw-tab cw-tab-active-down" } else { "cw-tab" },
                                onclick: move |_| toggle_dir(Direction::Down),
                                "Down"
                            }
                        }
                    }
            }
            div { class: "cw-clue-list",
                for m in filtered.iter().cloned() {
                    {
                        let key = qkey(&m.question);
                        let is_sel = selected == Some(key);
                        // `slots` is the live in-progress word and only ever belongs to
                        // the selected clue. Empty means the selection has not been
                        // wired up yet, so render the editor's absence — the bubbles
                        // above it render either way, which is what keeps the clue
                        // list in agreement with the board after a wrong guess.
                        let editing = is_sel && !slots.is_empty();
                        let mut sq = select_question.clone();
                        let row_cls = if is_sel { "cw-clue-row cw-clue-row-sel" } else { "cw-clue-row" };
                        rsx! {
                            div {
                                class: "{row_cls}",
                                onclick: move |_| sq(key),
                                div { class: "cw-clue-badge", "{m.question.number}" }
                                div { class: "cw-clue-body",
                                    div { class: "cw-clue-row-text", "{m.question.question_text}" }
                                    { render_bubbles(&m, key, selected, slots) }
                                    if editing {
                                        {
                                            let unselect_a = unselect.clone();
                                            let unselect_b = unselect.clone();
                                            let mut submit = submit_guess.clone();
                                            rsx! {
                                                // Clicks inside the editor must not reach the row's
                                                // own onclick — Cancel would otherwise re-select the
                                                // clue it just cleared, and every click on an input
                                                // would re-run select_question and reset focus.
                                                div {
                                                    class: "cw-clue-editor",
                                                    onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                                    // Direction badge + length. This editor replaced the
                                                    // standalone Active Clue panel, which is the only place
                                                    // the direction was ever shown: the clue list groups by
                                                    // the Across/Down tab, but a board cell click can select
                                                    // the OTHER direction, and without this the player cannot
                                                    // tell which word they are typing.
                                                    div { class: "cw-clue-meta",
                                                        span { class: "cw-dir-badge cw-dir-{dir_str(m.question.direction).to_lowercase()}",
                                                            "{dir_str(m.question.direction)}"
                                                        }
                                                        span { class: "muted",
                                                            "CLUE {m.question.number} · {m.answer_map.len()} LETTERS"
                                                        }
                                                    }
                                                    div { class: "cw-letters",
                                                        for (index , slot) in slots.iter().cloned().enumerate() {
                                                            {
                                                                let focused = focused_index == Some(index);
                                                                let mut cls = String::from("cw-letter-input");
                                                                if focused {
                                                                    cls.push_str(" cw-input-focused");
                                                                }
                                                                let mut hi = handle_letter_input.clone();
                                                                let mut hk = handle_key.clone();
                                                                let mut ef = editor_focus.clone();
                                                                rsx! {
                                                                    input {
                                                                        key: "{slot.cord_x}-{slot.cord_y}",
                                                                        class: "{cls}",
                                                                        r#type: "text",
                                                                        // No maxlength="1": a full box makes the browser
                                                                        // swallow the keystroke entirely — no oninput, no
                                                                        // auto-advance, and a prefilled (resumed) word
                                                                        // becomes impossible to edit. The handler keeps
                                                                        // the last char typed, so length stays enforced.
                                                                        autocomplete: "off",
                                                                        spellcheck: "false",
                                                                        value: "{slot.state}",
                                                                        onmounted: move |e: Event<MountedData>| {
                                                                            let mut refs = input_refs.write();
                                                                            if index < refs.len() {
                                                                                refs[index] = Some(e.data());
                                                                            }
                                                                        },
                                                                        oninput: move |e| hi(index, e.value()),
                                                                        onkeydown: move |e| hk(index, e.key()),
                                                                        onfocus: move |_| ef(),
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div { class: "cw-clue-actions",
                                                        button {
                                                            class: "cw-link-btn",
                                                            onclick: unselect_a,
                                                            "ESC to clear"
                                                        }
                                                        button {
                                                            class: "cw-btn-cancel",
                                                            onclick: unselect_b,
                                                            "Cancel"
                                                        }
                                                        button {
                                                            class: "cw-btn-guess",
                                                            onclick: move |_| submit(),
                                                            "Guess"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Progressive disclosure, native <details>: keyboard-operable and
            // in the accessibility tree with no JS focus management. The
            // audit's P1 — the play surface explained neither its controls
            // nor its state colors, so a first-time solver had to infer both
            // at the highest-stakes task.
            details { class: "cw-help",
                summary { "How to play" }
                div { class: "cw-help-body",
                    p { "Arrow keys move the cursor, typing advances to the next cell, Enter saves and checks the word, Escape clears it. The Across and Down tabs filter the list." }
                    div { class: "cw-help-legend",
                        span { class: "cw-legend-item cw-legend-editing", "Word you're editing" }
                        span { class: "cw-legend-item cw-legend-wrong", "Wrong guess" }
                        span { class: "cw-legend-item cw-legend-right", "Correct" }
                        span { class: "cw-legend-item cw-legend-live", "Teammate typing here" }
                        span { class: "cw-legend-item cw-legend-stale", "Teammate's last spot" }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// The co-op roster bar: one chip per player (color dot, name, host/you tags,
/// the clue they're on) + the invite-link button. Players we only know about
/// via presence (joined after we loaded) get chips too.
#[allow(clippy::too_many_arguments)]
fn render_players_strip(
    mems: &[MemberInfo],
    presence: &HashMap<String, PresenceEntry>,
    my_id: Option<&str>,
    tick: u64,
    invite_copied: bool,
    mut copy_invite: impl FnMut(Event<MouseData>) + Clone + 'static,
    conn: net::ConnectionState,
    stale: bool,
    retry_now: impl FnMut(Event<MouseData>) + Clone + 'static,
) -> Element {
    // (uid, name, is_owner, live selection)
    let mut chips: Vec<(String, String, bool, Option<QKey>)> = Vec::new();
    for m in mems {
        let sel = presence
            .get(&m.user_id)
            .filter(|e| tick.saturating_sub(e.tick) <= PRESENCE_TTL_SECS)
            .and_then(|e| e.selection);
        chips.push((m.user_id.clone(), m.user_name.clone(), m.is_owner, sel));
    }
    for (uid, e) in presence {
        if mems.iter().any(|m| &m.user_id == uid) || tick.saturating_sub(e.tick) > PRESENCE_TTL_SECS
        {
            continue;
        }
        chips.push((uid.clone(), e.name.clone(), false, e.selection));
    }

    // Snapshot the state before the closure below borrows other things, so the
    // pill reads one consistent value.
    let pill = render_conn_pill(conn, retry_now);

    // Roster status in words, before anything goes wrong. Presence was
    // invisible while healthy and appeared only when it broke, so a partner
    // who is paused looked identical to one who is typing. Live count
    // excludes the local player — they are trivially live.
    let live_now = chips
        .iter()
        .filter(|(uid, ..)| Some(uid.as_str()) != my_id)
        .filter(|(uid, ..)| {
            presence
                .get(uid)
                .is_some_and(|e| tick.saturating_sub(e.tick) <= PRESENCE_TTL_SECS)
        })
        .count();
    let others = chips.len().saturating_sub(1);
    let roster_line = if others == 0 {
        "Only you here — copy the invite link to play together".to_string()
    } else if live_now == 0 {
        "Waiting for teammates".to_string()
    } else if stale {
        format!("{live_now} of {others} live — positions last known")
    } else {
        format!("{live_now} of {others} live now")
    };

    rsx! {
        div { class: "cw-players",
            span { class: "cw-roster-line", "{roster_line}" }
            for (uid, name, is_owner, sel) in chips {
                {
                    let color = player_color(&uid, my_id);
                    let is_you = Some(uid.as_str()) == my_id;
                    // DEF-188 D1a: no `opacity` on the chip. The name is the one
                    // thing a player needs during an outage, and at .55 it
                    // composited to 3.89:1 on --bg-card in light mode — under AA,
                    // on the surface that tells you who is in the game with you.
                    // The underline (the 2px border-bottom that already correlates
                    // the chip with that player's ring) carries the staleness
                    // instead, and the title says it in words.
                    let underline = if stale {
                        format!("border-bottom: 2px dashed color-mix(in srgb, {color} 55%, transparent);")
                    } else {
                        format!("border-bottom: 2px solid {color};")
                    };
                    let chip_title = if stale {
                        "Last known position — reconnecting"
                    } else {
                        ""
                    };
                    rsx! {
                        span {
                            class: "cw-chip",
                            key: "{uid}",
                            // The underline correlates the chip with that
                            // player's focus ring on the board.
                            style: "{underline}",
                            title: "{chip_title}",
                            Identicon { seed: uid.clone(), size: 16 }
                            span { "{name}" }
                            if is_you {
                                span { class: "cw-chip-tag", "you" }
                            }
                            if is_owner {
                                span { class: "cw-chip-tag", "host" }
                            }
                            if let Some((n, d)) = sel {
                                span { class: "cw-chip-clue", style: "color: {color};",
                                    "#{n} {dir_str(d).to_lowercase()}"
                                }
                            }
                            // DEF-215: `title` is in the AX tree but lands as a
                            // `description` on a nameless `generic`, which no
                            // screen reader announces. A real StaticText in the
                            // content flow is announced in browse mode instead.
                            // Visible now, not screen-reader-only: a dashed
                            // underline is the one cue a sighted player had, and
                            // it does not survive a glance or a screenshot.
                            if stale {
                                span { class: "cw-chip-tag cw-chip-stale-tag", "last seen" }
                            }
                        }
                    }
                }
            }
            {pill}
            button {
                class: "cw-invite-btn",
                onclick: move |e| copy_invite(e),
                if invite_copied { "Link copied ✓" } else { "Copy invite link" }
            }
        }
    }
}

/// The co-op socket's status pill, rendered in the roster bar (DEF-175 §5).
///
/// While the socket is live — and while it is still making its very first
/// attempt — this renders NOTHING, and that is the assertion worth keeping: a
/// permanent "connected" badge is chrome that is right 100% of the time, which
/// is what makes it read as noise, and a warning-coloured badge that appears on
/// every cold load is the same noise in a louder key (DEF-188 D4). The e2e spec
/// asserts the absence from first paint rather than taking it on trust.
///
/// The copy is honest about the *direction* of the failure. During an outage the
/// player's own letters are not reaching the other players, and that is the
/// fact that changes what they do next — a vague "Connection lost" hides it.
fn render_conn_pill(
    state: net::ConnectionState,
    mut retry_now: impl FnMut(Event<MouseData>) + Clone + 'static,
) -> Element {
    let (variant, label, attempt) = match state {
        // No badge while healthy, and none for the very first open either
        // (DEF-188 D4). `Connecting` is only ever a subscription's first-ever
        // open — net.rs sets `reported` on the first iteration and never resets
        // it — so mapping it to `warn` put an amber badge on the roster bar of
        // every single game load, and, because the pill is a live region, an
        // announced "Connecting…" every single time. The play screen's own
        // first-paint state already covers that window, and a genuine failure
        // still reports `Reconnecting { attempt: 1 }` immediately.
        net::ConnectionState::Live | net::ConnectionState::Connecting => return rsx! {},
        net::ConnectionState::Reconnecting { attempt } => {
            let label = "Reconnecting…".to_string();
            if attempt > 1 {
                ("warn", label, Some(attempt))
            } else {
                ("warn", label, None)
            }
        }
        net::ConnectionState::Offline => (
            "error",
            "Connection lost — your letters aren't being shared".to_string(),
            None,
        ),
    };
    let cls = format!("cw-conn-pill cw-conn-{variant}");
    rsx! {
        span {
            // `data-conn-state` is the e2e hook: a test needs to tell
            // "reconnecting" from "offline" without matching on copy.
            class: "{cls}",
            // CONSTANT, and load-bearing (DEF-188 D3). A key that churns with
            // the attempt count (`reconnecting-2` → `reconnecting-3` → …) makes
            // dioxus-core's keyed diff drop the old node and mount a fresh one,
            // and re-creating an already-populated `role="status"` region is
            // the case screen readers are least likely to announce — the
            // counter ticks and the terminal jump to offline were both being
            // lost. The pill sits in a fixed slot in the roster bar, so it is
            // never reordered and the key buys nothing.
            key: "cw-conn-pill",
            role: "status",
            "aria-live": "polite",
            "data-conn-state": "{variant}",
            "{label}"
            if let Some(n) = attempt {
                span { class: "cw-conn-attempt", "({n}/{net::MAX_RETRY_ATTEMPTS})" }
            }
            if variant == "error" {
                button {
                    class: "cw-conn-retry",
                    onclick: move |e| retry_now(e),
                    "Retry now"
                }
            }
        }
    }
}

/// The join prompt covering the board for non-members (the game itself is
/// watchable either way — `activeGame.get` is public by design).
fn render_join_overlay(
    signed_in: bool,
    joining: bool,
    join_error: &str,
    mut join_game: impl FnMut(Event<MouseData>) + Clone + 'static,
) -> Element {
    rsx! {
        div { class: "cw-join-overlay",
            div { class: "cw-join-card",
                h3 { "Co-op game in progress" }
                if signed_in {
                    p { class: "muted",
                        "You're watching live. Join to start filling the grid with everyone else."
                    }
                    button {
                        class: "cw-btn-guess",
                        disabled: joining,
                        onclick: move |e| join_game(e),
                        if joining { "Joining…" } else { "Join game" }
                    }
                    if !join_error.is_empty() {
                        p { class: "error", "{join_error}" }
                    }
                } else {
                    p { class: "muted", "You're watching live. Sign in to join the grid." }
                    Link { to: Route::Login {}, class: "app-btn app-btn-active", "Sign in" }
                }
            }
        }
    }
}

/// Timestamp for an optimistic local action. `sort_modifications` orders
/// newest-first by lexicographic `submitted_at`, so this must be a real UTC
/// ISO instant like the server's. A fabricated "maximal" timestamp outranks
/// every real action forever: a remote player's correct guess could never
/// displace a stale local placeholder until reload (the whole board appeared
/// frozen to the other player).
fn js_now_iso() -> String {
    js_sys::Date::new_0().to_iso_string().into()
}

// ---------------------------------------------------------------------------

const GAME_CSS: &str = r#"
.cw-board-wrap { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; padding: 0; box-sizing: border-box; }
.cw-board-col { display: flex; flex-direction: column; height: 100%; width: 100%; }
.cw-board-area { position: relative; flex: 1; min-height: 0; overflow: hidden; display: flex; align-items: center; justify-content: center; padding: 4px; box-sizing: border-box; container-type: size; }
.cw-players { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; padding: 8px 10px; border-bottom: 1px solid var(--border-app); }
/* The roster's status in words — takes its own row (flex-basis 100%) above the
   chips so "who is actually here" is answered before anything goes wrong. */
.cw-roster-line { flex-basis: 100%; font-family: var(--font-sans); font-size: var(--fs-2xs);
  font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); }
/* Stale presence as a tag, not only a dashed underline. */
.cw-chip-stale-tag { border-style: dashed; }
.cw-chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 1px solid var(--border-app); border-bottom-width: 2px; font-size: var(--fs-xs); font-family: var(--font-sans); color: var(--text-primary); background: var(--bg-card); }
.cw-chip-tag { font-size: var(--fs-2xs); font-family: var(--font-sans); text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); border: 1px solid var(--border-app); padding: 0 4px; }
.cw-chip-clue { font-size: var(--fs-2xs); font-weight: 700; text-transform: uppercase; letter-spacing: .05em; }
.cw-invite-btn { margin-left: auto; padding: 4px 12px; position: relative; font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: .05em; border: 1px solid var(--border-app); background: transparent; color: var(--text-secondary); cursor: pointer; white-space: nowrap; }
.cw-invite-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
/* The co-op socket's status pill (DEF-175 §5). Rendered in the roster bar, which
   is the co-op surface: always visible, already carrying per-player live state,
   and where a player looks to understand who is doing what.

   The fill is --color-warning / --color-error, i.e. --pastel-yellow /
   --pastel-red, which light mode darkens — so the INK is --contrast-ink and
   nothing else. It is dark in both themes on the dark pastels and white on the
   light-mode pastels, so it clears 4.5:1 against either fill in either theme
   (measured: 14.9:1 and 8.0:1 dark, 6.7:1 and 6.6:1 light; DEF-188 D5). The
   earlier version of this comment named the theme-stable --fill-*/--fill-ink
   pair the selection fills use, and someone "correcting" the code to match it
   would have shipped --fill-ink, which is 2.65:1 in light mode. */
.cw-conn-pill { display: inline-flex; align-items: center; gap: 6px; margin-left: 6px; padding: 3px 10px; font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 600; line-height: 1.4; border: 1px solid; animation: cw-conn-breathe 2.4s ease-in-out infinite; }
@keyframes cw-conn-breathe { 0%, 100% { opacity: 1; } 50% { opacity: .72; } }
.cw-conn-warn { background: var(--color-warning); color: var(--contrast-ink); border-color: var(--pastel-yellow); }
.cw-conn-error { background: var(--color-error); color: var(--contrast-ink); border-color: var(--pastel-red); }
.cw-conn-attempt { font-variant-numeric: tabular-nums; opacity: .85; }
.cw-conn-retry { margin-left: 2px; padding: 1px 8px; font: inherit; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; background: transparent; color: inherit; border: 1px solid currentColor; cursor: pointer; }
.cw-conn-retry:hover { background: color-mix(in srgb, var(--contrast-ink) 14%, transparent); }
/* A last-known position, not a live one: the socket is reconnecting. Dimmed
   rather than dropped, so a network blip does not read as "everyone left".
   DEF-188 D1: the dimming is a `color-mix` on the underline (chip) and on the
   inset ring (cell), both emitted inline. It used to be `opacity: .55` on the
   element, which composites the WHOLE element — the player's name on the chip
   fell to 3.89:1 on --bg-card and the confirmed letter inside a stale ring fell
   to 3.94:1 on --bg-cell-letter, both under AA in light mode. Only the
   non-text decoration is dimmed now; the classes stay as state hooks for e2e. */
.cw-join-overlay { position: absolute; inset: 0; z-index: 5; display: flex; align-items: center; justify-content: center; background: var(--scrim); backdrop-filter: blur(2px); }
.cw-join-card { display: flex; flex-direction: column; gap: 12px; max-width: 22rem; padding: 24px 28px; text-align: center; background: var(--bg-card); border: 1px solid var(--border-app); }
.cw-join-card h3 { margin: 0; font-size: 15px; color: var(--text-primary); }
.cw-join-card p { margin: 0; font-size: 12px; }
.cw-join-card .error { font-size: var(--fs-2xs); font-family: var(--mono); }
.cw-board { display: grid; gap: var(--cw-gap, 3px); max-width: 100%; max-height: 100%; min-width: 0; min-height: 0; }
/* min-width/min-height:0 is load-bearing: grid items default to `auto`, whose
   automatic minimum size floors each 1fr track at the cell's content size. The
   tracks then blow past the board's own width and `.cw-board-area`'s
   `overflow:hidden` clips the last columns/rows off. */
/* Type is a fraction of --cw-cell (the exact cell edge, published by
   render_board) — never of the container, and never clamped. An uppercase
   glyph's cap height is ~0.7em, so 0.58 leaves ~20% of the cell as breathing
   room at EVERY size, and there is no bound for the glyph to outgrow when the
   cell shrinks. `line-height: 1` is the other half of the fix: the font's
   default leading made the line box taller than the cell independently of
   font-size, which is what pushed glyphs past the cell edge. */
/* Row wrappers exist only so the grid exposes a `row` structure to assistive
   tech. `display: contents` means the wrapper generates no box at all, so the
   cells are still placed by `.cw-board`'s own grid and the 2-D layout is
   unchanged. A visible row box here would re-introduce the exact reflow this
   container-query sizing exists to prevent. */
.cw-row { display: contents; }
.cw-cell { position: relative; aspect-ratio: 1 / 1; border-radius: 0; display: flex; align-items: center; justify-content: center; font-weight: 700; text-transform: uppercase; user-select: none; font-size: calc(var(--cw-cell) * 0.58); line-height: 1; min-width: 0; min-height: 0; }
.cw-block { background: var(--bg-cell-empty); border: 1px solid color-mix(in srgb, var(--border-app) 25%, transparent); opacity: 0.4; }
/* Keyboard focus ring, two-tone on a ::after (DEF-188 D2).

   `.cw-focused` is the GAME cursor (a --fill-yellow fill), which is not a focus
   indicator — a player who arrow-keys across a solved word sees the fill move
   without any indication of where the caret is. The ring is needed, and on
   first Tab into the board the roving cell is a bare `.cw-letter`, so this is
   the common case, not an edge case.

   One colour cannot clear 3:1 against BOTH cell fills: --fill-ink is 14.8:1 on
   the yellow cursor but 1.10:1 on a plain letter cell, and --text-primary is
   the mirror image. So the ring is two tones, outer light and inner dark:

     outer  --text-primary   14.8:1 dark / 17.7:1 light on a plain .cw-letter
     inner  --fill-ink       14.8:1 dark / 13.7:1 light on the --fill-yellow cursor

   Whichever fill the cell has, one of the two bands has 3:1 against it — which
   is the WCAG 1.4.11 requirement for a focus indicator.

   WHY ::after, NOT the cell's own box-shadow: the remote presence ring is an
   inline `box-shadow: inset 0 0 0 2px …` on this same element, and an inline
   style overwrites any stylesheet `box-shadow`, so a focus band placed there
   would either be clobbered by the ring or clobber it. A pseudo-element is its
   own box: the two cannot collide, the remote ring keeps the outer 0–2px band
   to itself, and neither can change layout. `.cw-cell` is already
   `position: relative`, which is what the ::after hangs off.

   `outline: none` on the cell is safe in forced-colours mode: the palette
   cannot repaint a box-shadow, but it does repaint the ::after's OWN outline,
   so the indicator survives where the old inset ring would have vanished. The
   z-index lifts the ring above `.cw-focused`'s `scale(1.05)` neighbour, so
   arrow-keying onto a cell next to the cursor does not clip the ring. */
.cw-cell:focus-visible { outline: none; z-index: 3; }
.cw-cell:focus-visible::after {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  box-shadow:
    inset 0 0 0 2px var(--text-primary),
    inset 0 0 0 4px var(--fill-ink);
  outline: 2px solid var(--text-primary);
  outline-offset: 2px;
}
/* ---- Cell state compositing -----------------------------------------------
   Focus, selection and the guess outcome are three INDEPENDENT facts about a
   cell, so they compose instead of excluding each other. They are published
   as custom properties and painted once by `.cw-letter` below.

   Two channels, ordered independently, because the two questions they answer
   have different winners:

     * wash + ink — "what happened to this word". The outcome wins, because
       "this word is wrong" is the one thing the player must not miss, even
       while the word stays selected so it can be fixed in place, or while the
       cursor sits on it. Falls back to the cursor, then the selection, then
       the default.
     * border — "which word am I editing". Selection wins, because the yellow
       outline is the activity signal and the outcome already reads from the
       wash. A wrong-but-selected cell therefore shows a red wash inside a
       yellow outline and reads as both at once.

   Geometry is deliberately NOT a channel: `.cw-focused`'s scale/z-index is the
   cursor and is unconditional, so arrow-keying across a wrong word still shows
   where the caret is. That is the #117 contract — focus and state must both
   stay legible — and it is why the red is allowed to win the wash. */
.cw-letter {
  background: var(--cw-wash, var(--bg-cell-letter));
  color: var(--cw-ink, var(--text-primary));
  border: 1px solid var(--cw-bd, var(--border-app));
  cursor: pointer;
  transition: all .12s ease;
}
/* `:where()` zeroes the specificity so this stays a (0,1,0) rule and the
   channel order below decides the border on hover. Without it the hover rule
   outranks every state class and a hovered selected cell would lose its yellow
   outline to a plain hover border. */
.cw-letter:where(:hover) { --cw-bd: var(--border-hover); }
/* Wash + ink channel, lowest precedence first. */
.cw-selected { --cw-wash: color-mix(in srgb, var(--pastel-yellow) 18%, transparent); --cw-ink: var(--text-primary); }
/* The cursor. --fill-yellow/--fill-ink rather than --pastel-yellow/
   --contrast-ink: this cell is the brightest thing in the grid and that IS the
   signal, but light mode darkens the pastels and flips the ink white, which
   would make the one cell the player is looking at a black hole among white
   ones. The fill tokens stay pale with dark ink in both themes (styles.rs). */
.cw-focused { --cw-wash: var(--fill-yellow); --cw-ink: var(--fill-ink); transform: scale(1.05); z-index: 2; }
/* Ink is --text-primary on BOTH outcome washes. The old per-outcome inks put
   the same hue's dark form on top of its own 15% wash: red-on-red measured
   4.45:1 in light mode (AA wants 4.5) and green-on-green sat at 4.93 — the
   outcome was carried by the letters instead of the wash, exactly what the
   two-channel split below exists to prevent. The wash and the border still
   read red/green; the letters just stop competing with them. Same rule the
   clue bubbles and .cw-selected already follow. */
.cw-incorrect { --cw-wash: color-mix(in srgb, var(--pastel-red) 15%, transparent); --cw-ink: var(--text-primary); }
.cw-correct { --cw-wash: color-mix(in srgb, var(--pastel-green) 15%, transparent); --cw-ink: var(--text-primary); }
/* Border channel, lowest precedence first. The outcome classes set their own
   border colour so a wrong cell that is NOT selected still reads as wrong from
   the outline alone; selection then overrides it, which is what makes a
   selected wrong cell read as both. */
.cw-placeholder { --cw-bd: var(--pastel-yellow); border-width: 2px; }
.cw-incorrect { --cw-bd: var(--pastel-red); }
.cw-correct { --cw-bd: var(--pastel-green); }
.cw-selected { --cw-bd: var(--pastel-yellow); }
.cw-focused { --cw-bd: var(--pastel-yellow); }
/* Proportional inset too: a fixed 2px/3px offset shoved the number off a small
   cell while the number itself was clamped large. */
.cw-num { position: absolute; top: calc(var(--cw-cell) * 0.06); left: calc(var(--cw-cell) * 0.09); font-size: calc(var(--cw-cell) * 0.26); line-height: 1; font-weight: 700; pointer-events: none; }
.cw-incorrect .cw-num,
.cw-correct .cw-num { color: var(--text-primary); }
.cw-char { pointer-events: none; line-height: 1; }

.cw-link-btn { margin-left: auto; background: none; border: none; color: var(--text-secondary); font-size: var(--fs-2xs); cursor: pointer; position: relative; }
.cw-link-btn:hover { color: var(--text-primary); }
/* The inline editor, inside the selected clue row. `.cw-clue-actions` used to
   sit at `margin-top: auto` in a full-height panel; in a content-sized row that
   auto margin collapses, so spacing is explicit here. */
/* Editor meta row: direction badge + "CLUE n · m LETTERS". Direction is
   load-bearing now that the editor lives inside the list — the Across/Down tab
   shows the list's filter, not the selected word's direction, and a board cell
   click can select the other one. */
.cw-clue-meta { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.cw-dir-badge { font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 600; letter-spacing: 0.1em; padding: 2px 6px; border-radius: 0; border: 1px solid; }
.cw-dir-across { background: color-mix(in srgb, var(--pastel-yellow) 10%, transparent); color: var(--pastel-yellow); border-color: color-mix(in srgb, var(--pastel-yellow) 20%, transparent); }
.cw-dir-down { background: color-mix(in srgb, var(--pastel-green) 10%, transparent); color: var(--pastel-green); border-color: color-mix(in srgb, var(--pastel-green) 20%, transparent); }
.cw-clue-editor { display: flex; flex-direction: column; gap: 10px; }
.cw-letters { display: flex; flex-wrap: wrap; gap: 8px; justify-content: center; padding: 4px 0; }
.cw-letter-input { width: 40px; height: 40px; text-align: center; font-size: 18px; font-weight: 700; text-transform: uppercase; border-radius: 0; border: 1px solid var(--border-app); background: var(--bg-card); color: var(--text-primary); }
.cw-letter-input:hover { border-color: var(--border-hover); }
/* Focus ring, not a glow: the old `0 0 12px` bloom only reads as light against
   a dark page — on the light theme it smears into a muddy halo around the box.
   A hard 2px ring at higher alpha is crisper and carries the same signal in
   both themes, and squares off with the input instead of feathering. */
.cw-input-focused { border-color: var(--pastel-yellow); box-shadow: 0 0 0 2px color-mix(in srgb, var(--pastel-yellow) 35%, transparent); }
.cw-clue-actions { display: flex; align-items: center; justify-content: flex-end; gap: 12px; }
.cw-btn-cancel { font-family: var(--font-sans); padding: 8px 16px; font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; border-radius: 0; border: 1px solid var(--border-app); background: var(--bg-card); color: var(--text-secondary); cursor: pointer; min-height: 44px; }
.cw-btn-cancel:hover { color: var(--text-primary); border-color: var(--border-hover); }
.cw-btn-guess { font-family: var(--font-sans); padding: 8px 20px; font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; border-radius: 0; border: 1px solid var(--pastel-yellow); background: var(--pastel-yellow); color: var(--contrast-ink); cursor: pointer; min-height: 44px; }

.cw-clues { display: flex; flex-direction: column; gap: 10px; height: 100%; }
/* The head's padding is deliberately ABOVE-heavier: the h2 sits in a flex row
   beside the tabs, and the detector reads a heading with more space below it
   than above as bound to the block beneath it. 12px above vs 2px below (plus
   the panel's own top inset) keeps it reading as the panel's title. */
.cw-clues-head { display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border-app); padding: 12px 0 2px; gap: .5rem; }
.cw-clues-head h2 { font-size: 14px; color: var(--text-secondary); margin: 0; }
/* Write feedback (saving / saved-with-verdict / not-saved + Retry). The
   severity colour is the same pastel pair the cells use, and the text says
   the state outright, so the line never rests on hue alone. */
.cw-head-right { display: flex; align-items: center; gap: .5rem; margin-left: auto; flex-wrap: wrap; justify-content: flex-end; }
.cw-sync { font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 600;
  text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary);
  display: inline-flex; align-items: center; gap: .3rem; white-space: nowrap; }
.cw-sync-saved { color: var(--pastel-green); }
.cw-sync-failed { color: var(--pastel-red); }
.cw-sync-retry { font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 700;
  text-transform: uppercase; letter-spacing: .05em; color: var(--pastel-red);
  background: transparent; border: 1px solid var(--pastel-red); border-radius: 0;
  padding: .125rem .375rem; cursor: pointer; position: relative; }
.cw-sync-retry:hover { color: var(--text-primary); border-color: var(--text-primary); }
/* How-to-play disclosure. Sits at the bottom of the panel, below the
   scrolling list, so it is always reachable without scrolling to the end. */
.cw-help { border-top: 1px solid var(--border-app); padding-top: 6px; }
.cw-help summary { font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 700;
  text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary);
  cursor: pointer; padding: .125rem 0; }
.cw-help summary:hover { color: var(--text-primary); }
.cw-help-body { display: flex; flex-direction: column; gap: .5rem; padding: .25rem 0 .125rem;
  font-size: var(--fs-xs); line-height: 1.5; color: var(--text-secondary); }
.cw-help-body p { margin: 0; max-width: 42ch; }
.cw-help-legend { display: flex; flex-wrap: wrap; gap: .25rem .875rem; }
.cw-legend-item { display: inline-flex; align-items: center; gap: .3rem; }
/* The swatch reuses the exact state channel it names — border colour for the
   outline states, solid ring for live presence, dashed for stale — so the
   legend key and the board read as the same system. */
.cw-legend-item::before { content: ""; width: 10px; height: 10px; flex-shrink: 0;
  border: 2px solid var(--text-secondary); box-sizing: border-box; }
.cw-legend-editing::before { border-color: var(--pastel-yellow); }
.cw-legend-wrong::before { border-color: var(--pastel-red); }
.cw-legend-right::before { border-color: var(--pastel-green); }
.cw-legend-live::before { border-color: var(--presence-2); border-radius: 50%; }
.cw-legend-stale::before { border-color: var(--presence-2); border-radius: 50%; border-style: dashed; }
.cw-tabs { display: flex; gap: 4px; }
.cw-tab { font-family: var(--font-sans); padding: 4px 12px; font-size: var(--fs-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; border-radius: 0; border: 1px solid var(--border-app); background: transparent; color: var(--text-secondary); cursor: pointer; position: relative; }
.cw-tab::after, .cw-invite-btn::after, .cw-link-btn::after, .cw-sync-retry::after {
  content: ""; position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
  width: max(100%, 44px); height: max(100%, 44px); }
.cw-tab:hover { border-color: var(--border-hover); }
/* Active direction tab: same reasoning as .cw-focused. "Active" is read against
   the other tab, so the filled one has to be the lighter of the pair in both
   themes — with --pastel-* it became the darkest chip on a pale panel. Fill and
   ink come from the theme-stable tokens, the border still carries the hue. */
.cw-tab-active-across { background: var(--fill-yellow); color: var(--fill-ink); border-color: var(--pastel-yellow); font-weight: 700; }
.cw-tab-active-down { background: var(--fill-green); color: var(--fill-ink); border-color: var(--pastel-green); font-weight: 700; }
.cw-clue-list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding-right: 4px; }
.cw-clue-row { display: flex; gap: 10px; padding: 10px; border-radius: 0; border: 1px solid var(--border-app); cursor: pointer; }
.cw-clue-row:hover { border-color: var(--border-hover); }
.cw-clue-row-sel { background: color-mix(in srgb, var(--pastel-yellow) 4%, transparent); border-color: color-mix(in srgb, var(--pastel-yellow) 40%, transparent); }
.cw-clue-badge { width: 28px; height: 28px; flex-shrink: 0; border-radius: 0; display: flex; align-items: center; justify-content: center; font-weight: 700; font-size: var(--fs-md); background: var(--bg-cell-empty); color: var(--text-secondary); border: 1px solid var(--border-app); }
.cw-clue-body { display: flex; flex-direction: column; gap: 8px; width: 100%; }
.cw-clue-row-text { font-size: 13px; color: var(--text-secondary); line-height: 1.4; }
.cw-bubbles { display: flex; flex-wrap: wrap; gap: 4px; }
.cw-bubble { width: 20px; height: 20px; border-radius: 0; display: flex; align-items: center; justify-content: center; font-size: var(--fs-2xs); font-weight: 700; text-transform: uppercase; }
.cw-bubble-empty { background: var(--bg-cell-empty); border: 1px solid var(--border-app); opacity: 0.3; }
.cw-bubble.cw-placeholder { background: var(--bg-cell-letter); color: var(--text-primary); border: 1px solid var(--pastel-yellow); }
/* Ink is --text-primary, not --pastel-red: red letters on the 18% red wash
   measured 4.45:1 in light mode — under AA on the smallest text in the app.
   The hue still reads from the border and the wash; the letters just stop
   being the same colour as their own background's pigment. Same ink-on-wash
   rule the board cells use (--cw-ink). */
.cw-bubble.cw-incorrect { background: color-mix(in srgb, var(--pastel-red) 18%, transparent); color: var(--text-primary); border: 1px solid var(--pastel-red); }
.cw-bubble.cw-correct { background: color-mix(in srgb, var(--pastel-green) 18%, transparent); color: var(--pastel-green); border: 1px solid var(--pastel-green); }

/* Desktop tiling stretches every panel to the full workspace height, which left
   Active Clue as a ~900px column holding one clue and a row of letter boxes.
   Size it to its content instead. */
@media (min-width: 761px) {
  .ws.tiling .panel-active-clue { align-self: flex-start; height: auto; }
}

/* The compact tier (<760px) stacks panels and lets the page scroll, so panel-kit sizes
   the Board panel `height:auto` — and `container-type: size` tells it the board
   contributes no height, collapsing the panel to its 180px floor. Drop the
   height chain here so the board is sized by WIDTH alone and the panel grows to
   fit it: `inline-size` containment keeps 100cqw meaningful while letting 100cqh
   fall back to the viewport, where it never binds. */
@media (max-width: 760px) {
  .cw-board-col { height: auto; }
  .cw-board-area { flex: none; container-type: inline-size; }

  /* Mobile stacks Active Clue BELOW the board, so focusing a letter input
     scrolled the board off the top and the player typed blind. scroll-margin
     makes the browser's own focus scroll overshoot by a board's height, so the
     grid and the inputs are both on camera. Beats a scrollIntoView effect: no
     timing race with the focus scroll, and a web_sys binding for it once broke
     wasm instantiation outright (f362a0c).
     ponytail: 62vh ≈ board (up to 100vw ≈ 59vh) + players strip on a phone.
     If the board ever gets taller than the viewport, pin the clue panel
     instead. */
  .cw-letter-input { scroll-margin-top: 62vh; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// DEF-243 moved board membership onto the coordinate: a cell is a block
    /// square when it carries the `(-1, -1)` sentinel (see `Cell::is_block`).
    /// The answer is not on the client any more, so there is nothing to mark a
    /// cell playable with — the coordinate is the whole signal.
    fn cell(x: i32, y: i32) -> Cell {
        Cell {
            modifications: vec![],
            cord_x: x,
            cord_y: y,
        }
    }

    fn block() -> Cell {
        Cell {
            modifications: vec![],
            cord_x: -1,
            cord_y: -1,
        }
    }

    /// 5x5, with a block at (3,1) so the horizontal skip has something to cross
    /// and a block wall across row 3 so the vertical skip has to cross a run.
    fn fixture() -> Vec<Vec<Cell>> {
        vec![
            vec![cell(0, 0), cell(1, 0), cell(2, 0), cell(3, 0), cell(4, 0)],
            vec![cell(0, 1), cell(1, 1), cell(2, 1), block(), cell(4, 1)],
            vec![cell(0, 2), cell(1, 2), cell(2, 2), cell(3, 2), cell(4, 2)],
            vec![block(), block(), cell(2, 3), cell(3, 3), cell(4, 3)],
            vec![cell(0, 4), cell(1, 4), cell(2, 4), cell(3, 4), cell(4, 4)],
        ]
    }

    #[test]
    fn first_playable_is_the_first_non_block_in_row_major_order() {
        assert_eq!(first_playable(&fixture()), Some((0, 0)));
        let mut g = fixture();
        g[0][0] = block();
        g[0][1] = block();
        assert_eq!(first_playable(&g), Some((2, 0)));
        assert_eq!(first_playable(&[vec![block(), block()]]), None);
    }

    #[test]
    fn step_to_playable_lands_on_the_neighbour() {
        let g = fixture();
        assert_eq!(step_to_playable(&g, (1, 0), 1, 0), Some((2, 0)));
        assert_eq!(step_to_playable(&g, (2, 0), -1, 0), Some((1, 0)));
        assert_eq!(step_to_playable(&g, (2, 0), 0, 1), Some((2, 1)));
    }

    #[test]
    fn step_to_playable_skips_a_run_of_blocks_in_one_press() {
        let g = fixture();
        // (2,1) -> right crosses the block at (3,1) and lands on (4,1).
        assert_eq!(step_to_playable(&g, (2, 1), 1, 0), Some((4, 1)));
        // (1,2) -> down crosses the block at (1,3) and lands on (1,4).
        assert_eq!(step_to_playable(&g, (1, 2), 0, 1), Some((1, 4)));
    }

    /// The same skip, on a grid built by `board_state_from_actions` — the only
    /// thing that builds the grid in production (`game_play.rs`), and the one
    /// that emits the `(-1, -1)` sentinel for a block. The hand-built `fixture()`
    /// above would pass even if the ray looked blocks up by coordinate, because
    /// a coordinate lookup cannot find a sentinel cell; this one cannot.
    #[test]
    fn step_to_playable_skips_sentinel_blocks_on_a_real_board() {
        // 5x1: "AB" across at (0,0) and "C" across at (4,0). (2,0) and (3,0) are
        // covered by no answer, so they come back as block sentinels.
        let qs = vec![
            compute_answer_map(
                &Question {
                    number: 1,
                    len: 2,
                    question_text: "Pair".to_string(),
                    root_x: 0,
                    root_y: 0,
                    direction: Direction::Across,
                },
                &[],
            ),
            compute_answer_map(
                &Question {
                    number: 2,
                    len: 1,
                    question_text: "Solo".to_string(),
                    root_x: 4,
                    root_y: 0,
                    direction: Direction::Across,
                },
                &[],
            ),
        ];
        let size = compute_board_size(&qs);
        let g = board_state_from_actions(size, &[], &qs);

        assert!(g[0][2].is_block() && g[0][3].is_block(), "fixture premise");
        // (1,0) -> right walks both sentinels and lands on (4,0).
        assert_eq!(step_to_playable(&g, (1, 0), 1, 0), Some((4, 0)));
        // ...and off the far edge is still None, so the sentinel did not turn
        // "left the board" into "keep going forever".
        assert_eq!(step_to_playable(&g, (4, 0), 1, 0), None);
    }

    #[test]
    fn step_to_playable_at_a_wall_or_an_edge_does_not_move() {
        let g = fixture();
        // Block immediately west of (0,1): nothing playable left, so the cursor
        // must stay put rather than wrapping or jumping to the far side.
        assert_eq!(step_to_playable(&g, (0, 1), -1, 0), None);
        // Grid edge.
        assert_eq!(step_to_playable(&g, (0, 0), 0, -1), None);
        assert_eq!(step_to_playable(&g, (4, 4), 1, 0), None);
    }

    #[test]
    fn cell_aria_label_reads_number_letter_then_state() {
        assert_eq!(
            cell_aria_label(Some(14), "R", Some(ActionType::CorrectGuess), None),
            "14, R, correct"
        );
        assert_eq!(
            cell_aria_label(Some(7), "", Some(ActionType::CorrectGuess), None),
            "7, empty"
        );
        assert_eq!(
            cell_aria_label(None, "Q", Some(ActionType::IncorrectGuess), None),
            "Q, incorrect"
        );
        // A letter in the local draft but not yet committed is still drawn, so
        // it is announced rather than read as empty.
        assert_eq!(
            cell_aria_label(Some(3), "A", None, None),
            "3, A, in progress"
        );
    }

    /// DEF-215 A14: a cell under a remote ring has to say whose cell it is, and
    /// a stale one has to say the position is last-known — without that append
    /// costing the letter its state suffix.
    #[test]
    fn cell_aria_label_appends_presence_without_dropping_state() {
        assert_eq!(
            cell_aria_label(
                Some(14),
                "R",
                Some(ActionType::CorrectGuess),
                Some(("ada", false))
            ),
            "14, R, correct, ada"
        );
        assert_eq!(
            cell_aria_label(
                Some(14),
                "R",
                Some(ActionType::CorrectGuess),
                Some(("ada", true))
            ),
            "14, R, correct, ada (last known — reconnecting)"
        );
        // An empty square on somebody's ring still carries the ring: "empty" is
        // the whole state there is, so there is nothing to preserve behind it.
        assert_eq!(
            cell_aria_label(
                Some(7),
                "",
                Some(ActionType::CorrectGuess),
                Some(("ada", true))
            ),
            "7, empty, ada (last known — reconnecting)"
        );
        // In-progress is the default suffix and must survive the same way.
        assert_eq!(
            cell_aria_label(None, "A", None, Some(("bo", true))),
            "A, in progress, bo (last known — reconnecting)"
        );
    }

    /// A word running across row 1 from (0,1) for 3 cells, and one running down
    /// column 2 from (2,0) for 3 cells. They cross at (2,1), which is the
    /// coordinate the keyboard resolver has to attribute to one or the other.
    fn crossing_maps() -> Vec<QuestionWithAnswerMap> {
        let across = Question {
            number: 1,
            len: 3,
            question_text: "A pet".to_string(),
            root_x: 0,
            root_y: 1,
            direction: Direction::Across,
        };
        let down = Question {
            number: 2,
            len: 3,
            question_text: "Nickel tally".to_string(),
            root_x: 2,
            root_y: 0,
            direction: Direction::Down,
        };
        vec![
            compute_answer_map(&across, &[]),
            compute_answer_map(&down, &[]),
        ]
    }

    #[test]
    fn covers_cell_matches_only_the_cells_of_that_word() {
        let maps = crossing_maps();
        // (0,1) and (1,1) belong to the across word only; (2,1) to both.
        assert!(covers_cell(&maps[0], 0, 1));
        assert!(covers_cell(&maps[0], 2, 1));
        assert!(!covers_cell(&maps[0], 3, 1));
        assert!(covers_cell(&maps[1], 2, 1));
        assert!(!covers_cell(&maps[1], 2, 3));
        // The crossing cell is genuinely in both, which is what makes the
        // direction-first ordering in the resolver load-bearing.
        assert!(covers_cell(&maps[0], 2, 1) && covers_cell(&maps[1], 2, 1));
    }

    #[test]
    fn answer_map_index_round_trips_to_the_cell_it_owns() {
        // The resolver pairs a covering word with `position(..)`, and the
        // cursor is then derived from that index. If the two ever disagreed
        // the cursor would render on a different cell than the one resolved,
        // so pin the round trip on the crossing word.
        let maps = crossing_maps();
        for (i, c) in maps[1].answer_map.iter().enumerate() {
            let found = maps[1]
                .answer_map
                .iter()
                .position(|k| k.cord_x == c.cord_x && k.cord_y == c.cord_y);
            assert_eq!(found, Some(i));
        }
        // Index 1 of the down word is the crossing cell (2,1).
        let crossing = &maps[1].answer_map[1];
        assert_eq!((crossing.cord_x, crossing.cord_y), (2, 1));
        assert_eq!(
            maps[1]
                .answer_map
                .iter()
                .position(|c| c.cord_x == 2 && c.cord_y == 1),
            Some(1)
        );
    }

    /// DEF-227: the outcome class used to be the `else` of focus/selection, so
    /// it vanished in exactly the state that needs it — a wrong guess keeps the
    /// word selected so the player can fix it in place. These pin the composition
    /// rule directly rather than the old chain order.
    #[test]
    fn outcome_class_survives_focus_and_selection() {
        let wrong = cell_classes(true, true, Some(ActionType::IncorrectGuess));
        assert!(wrong.contains("cw-incorrect"), "{wrong}");
        assert!(wrong.contains("cw-focused"), "{wrong}");
        assert!(wrong.contains("cw-selected"), "{wrong}");

        // The cursor cell of a wrong word is the one most likely to swallow the
        // red, and it is also the cell the player is looking straight at.
        let focused_only = cell_classes(true, false, Some(ActionType::IncorrectGuess));
        assert!(focused_only.contains("cw-incorrect"), "{focused_only}");
        assert!(focused_only.contains("cw-focused"), "{focused_only}");

        // The green and the placeholder take the same route, so neither is a
        // special case left behind by the fix.
        assert!(cell_classes(true, true, Some(ActionType::CorrectGuess)).contains("cw-correct"));
        assert!(cell_classes(true, true, Some(ActionType::Placeholder)).contains("cw-placeholder"));
    }

    /// A cell with no action yet still gets exactly one state class, and the
    /// focus/selection classes are not conditional on there being one.
    #[test]
    fn cell_classes_emit_no_outcome_without_an_action() {
        assert_eq!(cell_classes(false, false, None), "cw-cell cw-letter");
        assert_eq!(
            cell_classes(true, false, None),
            "cw-cell cw-letter cw-focused"
        );
        assert_eq!(
            cell_classes(false, true, None),
            "cw-cell cw-letter cw-selected"
        );
        // A cell can be neither focused nor selected yet still be answered —
        // that is the case the old `else` handled and the only one that worked.
        assert_eq!(
            cell_classes(false, false, Some(ActionType::CorrectGuess)),
            "cw-cell cw-letter cw-correct"
        );
    }

    /// The composed classes must stay distinct so the CSS channels can be
    /// addressed independently: one class per concern, never a combined selector.
    #[test]
    fn cell_classes_never_repeat_a_state_class() {
        for focused in [false, true] {
            for selected in [false, true] {
                for at in [
                    None,
                    Some(ActionType::Placeholder),
                    Some(ActionType::IncorrectGuess),
                    Some(ActionType::CorrectGuess),
                ] {
                    let cls = cell_classes(focused, selected, at);
                    let mut seen: Vec<&str> = cls.split_whitespace().collect();
                    let before = seen.len();
                    seen.sort_unstable();
                    seen.dedup();
                    assert_eq!(before, seen.len(), "repeated class in `{cls}`");
                }
            }
        }
    }

    /// DEF-103: a correct guess used to end the word, and DEF-243 took the
    /// decision with it. The client no longer holds the answer, so "is this word
    /// right" can only be answered by the verdicts the server returns — and only
    /// on positive evidence, never by the letters sitting in the editor.
    #[test]
    fn word_solved_needs_a_verdict_for_every_cell() {
        let word = vec![(0, 0), (1, 0), (2, 0)];
        let all_right = HashMap::from([((0, 0), true), ((1, 0), true), ((2, 0), true)]);
        assert!(word_solved(&word, &all_right));

        // One cell the server marked wrong: not solved.
        let mut one_wrong = all_right.clone();
        one_wrong.insert((1, 0), false);
        assert!(!word_solved(&word, &one_wrong));

        // One cell the server never spoke about: unknown, so not solved. This is
        // the case a locally-computed answer gets wrong.
        let mut partial = all_right.clone();
        partial.remove(&(2, 0));
        assert!(!word_solved(&word, &partial));

        // Nothing at all — the shape an untouched editor compares equal to.
        assert!(!word_solved(&word, &HashMap::new()));

        // An empty word is not a solved word; otherwise the clear would fire on
        // a submit that had no slots to begin with.
        assert!(!word_solved(&[], &all_right));
    }

    /// Verdicts for cells outside the open word must not stand in for it. The
    /// `addActions` reply covers the whole submitted batch, and a reconciled
    /// board carries every letter ever played, so a `correct` somewhere else is
    /// not evidence about this word.
    #[test]
    fn word_solved_ignores_verdicts_outside_the_word() {
        let word = vec![(0, 0), (1, 0)];
        let verdicts = HashMap::from([
            ((0, 0), true),
            ((1, 0), true),
            ((7, 7), true),
            ((8, 8), true),
        ]);
        assert!(word_solved(&word, &verdicts));

        // The same letters, but one of the word's cells is wrong and the surplus
        // cells are right: still not solved.
        let mixed = HashMap::from([
            ((0, 0), true),
            ((1, 0), false),
            ((7, 7), true),
            ((8, 8), true),
        ]);
        assert!(!word_solved(&word, &mixed));
    }

    /// DEF-235: the presence ring never appeared for a live co-op player.
    ///
    /// Presence is fire-and-forget, so a reconnect silently dropped every
    /// publish we had made over the dead socket. Nothing republished, which
    /// left the partner rendering us as idle for the rest of the session — the
    /// exact "no ring anywhere in the DOM after 20s" the demo tour hit. The
    /// repair rides the reconcile, so the guard has to fire on every reconnect,
    /// not just the first.
    #[test]
    fn reconnect_after_the_first_live_still_triggers_a_reconcile() {
        use net::ConnectionState::*;

        let mut seen_live = false;

        // Cold start: Connecting → Live is the first-ever Live. Nothing to
        // reconcile (and re-fetching would drop the un-submitted optimistic
        // merge), so it must not fire.
        assert!(!is_reconcile_trigger(&Connecting, &mut seen_live));
        assert!(!is_reconcile_trigger(&Live, &mut seen_live));

        // The blip that broke it: the socket drops and comes back. This is the
        // transition that must reconcile — and therefore must re-announce
        // presence.
        assert!(!is_reconcile_trigger(
            &Reconnecting { attempt: 1 },
            &mut seen_live
        ));
        assert!(
            is_reconcile_trigger(&Live, &mut seen_live),
            "a reconnect must reconcile so presence is re-announced"
        );

        // And it keeps firing: a player whose connection flaps repeatedly must
        // be re-announced on every repair, not just the first.
        assert!(!is_reconcile_trigger(
            &Reconnecting { attempt: 2 },
            &mut seen_live
        ));
        assert!(is_reconcile_trigger(&Live, &mut seen_live));
    }

    /// A `Live` is the only thing that reconciles, and a non-`Live` state must
    /// not consume the first-`Live` slot. `Connecting`, `Reconnecting` and
    /// `Offline` leave the board alone and leave `seen_live` untouched, so the
    /// first-ever `Live` is still recognised as the first one and skipped.
    #[test]
    fn non_live_transitions_never_reconcile_or_consume_the_first_live() {
        use net::ConnectionState::*;

        let mut seen_live = false;

        for state in [Connecting, Reconnecting { attempt: 3 }, Offline] {
            assert!(
                !is_reconcile_trigger(&state, &mut seen_live),
                "{state:?} must not reconcile"
            );
        }
        assert!(
            !seen_live,
            "a non-Live state must not consume the first-Live slot"
        );
        // The first-ever `Live` is skipped, not reconciled...
        assert!(!is_reconcile_trigger(&Live, &mut seen_live));
        // ...and every `Live` after it is a genuine reconnect.
        assert!(is_reconcile_trigger(&Live, &mut seen_live));
    }
}
