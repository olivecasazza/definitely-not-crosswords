import fs from "node:fs";
import path from "node:path";
import { PrismaClient } from "@prisma/client";

type Direction = "ACROSS" | "DOWN";

type PlacedWord = {
  word: string;
  direction: Direction;
  x: number;
  y: number;
};

type DictionaryRow = {
  word: string;
  length: number;
  definitions: Array<{
    partOfSpeech: string;
    gloss: string;
  }>;
  qualityScore: number;
  frequencyScore: number;
};

type Dictionary = {
  dictionarySet: Set<string>;
  topicWords: Set<string>;
  topicScores: Map<string, number>;
  qualityScores: Map<string, number>;
  frequencyScores: Map<string, number>;
  clueByWord: Map<string, string>;
  byLetter: Map<string, string[]>;
  byLength: Map<number, string[]>;
};

export type CrosswordGenerationParams = {
  topic: string;
  width: number;
  height: number;
  minWordLength: number;
  maxWordLength: number;
  targetWords: number;
  runs: number;
  maxAttempts: number;
};

export type GeneratedCrossword = {
  title: string;
  questions: Array<{
    number: number;
    answer: string;
    questionText: string;
    rootX: number;
    rootY: number;
    direction: Direction;
  }>;
  metrics: Record<string, unknown>;
  grid: Array<Array<string | null>>;
};

/**
 * Progress events emitted by the generation pipeline. The tRPC `runGeneration`
 * subscription forwards these (plus job lifecycle events) to the admin UI so a
 * long generation shows granular progress instead of an opaque spinner.
 */
export type GenerationProgressEvent =
  | { type: "stage"; stage: string; message: string }
  | { type: "progress"; stage: string; current: number; total: number; message?: string }
  | { type: "log"; level: "info" | "warn" | "error"; message: string };

export type GenerationProgressCallback = (event: GenerationProgressEvent) => void;

const EMBEDDING_MODEL_PATH = "data/crossword/models/all-MiniLM-L6-v2";
const WORDNET_COUNT_PATH = "data/crossword/wordnet/dict/cntlist";
const EMBEDDING_CANDIDATE_LIMIT = 4000;
const EMBEDDING_BATCH_SIZE = 32;

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

export async function generateCrosswordFromDictionary(
  prisma: PrismaClient,
  params: CrosswordGenerationParams,
  onEvent?: GenerationProgressCallback
): Promise<GeneratedCrossword> {
  onEvent?.({
    type: "stage",
    stage: "loading-dictionary",
    message: "Loading dictionary and scoring candidates",
  });
  const dictionary = await loadDictionary(prisma, params, onEvent);

  onEvent?.({
    type: "stage",
    stage: "solving",
    message: `Generating crossword grids (${params.runs} runs)`,
  });
  const best = await generateBest(dictionary, params, onEvent);

  onEvent?.({ type: "stage", stage: "validating", message: "Validating winning grid" });
  validateGrid(best.grid, best.placedWords, dictionary.dictionarySet, params);

  onEvent?.({
    type: "log",
    level: "info",
    message: `Best grid: ${best.placedWords.length} words placed (score ${best.score}, seed ${best.seed})`,
  });

  return {
    title: generatedTitle(params.topic),
    questions: numberWords(best.placedWords).map((word) => ({
      number: word.number,
      answer: word.word,
      questionText: dictionary.clueByWord.get(word.word) ?? `Related to ${params.topic}`,
      rootX: word.x,
      rootY: word.y,
      direction: word.direction,
    })),
    metrics: {
      topic: params.topic,
      width: params.width,
      height: params.height,
      targetWords: params.targetWords,
      placedWords: best.placedWords.length,
      seed: best.seed,
      runs: params.runs,
      score: best.score,
    },
    grid: best.grid,
  };
}

function generatedTitle(topic: string) {
  const trimmed = topic.trim();
  return trimmed.length ? `Generated: ${trimmed}` : "Generated Crossword";
}

