import { PrismaClient } from "@prisma/client";
import { env, pipeline } from "@xenova/transformers";
import fs from "node:fs";

const WIDTH = Number.parseInt(process.env.CROSSWORD_WIDTH ?? "10", 10);
const HEIGHT = Number.parseInt(process.env.CROSSWORD_HEIGHT ?? "10", 10);
const MIN_WORD_LENGTH = 3;
const MAX_WORD_LENGTH = Math.min(Number.parseInt(process.env.CROSSWORD_MAX_WORD_LENGTH ?? String(Math.max(WIDTH, HEIGHT)), 10), Math.max(WIDTH, HEIGHT));
const TARGET_WORDS = Number.parseInt(process.env.CROSSWORD_TARGET_WORDS ?? "16", 10);
const MAX_ATTEMPTS = Number.parseInt(process.env.CROSSWORD_MAX_ATTEMPTS ?? "8000", 10);
const SEED = Number.parseInt(process.env.CROSSWORD_SEED ?? "42", 10);
const RUNS = Number.parseInt(process.env.CROSSWORD_RUNS ?? "40", 10);
const TOPIC = process.env.CROSSWORD_TOPIC ?? "space exploration and planetary science";
const EMBEDDING_MODEL_PATH = "data/crossword/models/all-MiniLM-L6-v2";
const WORDNET_COUNT_PATH = "data/crossword/wordnet/dict/cntlist";
const EMBEDDING_CANDIDATE_LIMIT = 4000;
const EMBEDDING_BATCH_SIZE = 32;
const COMMON_WORDS = new Set([
  "ABOUT", "ABOVE", "ACTOR", "ADAPT", "ADMIT", "ADOPT", "ADORE", "ADULT", "AFTER", "AGAIN",
  "AGENT", "AGILE", "AGREE", "AHEAD", "ALARM", "ALBUM", "ALERT", "ALIEN", "ALIGN", "ALIKE",
  "ALIVE", "ALLOW", "ALONE", "ALONG", "ALOUD", "ALTER", "AMBER", "AMEND", "AMONG", "AMPLE",
  "ANGEL", "ANGER", "ANGLE", "ANGRY", "APART", "APPLE", "APPLY", "APRIL", "ARGON", "ARISE",
  "ARMOR", "AROMA", "AROSE", "ARRAY", "ARROW", "ASIDE", "ASSET", "ATLAS", "ATONE", "AUDIO",
  "AVAIL", "AVERT", "AVOID", "AWAIT", "AWAKE", "AWARD", "AWARE", "BASIC", "BATCH", "BEACH",
  "BEARD", "BEAST", "BEGIN", "BEING", "BELOW", "BENCH", "BIRTH", "BLACK", "BLEND", "BLIND",
  "BLINK", "BLOCK", "BLOOM", "BOARD", "BOOST", "BOUND", "BRAIN", "BRAND", "BRAVE", "BREAD",
  "BREAK", "BRICK", "BRIDE", "BRIEF", "BRING", "BROAD", "BROKE", "BROWN", "BUILD", "BUILT",
  "CABLE", "CARRY", "CHAIR", "CHAIN", "CHART", "CHASE", "CHEAP", "CHECK", "CHEST", "CHIEF",
  "CHILD", "CIVIL", "CLAIM", "CLASS", "CLEAN", "CLEAR", "CLIMB", "CLOCK", "CLOUD", "COACH",
  "COAST", "COUNT", "COURT", "COVER", "CRAFT", "CRASH", "CREAM", "CRIME", "CROSS", "CROWD",
  "BUST", "DANCE", "DEATH", "DEPTH", "DIME", "DOUBT", "DREAM", "DRESS", "DRINK", "DRIVE", "EARTH", "EMPTY",
  "ENEMY", "ENJOY", "ENTER", "EQUAL", "EVENT", "EVERY", "EXACT", "EXIST", "FAITH", "FALSE",
  "FAULT", "FIELD", "FIGHT", "FINAL", "FIRST", "FLASH", "FLOOR", "FOCUS", "FORCE", "FRAME",
  "FRESH", "FRONT", "FRUIT", "GIANT", "GRACE", "GRADE", "GRAND", "GRANT", "GRAPH", "GRASS",
  "GREAT", "GREEN", "GROUP", "GUARD", "GUESS", "GUEST", "GUIDE", "HAPPY", "HEART", "HEAVY",
  "HORSE", "HOTEL", "HOUSE", "IMAGE", "INDEX", "INNER", "INPUT", "ISSUE", "JUDGE", "KNIFE",
  "LABEL", "LARGE", "LASER", "LAYER", "LEARN", "LEAST", "LEAVE", "LEGAL", "LEVEL", "LIGHT", "LIMIT",
  "LOCAL", "LOGIC", "LOOSE", "LOWER", "LUCKY", "LUNAR", "MAJOR", "MATCH", "MAYBE", "METAL",
  "MODEL", "MONEY", "MONTH", "MOTOR", "MOUNT", "MOUSE", "MOUTH", "MUSIC", "NEVER", "NIGHT",
  "NORTH", "NOVEL", "NURSE", "OCEAN", "OFFER", "ORDER", "OTHER", "OUTER", "OWNER", "PANEL",
  "PARTY", "PEACE", "PHONE", "PIANO", "PIECE", "PILOT", "PITCH", "PLACE", "PLAIN", "PLANE",
  "PINT", "PLANT", "PLATE", "POINT", "POWER", "PRESS", "PRICE", "PRIDE", "PRIME", "PRINT", "PRIZE",
  "PROOF", "QUEEN", "QUICK", "QUIET", "RADIO", "RAISE", "RANGE", "RATIO", "REACH", "READY",
  "RIGHT", "RIVER", "ROBOT", "ROUGH", "ROUND", "ROUTE", "ROYAL", "SCALE", "SCENE", "SCOPE",
  "SCORE", "SENSE", "SERVE", "SHADE", "SHAKE", "SHAPE", "SHARE", "SHARP", "SHEET", "SHIFT",
  "SHIRT", "SHOCK", "SHORT", "SIGHT", "SKILL", "SLEEP", "SMALL", "SMART", "SMILE", "SOLAR",
  "SOLID", "SOUND", "SOUTH", "SPACE", "SPEAK", "SPEED", "SPEND", "SPORT", "STAFF", "STAGE",
  "STAND", "START", "STATE", "STEAM", "STEEL", "STICK", "STILL", "STONE", "STORE", "STORM",
  "STORY", "STRIP", "STUDY", "STYLE", "SUGAR", "TABLE", "TASTE", "TEACH", "THEME", "THING",
  "THINK", "THIRD", "THROW", "TIGHT", "TITLE", "TODAY", "TOPIC", "TOTAL", "TOUCH", "TOUGH",
  "TOUR", "TOWER", "TRACK", "TRADE", "TRAIL", "TRAIN", "TREND", "TRIAL", "TRUST", "TRUTH", "UNDER",
  "UNION", "UNITY", "UPPER", "VALUE", "VIDEO", "VISIT", "VOICE", "WASTE", "WATCH", "WATER",
  "WHEEL", "WHERE", "WHILE", "WHITE", "WHOLE", "WOMAN", "WORLD", "WORTH", "WRITE", "WRONG",
  "YOUNG",
]);
const BAD_GLOSS_PATTERNS = [
  /\babbreviation\b/i,
  /\bacronym\b/i,
  /\bRoman numeral\b/i,
  /\bunit of measurement\b/i,
  /\bgenus\b/i,
  /\bfamily\b/i,
  /\btaxonomic\b/i,
  /\bpidgin\b/i,
  /\bvariety of zircon\b/i,
  /\barchaic\b/i,
  /\bobsolete\b/i,
];

function createRng(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 2 ** 32;
  };
}

let random = createRng(SEED);

function sample(values) {
  return values[Math.floor(random() * values.length)];
}

function shuffle(values) {
  const copy = [...values];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

function resetRandom(seed) {
  random = createRng(seed);
}

function chunk(values, size) {
  const chunks = [];
  for (let index = 0; index < values.length; index += size) {
    chunks.push(values.slice(index, index + size));
  }
  return chunks;
}

function emptyGrid() {
  return Array.from({ length: HEIGHT }, () => Array(WIDTH).fill(null));
}

function cellAt(word, index) {
  return word.direction === "ACROSS"
    ? { x: word.x + index, y: word.y }
    : { x: word.x, y: word.y + index };
}

function inBounds(x, y) {
  return x >= 0 && x < WIDTH && y >= 0 && y < HEIGHT;
}

function letterAt(grid, x, y) {
  if (!inBounds(x, y)) return null;
  return grid[y][x];
}

function occupiedDirectionsAt(placedWords, x, y) {
  const directions = new Set();
  for (const placed of placedWords) {
    for (let i = 0; i < placed.word.length; i++) {
      const cell = cellAt(placed, i);
      if (cell.x === x && cell.y === y) directions.add(placed.direction);
    }
  }
  return directions;
}

function canPlace(grid, placedWords, candidate) {
  const dx = candidate.direction === "ACROSS" ? 1 : 0;
  const dy = candidate.direction === "DOWN" ? 1 : 0;
  const before = { x: candidate.x - dx, y: candidate.y - dy };
  const after = {
    x: candidate.x + dx * candidate.word.length,
    y: candidate.y + dy * candidate.word.length,
  };

  if (letterAt(grid, before.x, before.y) || letterAt(grid, after.x, after.y)) {
    return false;
  }

  let crossingCount = 0;
  for (let i = 0; i < candidate.word.length; i++) {
    const x = candidate.x + dx * i;
    const y = candidate.y + dy * i;
    if (!inBounds(x, y)) return false;

    const existing = grid[y][x];
    if (existing && existing !== candidate.word[i]) return false;
    if (existing === candidate.word[i]) {
      if (occupiedDirectionsAt(placedWords, x, y).has(candidate.direction)) return false;
      crossingCount++;
      continue;
    }

    if (candidate.direction === "ACROSS") {
      if (letterAt(grid, x, y - 1) || letterAt(grid, x, y + 1)) return false;
    } else if (letterAt(grid, x - 1, y) || letterAt(grid, x + 1, y)) {
      return false;
    }
  }

  return placedWords.length === 0 || crossingCount > 0;
}

function crossingCount(grid, candidate) {
  const dx = candidate.direction === "ACROSS" ? 1 : 0;
  const dy = candidate.direction === "DOWN" ? 1 : 0;
  let count = 0;
  for (let i = 0; i < candidate.word.length; i++) {
    const x = candidate.x + dx * i;
    const y = candidate.y + dy * i;
    if (letterAt(grid, x, y) === candidate.word[i]) count++;
  }
  return count;
}

function placeWord(grid, placedWords, candidate) {
  for (let i = 0; i < candidate.word.length; i++) {
    const cell = cellAt(candidate, i);
    grid[cell.y][cell.x] = candidate.word[i];
  }
  placedWords.push(candidate);
}

function renderGrid(grid) {
  return grid
    .map((row) => row.map((letter) => letter ?? "#").join(" "))
    .join("\n");
}

function validateGrid(grid, placedWords, dictionarySet) {
  const answers = new Set();
  for (const placed of placedWords) {
    if (answers.has(placed.word)) throw new Error(`Duplicate answer: ${placed.word}`);
    answers.add(placed.word);
    if (!dictionarySet.has(placed.word)) throw new Error(`Answer not in dictionary: ${placed.word}`);
    if (!canReadPlacedWord(grid, placed)) throw new Error(`Placed word is not readable: ${placed.word}`);
    assertMaximalSlot(grid, placed);
  }

  const extracted = extractSlots(grid);
  const placedKeys = new Set(placedWords.map(slotKey));
  for (const slot of extracted) {
    if (slot.word.length < MIN_WORD_LENGTH) continue;
    if (!placedKeys.has(slotKey(slot))) {
      throw new Error(`Unclued accidental slot found: ${slot.word} at ${slot.x},${slot.y}`);
    }
  }
}

function canReadPlacedWord(grid, placed) {
  for (let i = 0; i < placed.word.length; i++) {
    const cell = cellAt(placed, i);
    if (grid[cell.y][cell.x] !== placed.word[i]) return false;
  }
  return true;
}

function assertMaximalSlot(grid, placed) {
  const dx = placed.direction === "ACROSS" ? 1 : 0;
  const dy = placed.direction === "DOWN" ? 1 : 0;
  const before = { x: placed.x - dx, y: placed.y - dy };
  const after = {
    x: placed.x + dx * placed.word.length,
    y: placed.y + dy * placed.word.length,
  };

  if (letterAt(grid, before.x, before.y) || letterAt(grid, after.x, after.y)) {
    throw new Error(`Placed word is not a maximal slot: ${placed.word}`);
  }
}

function slotKey(slot) {
  return `${slot.direction}:${slot.x}:${slot.y}:${slot.word}`;
}

function extractSlots(grid) {
  const slots = [];

  for (let y = 0; y < HEIGHT; y++) {
    let x = 0;
    while (x < WIDTH) {
      while (x < WIDTH && !grid[y][x]) x++;
      const start = x;
      let word = "";
      while (x < WIDTH && grid[y][x]) word += grid[y][x++];
      if (word.length >= MIN_WORD_LENGTH) {
        slots.push({ direction: "ACROSS", x: start, y, word });
      }
    }
  }

  for (let x = 0; x < WIDTH; x++) {
    let y = 0;
    while (y < HEIGHT) {
      while (y < HEIGHT && !grid[y][x]) y++;
      const start = y;
      let word = "";
      while (y < HEIGHT && grid[y][x]) word += grid[y++][x];
      if (word.length >= MIN_WORD_LENGTH) {
        slots.push({ direction: "DOWN", x, y: start, word });
      }
    }
  }

  return slots;
}

function numberWords(placedWords) {
  const starts = new Map();
  for (const word of placedWords) {
    starts.set(`${word.x}:${word.y}`, null);
  }

  let number = 1;
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const key = `${x}:${y}`;
      if (starts.has(key)) starts.set(key, number++);
    }
  }

  return placedWords
    .map((word) => ({ ...word, number: starts.get(`${word.x}:${word.y}`) }))
    .sort((a, b) => a.number - b.number || a.direction.localeCompare(b.direction));
}