async function loadDictionary(
  prisma: PrismaClient,
  params: CrosswordGenerationParams,
  onEvent?: GenerationProgressCallback
): Promise<Dictionary> {
  const frequencyScores = loadWordNetFrequencyScores(
    path.join(process.cwd(), WORDNET_COUNT_PATH)
  );
  const rows = await prisma.dictionaryWord.findMany({
    where: {
      length: { gte: params.minWordLength, lte: params.maxWordLength },
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

  const scoredWords: DictionaryRow[] = rows
    .map((row) => {
      const frequencyScore = frequencyScores.get(row.word) ?? 0;
      return {
        ...row,
        frequencyScore,
        qualityScore: scoreWordQuality(row, frequencyScore),
      };
    })
    .filter((row) => row.qualityScore >= 3);

  if (!scoredWords.length) {
    throw new Error("Dictionary is empty. Seed DictionaryWord first.");
  }

  onEvent?.({
    type: "log",
    level: "info",
    message: `${rows.length} dictionary rows in range; ${scoredWords.length} passed the quality filter`,
  });

  const topicScores = await scoreCandidatesByEmbedding(params.topic, scoredWords, onEvent);
  const topicWords = new Set(
    [...topicScores.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 800)
      .map(([word]) => word)
  );

  const byLetter = new Map<string, string[]>();
  const byLength = new Map<number, string[]>();
  const qualityScores = new Map(scoredWords.map((row) => [row.word, row.qualityScore]));
  const frequencyByWord = new Map(scoredWords.map((row) => [row.word, row.frequencyScore]));
  const clueByWord = new Map(
    scoredWords.map((row) => [row.word, cleanClue(row.definitions[0]?.gloss ?? row.word)])
  );

  for (const row of scoredWords) {
    if (!byLength.has(row.length)) byLength.set(row.length, []);
    byLength.get(row.length)?.push(row.word);

    for (const letter of new Set(row.word)) {
      if (!byLetter.has(letter)) byLetter.set(letter, []);
      byLetter.get(letter)?.push(row.word);
    }
  }

  for (const [letter, letterWords] of byLetter) {
    letterWords.sort(
      (a, b) =>
        (topicScores.get(b) ?? 0) - (topicScores.get(a) ?? 0) ||
        (frequencyByWord.get(b) ?? 0) - (frequencyByWord.get(a) ?? 0) ||
        (qualityScores.get(b) ?? 0) - (qualityScores.get(a) ?? 0) ||
        a.length - b.length
    );
    byLetter.set(letter, letterWords.slice(0, 2500));
  }

  return {
    dictionarySet: new Set(scoredWords.map((row) => row.word)),
    topicWords,
    topicScores,
    qualityScores,
    frequencyScores: frequencyByWord,
    clueByWord,
    byLetter,
    byLength,
  };
}

function cleanClue(gloss: string) {
  return gloss.replace(/^["']|["']$/g, "").replace(/\s+/g, " ").trim();
}

function loadWordNetFrequencyScores(filePath: string) {
  const scores = new Map<string, number>();
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

function scoreWordQuality(
  row: { word: string; definitions: Array<{ partOfSpeech: string; gloss: string }> },
  frequencyScore: number
) {
  const word = row.word;
  const glossText = row.definitions.map((definition) => definition.gloss).join(" ");
  const partsOfSpeech = new Set(row.definitions.map((definition) => definition.partOfSpeech));
  let score = 0;

  if (!/[AEIOUY]/.test(word)) return -10;
  if (word.length <= 5 && frequencyScore < 5) return -10;
  if (/(.)\1\1/.test(word)) score -= 2;
  if (/[QXZ]/.test(word)) score -= 1;
  if (word.length >= 4 && word.length <= 8) score += 2;
  if (word.length === 3) score -= 1;
  if (word.length >= 9) score -= 1;
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

async function scoreCandidatesByEmbedding(
  topic: string,
  candidates: DictionaryRow[],
  onEvent?: GenerationProgressCallback
) {
  onEvent?.({ type: "stage", stage: "embedding-model", message: "Loading embedding model" });
  const { env, pipeline } = await import("@xenova/transformers");
  env.allowRemoteModels = false;
  env.allowLocalModels = true;
  env.localModelPath = ".";

  const extractor = await pipeline("feature-extraction", EMBEDDING_MODEL_PATH, {
    quantized: true,
  });
  const topicEmbedding = await embedText(extractor, topic);
  const candidateRows = [...candidates]
    .sort((a, b) => b.qualityScore - a.qualityScore)
    .slice(0, EMBEDDING_CANDIDATE_LIMIT);

  const batches = chunk(candidateRows, EMBEDDING_BATCH_SIZE);
  onEvent?.({
    type: "stage",
    stage: "embedding",
    message: `Scoring ${candidateRows.length} candidates for topic relevance`,
  });

  const scores = new Map<string, number>();
  for (let batchIndex = 0; batchIndex < batches.length; batchIndex++) {
    const rows = batches[batchIndex];
    const embeddings = await embedTexts(extractor, rows.map(candidateEmbeddingText));
    for (let index = 0; index < rows.length; index++) {
      scores.set(rows[index].word, cosineSimilarity(topicEmbedding, embeddings[index]));
    }
    onEvent?.({
      type: "progress",
      stage: "embedding",
      current: batchIndex + 1,
      total: batches.length,
      message: "Embedding candidate words",
    });
  }

  return scores;
}

function candidateEmbeddingText(row: DictionaryRow) {
  const glosses = row.definitions
    .slice(0, 4)
    .map((definition) => definition.gloss)
    .join("; ");
  return `${row.word.toLowerCase()}: ${glosses}`;
}

async function embedText(extractor: any, text: string) {
  const output = await extractor(text, { pooling: "mean", normalize: true });
  return Array.from(output.data) as number[];
}

async function embedTexts(extractor: any, texts: string[]) {
  const output = await extractor(texts, { pooling: "mean", normalize: true });
  const [count, dimensions] = output.dims;
  const embeddings: number[][] = [];
  for (let row = 0; row < count; row++) {
    embeddings.push(Array.from(output.data.slice(row * dimensions, (row + 1) * dimensions)));
  }
  return embeddings;
}

function cosineSimilarity(a: number[], b: number[]) {
  let sum = 0;
  for (let index = 0; index < a.length; index++) sum += a[index] * b[index];
  return sum;
}

function createRng(seed: number) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 2 ** 32;
  };
}

async function generateBest(
  dictionary: Dictionary,
  params: CrosswordGenerationParams,
  onEvent?: GenerationProgressCallback
) {
  let best:
    | {
        grid: Array<Array<string | null>>;
        placedWords: PlacedWord[];
        score: number;
        seed: number;
      }
    | null = null;

  for (let run = 0; run < params.runs; run++) {
    const seed = run + 1;
    const result = await generate(dictionary, params, seed, onEvent, run + 1);

    try {
      validateGrid(result.grid, result.placedWords, dictionary.dictionarySet, params);
      const score = scoreBoard(result, dictionary);
      if (!best || score > best.score) {
        best = { ...result, score, seed };
      }
    } catch {
      // Skip invalid attempts; the next seeded run may still produce a valid board.
    }

    onEvent?.({
      type: "progress",
      stage: "solving",
      current: run + 1,
      total: params.runs,
      message: `Best so far: ${best?.placedWords.length ?? 0} words`,
    });
    // Yield so progress events flush over the WebSocket between runs (the
    // per-run solver is synchronous CPU and otherwise blocks the event loop).
    await new Promise((resolve) => setImmediate(resolve));
  }

  if (!best) throw new Error("No valid crossword was generated.");
  return best;
}

async function generate(
  dictionary: Dictionary,
  params: CrosswordGenerationParams,
  seed: number,
  onEvent?: GenerationProgressCallback,
  runNumber?: number
) {
  const random = createRng(seed);
  const grid = emptyGrid(params);
  const placedWords: PlacedWord[] = [];
  const seedWords = seedCandidates(dictionary, params);

  for (const seedWord of seedWords) {
    const candidate = {
      word: seedWord,
      direction: "ACROSS" as Direction,
      x: Math.floor((params.width - seedWord.length) / 2),
      y: Math.floor(params.height / 2),
    };
    if (canPlace(grid, placedWords, candidate, params)) {
      placeWord(grid, placedWords, candidate);
      break;
    }
  }

  for (
    let attempt = 0;
    attempt < params.maxAttempts && placedWords.length < params.targetWords;
    attempt++
  ) {
    const candidates = findBestPlacements(grid, placedWords, dictionary, params, random);
    if (!candidates.length) break;
    const chosen = sample(candidates.slice(0, Math.min(12, candidates.length)), random);
    placeWord(grid, placedWords, chosen);

    // Periodically surface within-run progress and yield the event loop so the
    // UI keeps moving during a long solving run (not just between runs).
    if (onEvent && attempt % 20 === 0) {
      onEvent({
        type: "progress",
        stage: "solving-attempts",
        current: attempt,
        total: params.maxAttempts,
        message: `Run ${runNumber ?? "?"}/${params.runs}: placed ${placedWords.length}/${params.targetWords}`,
      });
      await new Promise((resolve) => setImmediate(resolve));
    }
  }

  return { grid, placedWords };
}

function seedCandidates(dictionary: Dictionary, params: CrosswordGenerationParams) {
  const min = Math.max(params.minWordLength, 5);
  const max = Math.min(params.maxWordLength, 12);
  const words: string[] = [];
  for (let length = min; length <= max; length++) {
    words.push(...(dictionary.byLength.get(length) ?? []).filter((word) => dictionary.topicWords.has(word)));
  }
  for (let length = min; length <= max; length++) {
    words.push(...(dictionary.byLength.get(length) ?? []));
  }
  return words.sort((a, b) => (dictionary.topicScores.get(b) ?? 0) - (dictionary.topicScores.get(a) ?? 0));
}

function findBestPlacements(
  grid: Array<Array<string | null>>,
  placedWords: PlacedWord[],
  dictionary: Dictionary,
  params: CrosswordGenerationParams,
  random: () => number
) {
  const usedWords = new Set(placedWords.map((placed) => placed.word));
  const placements: Array<PlacedWord & { placementScore: number }> = [];

  for (const anchor of shuffle(placedWords, random)) {
    for (const anchorIndex of shuffle(
      Array.from({ length: anchor.word.length }, (_, index) => index),
      random
    )) {
      const anchorCell = cellAt(anchor, anchorIndex);
      const letter = anchor.word[anchorIndex];
      const direction = anchor.direction === "ACROSS" ? "DOWN" : "ACROSS";

      for (const word of (dictionary.byLetter.get(letter) ?? []).slice(0, 650)) {
        if (usedWords.has(word)) continue;

        const matchingIndexes = [...word]
          .map((candidateLetter, index) => (candidateLetter === letter ? index : -1))
          .filter((index) => index >= 0);

        for (const wordIndex of shuffle(matchingIndexes, random)) {
          const candidate = {
            word,
            direction,
            x: direction === "ACROSS" ? anchorCell.x - wordIndex : anchorCell.x,
            y: direction === "DOWN" ? anchorCell.y - wordIndex : anchorCell.y,
          };

          if (!canPlace(grid, placedWords, candidate, params)) continue;

          const topicScore = dictionary.topicScores.get(word) ?? 0;
          const qualityScore = dictionary.qualityScores.get(word) ?? 0;
          const frequencyScore = dictionary.frequencyScores.get(word) ?? 0;
          const lengthScore = word.length >= 4 && word.length <= 7 ? 8 : 0;
          placements.push({
            ...candidate,
            placementScore:
              crossingCount(grid, candidate, params) * 50 +
              topicScore * 100 +
              qualityScore * 5 +
              Math.log1p(frequencyScore) * 8 +
              lengthScore,
          });
        }
      }
    }

    if (placements.length > 1000) break;
  }

  return placements.sort((a, b) => b.placementScore - a.placementScore);
}

function emptyGrid(params: CrosswordGenerationParams) {
  return Array.from({ length: params.height }, () => Array(params.width).fill(null));
}

function cellAt(word: PlacedWord, index: number) {
  return word.direction === "ACROSS"
    ? { x: word.x + index, y: word.y }
    : { x: word.x, y: word.y + index };
}

function inBounds(x: number, y: number, params: CrosswordGenerationParams) {
  return x >= 0 && x < params.width && y >= 0 && y < params.height;
}

function letterAt(grid: Array<Array<string | null>>, x: number, y: number) {
  if (y < 0 || y >= grid.length || x < 0 || x >= grid[0].length) return null;
  return grid[y][x];
}

function occupiedDirectionsAt(placedWords: PlacedWord[], x: number, y: number) {
  const directions = new Set<Direction>();
  for (const placed of placedWords) {
    for (let i = 0; i < placed.word.length; i++) {
      const cell = cellAt(placed, i);
      if (cell.x === x && cell.y === y) directions.add(placed.direction);
    }
  }
  return directions;
}

function canPlace(
  grid: Array<Array<string | null>>,
  placedWords: PlacedWord[],
  candidate: PlacedWord,
  params: CrosswordGenerationParams
) {
  const dx = candidate.direction === "ACROSS" ? 1 : 0;
  const dy = candidate.direction === "DOWN" ? 1 : 0;
  const before = { x: candidate.x - dx, y: candidate.y - dy };
  const after = {
    x: candidate.x + dx * candidate.word.length,
    y: candidate.y + dy * candidate.word.length,
  };

  if (letterAt(grid, before.x, before.y) || letterAt(grid, after.x, after.y)) return false;

  let crossings = 0;
  for (let i = 0; i < candidate.word.length; i++) {
    const x = candidate.x + dx * i;
    const y = candidate.y + dy * i;
    if (!inBounds(x, y, params)) return false;

    const existing = grid[y][x];
    if (existing && existing !== candidate.word[i]) return false;
    if (existing === candidate.word[i]) {
      if (occupiedDirectionsAt(placedWords, x, y).has(candidate.direction)) return false;
      crossings++;
      continue;
    }

    if (candidate.direction === "ACROSS") {
      if (letterAt(grid, x, y - 1) || letterAt(grid, x, y + 1)) return false;
    } else if (letterAt(grid, x - 1, y) || letterAt(grid, x + 1, y)) {
      return false;
    }
  }

  return placedWords.length === 0 || crossings > 0;
}

function crossingCount(
  grid: Array<Array<string | null>>,
  candidate: PlacedWord,
  params: CrosswordGenerationParams
) {
  const dx = candidate.direction === "ACROSS" ? 1 : 0;
  const dy = candidate.direction === "DOWN" ? 1 : 0;
  let count = 0;
  for (let i = 0; i < candidate.word.length; i++) {
    const x = candidate.x + dx * i;
    const y = candidate.y + dy * i;
    if (inBounds(x, y, params) && letterAt(grid, x, y) === candidate.word[i]) count++;
  }
  return count;
}

function placeWord(
  grid: Array<Array<string | null>>,
  placedWords: PlacedWord[],
  candidate: PlacedWord
) {
  for (let i = 0; i < candidate.word.length; i++) {
    const cell = cellAt(candidate, i);
    grid[cell.y][cell.x] = candidate.word[i];
  }
  placedWords.push(candidate);
}

function validateGrid(
  grid: Array<Array<string | null>>,
  placedWords: PlacedWord[],
  dictionarySet: Set<string>,
  params: CrosswordGenerationParams
) {
  const answers = new Set<string>();
  for (const placed of placedWords) {
    if (answers.has(placed.word)) throw new Error(`Duplicate answer: ${placed.word}`);
    answers.add(placed.word);
    if (!dictionarySet.has(placed.word)) throw new Error(`Answer not in dictionary: ${placed.word}`);
    assertMaximalSlot(grid, placed);
  }

  const placedKeys = new Set(placedWords.map(slotKey));
  for (const slot of extractSlots(grid, params)) {
    if (slot.word.length >= params.minWordLength && !placedKeys.has(slotKey(slot))) {
      throw new Error(`Unclued accidental slot found: ${slot.word}`);
    }
  }
}

function assertMaximalSlot(grid: Array<Array<string | null>>, placed: PlacedWord) {
  const dx = placed.direction === "ACROSS" ? 1 : 0;
  const dy = placed.direction === "DOWN" ? 1 : 0;
  if (
    letterAt(grid, placed.x - dx, placed.y - dy) ||
    letterAt(grid, placed.x + dx * placed.word.length, placed.y + dy * placed.word.length)
  ) {
    throw new Error(`Placed word is not a maximal slot: ${placed.word}`);
  }
}

function extractSlots(grid: Array<Array<string | null>>, params: CrosswordGenerationParams) {
  const slots: PlacedWord[] = [];
  for (let y = 0; y < params.height; y++) {
    let x = 0;
    while (x < params.width) {
      while (x < params.width && !grid[y][x]) x++;
      const start = x;
      let word = "";
      while (x < params.width && grid[y][x]) word += grid[y][x++];
      if (word.length >= params.minWordLength) {
        slots.push({ direction: "ACROSS", x: start, y, word });
      }
    }
  }

  for (let x = 0; x < params.width; x++) {
    let y = 0;
    while (y < params.height) {
      while (y < params.height && !grid[y][x]) y++;
      const start = y;
      let word = "";
      while (y < params.height && grid[y][x]) word += grid[y++][x];
      if (word.length >= params.minWordLength) {
        slots.push({ direction: "DOWN", x, y: start, word });
      }
    }
  }
  return slots;
}

function slotKey(slot: PlacedWord) {
  return `${slot.direction}:${slot.x}:${slot.y}:${slot.word}`;
}

function numberWords(placedWords: PlacedWord[]) {
  const starts = new Map<string, number | null>();
  for (const word of placedWords) starts.set(`${word.x}:${word.y}`, null);

  let number = 1;
  const maxY = Math.max(...placedWords.map((word) => word.y));
  const maxX = Math.max(...placedWords.map((word) => word.x));
  for (let y = 0; y <= maxY; y++) {
    for (let x = 0; x <= maxX; x++) {
      const key = `${x}:${y}`;
      if (starts.has(key)) starts.set(key, number++);
    }
  }

  return placedWords
    .map((word) => ({ ...word, number: starts.get(`${word.x}:${word.y}`) ?? 0 }))
    .sort((a, b) => a.number - b.number || a.direction.localeCompare(b.direction));
}

function scoreBoard(
  result: { grid: Array<Array<string | null>>; placedWords: PlacedWord[] },
  dictionary: Dictionary
) {
  const filledCells = result.grid.flat().filter(Boolean).length;
  const topicScore = result.placedWords.reduce(
    (sum, word) => sum + (dictionary.topicScores.get(word.word) ?? 0),
    0
  );
  const topicWordCount = result.placedWords.filter((word) => dictionary.topicWords.has(word.word)).length;
  return result.placedWords.length * 1000 + filledCells * 10 + topicWordCount * 25 + topicScore;
}

function sample<T>(values: T[], random: () => number) {
  return values[Math.floor(random() * values.length)];
}

function shuffle<T>(values: T[], random: () => number) {
  const copy = [...values];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

function chunk<T>(values: T[], size: number) {
  const chunks: T[][] = [];
  for (let index = 0; index < values.length; index += size) {
    chunks.push(values.slice(index, index + size));
  }
  return chunks;
}