async function loadDictionary(prisma) {
  const frequencyScores = loadWordNetFrequencyScores(WORDNET_COUNT_PATH);
  const rows = await prisma.dictionaryWord.findMany({
    where: {
      length: { gte: MIN_WORD_LENGTH, lte: MAX_WORD_LENGTH },
      definitions: { some: {} },
    },
    select: {
      word: true,
      length: true,
      definitions: {
        select: {
          partOfSpeech: true,
          gloss: true,
        },
        take: 8,
      },
    },
    orderBy: [{ length: "desc" }, { word: "asc" }],
  });
  const scoredWords = rows
    .map((row) => ({
      ...row,
      frequencyScore: frequencyScores.get(row.word) ?? 0,
      qualityScore: scoreWordQuality(row, frequencyScores.get(row.word) ?? 0),
    }))
    .filter((row) => row.qualityScore >= 3);

  if (scoredWords.length === 0) {
    throw new Error("Dictionary is empty. Run `npm run seed:dictionary` first.");
  }

  const topicScores = await scoreCandidatesByEmbedding(TOPIC, scoredWords);
  const topicWords = new Set(
    [...topicScores.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 800)
      .map(([word]) => word)
  );
  const byLetter = new Map();
  const byLength = new Map();
  for (const row of scoredWords) {
    if (!byLength.has(row.length)) byLength.set(row.length, []);
    byLength.get(row.length).push(row.word);
    for (const letter of new Set(row.word)) {
      if (!byLetter.has(letter)) byLetter.set(letter, []);
      byLetter.get(letter).push(row.word);
    }
  }

  const scoreByWord = new Map(scoredWords.map((row) => [row.word, row.qualityScore]));
  const frequencyByWord = new Map(scoredWords.map((row) => [row.word, row.frequencyScore]));
  for (const [letter, letterWords] of byLetter) {
    letterWords.sort(
      (a, b) =>
        (topicScores.get(b) ?? 0) - (topicScores.get(a) ?? 0) ||
        (frequencyByWord.get(b) ?? 0) - (frequencyByWord.get(a) ?? 0) ||
        scoreByWord.get(b) - scoreByWord.get(a) ||
        a.length - b.length
    );
    byLetter.set(letter, letterWords.slice(0, 2500));
  }

  return {
    words: scoredWords.map((row) => row.word),
    dictionarySet: new Set(scoredWords.map((row) => row.word)),
    topicWords,
    topicScores,
    qualityScores: scoreByWord,
    frequencyScores: frequencyByWord,
    byLetter,
    byLength,
  };
}

function loadWordNetFrequencyScores(filePath) {
  const scores = new Map();
  const contents = fs.readFileSync(filePath, "utf8");

  for (const line of contents.split(/\r?\n/)) {
    if (!line.trim()) continue;
    const [countText, senseKey] = line.trim().split(/\s+/);
    const count = Number.parseInt(countText, 10);
    const lemma = senseKey?.split("%")[0];
    if (!Number.isFinite(count) || !lemma) continue;
    if (lemma.includes("_") || lemma.includes("-") || lemma.includes("'")) continue;

    const word = lemma.toUpperCase();
    if (!/^[A-Z]+$/.test(word)) continue;
    scores.set(word, (scores.get(word) ?? 0) + count);
  }

  return scores;
}

function scoreWordQuality(row, frequencyScore) {
  const word = row.word;
  const glossText = row.definitions.map((definition) => definition.gloss).join(" ");
  const partsOfSpeech = new Set(row.definitions.map((definition) => definition.partOfSpeech));
  let score = 0;

  if (!/[AEIOUY]/.test(word)) return -10;
  if (word.length <= 5 && !COMMON_WORDS.has(word) && frequencyScore < 5) return -10;
  if (/(.)\1\1/.test(word)) score -= 2;
  if (/[QXZ]/.test(word)) score -= 1;
  if (word.length >= 4 && word.length <= 8) score += 2;
  if (word.length === 3) score -= 1;
  if (word.length >= 9) score -= 1;
  if (COMMON_WORDS.has(word)) score += 5;
  if (frequencyScore >= 50) score += 5;
  else if (frequencyScore >= 20) score += 4;
  else if (frequencyScore >= 10) score += 3;
  else if (frequencyScore >= 5) score += 2;
  if (partsOfSpeech.has("NOUN")) score += 1;
  if (partsOfSpeech.has("VERB")) score += 1;
  if (partsOfSpeech.has("ADJECTIVE")) score += 1;
  if (row.definitions.length > 1) score += 1;
  if (BAD_GLOSS_PATTERNS.some((pattern) => pattern.test(glossText))) score -= 6;
  if (/^[A-Z]{1,3}$/.test(word)) score -= 4;
  if (/^[XVI]+$/.test(word)) score -= 8;

  return score;
}

async function scoreCandidatesByEmbedding(topic, candidates) {
  env.allowRemoteModels = false;
  env.allowLocalModels = true;
  env.localModelPath = ".";

  const extractor = await pipeline("feature-extraction", EMBEDDING_MODEL_PATH, { quantized: true });
  const topicEmbedding = await embedText(extractor, topic);
  const candidateRows = [...candidates]
    .sort((a, b) => b.qualityScore - a.qualityScore)
    .slice(0, EMBEDDING_CANDIDATE_LIMIT);

  const scores = new Map();
  for (const rows of chunk(candidateRows, EMBEDDING_BATCH_SIZE)) {
    const texts = rows.map(candidateEmbeddingText);
    const embeddings = await embedTexts(extractor, texts);
    for (let index = 0; index < rows.length; index++) {
      scores.set(rows[index].word, cosineSimilarity(topicEmbedding, embeddings[index]));
    }
  }

  return scores;
}

function candidateEmbeddingText(row) {
  const glosses = row.definitions
    .slice(0, 4)
    .map((definition) => definition.gloss)
    .join("; ");
  return `${row.word.toLowerCase()}: ${glosses}`;
}

async function embedText(extractor, text) {
  const output = await extractor(text, { pooling: "mean", normalize: true });
  return Array.from(output.data);
}

async function embedTexts(extractor, texts) {
  const output = await extractor(texts, { pooling: "mean", normalize: true });
  const [count, dimensions] = output.dims;
  const embeddings = [];
  for (let row = 0; row < count; row++) {
    embeddings.push(Array.from(output.data.slice(row * dimensions, (row + 1) * dimensions)));
  }
  return embeddings;
}

function cosineSimilarity(a, b) {
  let sum = 0;
  for (let index = 0; index < a.length; index++) sum += a[index] * b[index];
  return sum;
}

function generate(dictionary) {
  const grid = emptyGrid();
  const placedWords = [];
  const seedWords = [
    ...(dictionary.byLength.get(5) ?? []).filter((word) => dictionary.topicWords.has(word)),
    ...(dictionary.byLength.get(6) ?? []).filter((word) => dictionary.topicWords.has(word)),
    ...(dictionary.byLength.get(7) ?? []).filter((word) => dictionary.topicWords.has(word)),
    ...(dictionary.byLength.get(8) ?? []).filter((word) => dictionary.topicWords.has(word)),
    ...(dictionary.byLength.get(5) ?? []),
    ...(dictionary.byLength.get(6) ?? []),
    ...(dictionary.byLength.get(7) ?? []),
    ...(dictionary.byLength.get(8) ?? []),
  ].sort((a, b) => (dictionary.topicScores.get(b) ?? 0) - (dictionary.topicScores.get(a) ?? 0));

  for (const seedWord of seedWords) {
    const candidate = {
      word: seedWord,
      direction: "ACROSS",
      x: Math.floor((WIDTH - seedWord.length) / 2),
      y: Math.floor(HEIGHT / 2),
    };
    if (canPlace(grid, placedWords, candidate)) {
      placeWord(grid, placedWords, candidate);
      break;
    }
  }

  for (let attempt = 0; attempt < MAX_ATTEMPTS && placedWords.length < TARGET_WORDS; attempt++) {
    const candidates = findBestPlacements(grid, placedWords, dictionary);
    if (candidates.length === 0) break;
    const chosen = sample(candidates.slice(0, Math.min(12, candidates.length)));
    placeWord(grid, placedWords, chosen);
  }

  return { grid, placedWords };
}

function findBestPlacements(grid, placedWords, dictionary) {
  const usedWords = new Set(placedWords.map((placed) => placed.word));
  const placements = [];

  for (const anchor of shuffle(placedWords)) {
    for (const anchorIndex of shuffle(Array.from({ length: anchor.word.length }, (_, index) => index))) {
      const anchorCell = cellAt(anchor, anchorIndex);
      const letter = anchor.word[anchorIndex];
      const direction = anchor.direction === "ACROSS" ? "DOWN" : "ACROSS";
      const words = dictionary.byLetter.get(letter) ?? [];

      for (const word of words.slice(0, 650)) {
        if (usedWords.has(word)) continue;

        const matchingIndexes = [];
        for (let index = 0; index < word.length; index++) {
          if (word[index] === letter) matchingIndexes.push(index);
        }

        for (const wordIndex of shuffle(matchingIndexes)) {
          const candidate = {
            word,
            direction,
            x: direction === "ACROSS" ? anchorCell.x - wordIndex : anchorCell.x,
            y: direction === "DOWN" ? anchorCell.y - wordIndex : anchorCell.y,
          };

          if (!canPlace(grid, placedWords, candidate)) continue;

          const crosses = crossingCount(grid, candidate);
          const topicScore = dictionary.topicScores.get(word) ?? 0;
          const qualityScore = dictionary.qualityScores.get(word) ?? 0;
          const frequencyScore = dictionary.frequencyScores.get(word) ?? 0;
          const lengthScore = word.length >= 4 && word.length <= 7 ? 8 : 0;
          placements.push({
            ...candidate,
            placementScore: crosses * 50 + topicScore * 100 + qualityScore * 5 + Math.log1p(frequencyScore) * 8 + lengthScore,
          });
        }
      }
    }

    if (placements.length > 1000) break;
  }

  return placements.sort((a, b) => b.placementScore - a.placementScore);
}

function scoreBoard(result, dictionary) {
  const filledCells = result.grid.flat().filter(Boolean).length;
  const topicScore = result.placedWords.reduce(
    (sum, word) => sum + (dictionary.topicScores.get(word.word) ?? 0),
    0
  );
  const topicWordCount = result.placedWords.filter((word) => dictionary.topicWords.has(word.word)).length;
  return result.placedWords.length * 1000 + filledCells * 10 + topicWordCount * 25 + topicScore;
}

function generateBest(dictionary) {
  let best = null;
  const failures = [];

  for (let run = 0; run < RUNS; run++) {
    const seed = SEED + run;
    resetRandom(seed);
    const result = generate(dictionary);

    try {
      validateGrid(result.grid, result.placedWords, dictionary.dictionarySet);
      const score = scoreBoard(result, dictionary);
      if (!best || score > best.score) {
        best = { ...result, score, seed };
      }
    } catch (error) {
      failures.push({ seed, error: error.message });
    }
  }

  if (!best) {
    throw new Error(`No valid generated board. Failures: ${failures.slice(0, 3).map((f) => `${f.seed}: ${f.error}`).join("; ")}`);
  }

  return best;
}

const prisma = new PrismaClient();
try {
  const dictionary = await loadDictionary(prisma);
  console.log(`Dictionary words loaded: ${dictionary.words.length}`);
  console.log(`Topic-matched words loaded: ${dictionary.topicWords.size}`);

  const result = generateBest(dictionary);
  validateGrid(result.grid, result.placedWords, dictionary.dictionarySet);

  console.log("");
  console.log(renderGrid(result.grid));
  console.log("");
  for (const word of numberWords(result.placedWords)) {
    const topicMarker = dictionary.topicWords.has(word.word) ? " topic" : "";
    console.log(`${String(word.number).padStart(2)} ${word.direction.padEnd(6)} (${word.x},${word.y}) ${word.word}${topicMarker}`);
  }
  console.log("");
  console.log(`Validation: ok`);
  console.log(`Best seed: ${result.seed}`);
  console.log(`Runs: ${RUNS}`);
  console.log(`Placed words: ${result.placedWords.length}`);
} finally {
  await prisma.$disconnect();
}
