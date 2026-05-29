import { randomUUID } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const WORDNET_DICT_DIR = "data/crossword/wordnet/dict";
const MIN_WORD_LENGTH = 2;
const MAX_WORD_LENGTH = 50;
const WORD_CHUNK_SIZE = 1000;
const DEFINITION_CHUNK_SIZE = 5000;
const isDryRun = process.argv.includes("--dry-run");
const isSkipIfPresent = process.argv.includes("--skip-if-present");

const dataFiles = [
  { file: "data.noun", partOfSpeech: "NOUN" },
  { file: "data.verb", partOfSpeech: "VERB" },
  { file: "data.adj", partOfSpeech: "ADJECTIVE" },
  { file: "data.adv", partOfSpeech: "ADVERB" },
];

function chunk(values, size) {
  const chunks = [];
  for (let index = 0; index < values.length; index += size) {
    chunks.push(values.slice(index, index + size));
  }
  return chunks;
}

function logProgress(label, completed, total) {
  console.log(`${label}: ${Math.min(completed, total)}/${total}`);
}

function normalizeLemma(lemma) {
  if (lemma.includes("_") || lemma.includes("-") || lemma.includes("'")) return null;
  const word = lemma.toUpperCase();
  if (!/^[A-Z]+$/.test(word)) return null;
  if (word.length < MIN_WORD_LENGTH || word.length > MAX_WORD_LENGTH) return null;
  return word;
}

function parseGloss(rawGloss) {
  const parts = rawGloss
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean);
  const definition = parts[0] ?? rawGloss.trim();
  const examples = parts
    .slice(1)
    .map((part) => part.match(/^"(.+)"$/)?.[1])
    .filter(Boolean);

  return {
    gloss: definition,
    examples: examples.length ? examples : undefined,
  };
}

function parseWordNetDataFile(filePath, partOfSpeech) {
  const rows = [];
  const contents = fs.readFileSync(filePath, "utf8");

  for (const line of contents.split(/\r?\n/)) {
    if (!line || line.startsWith("  ")) continue;

    const [rawSynset, rawGloss = ""] = line.split(" | ");
    const fields = rawSynset.trim().split(/\s+/);
    if (fields.length < 5) continue;

    const synsetOffset = fields[0];
    const wordCount = Number.parseInt(fields[3], 16);
    if (!Number.isFinite(wordCount) || wordCount < 1) continue;

    const { gloss, examples } = parseGloss(rawGloss);
    const firstWordIndex = 4;

    for (let wordIndex = 0; wordIndex < wordCount; wordIndex++) {
      const lemma = fields[firstWordIndex + wordIndex * 2];
      if (!lemma) continue;

      const word = normalizeLemma(lemma);
      if (!word) continue;

      rows.push({
        word,
        lemma,
        length: word.length,
        partOfSpeech,
        synsetOffset,
        gloss,
        examples,
      });
    }
  }

  return rows;
}

async function main() {
  const parsedRows = dataFiles.flatMap(({ file, partOfSpeech }) => {
    const filePath = path.join(WORDNET_DICT_DIR, file);
    return parseWordNetDataFile(filePath, partOfSpeech);
  });

  const wordRowsByWord = new Map();
  for (const row of parsedRows) {
    if (!wordRowsByWord.has(row.word)) {
      wordRowsByWord.set(row.word, {
        word: row.word,
        length: row.length,
        source: "WORDNET",
      });
    }
  }

  const wordRows = [...wordRowsByWord.values()];

  if (isDryRun) {
    console.log(`Parsed ${wordRows.length} WordNet words.`);
    console.log(`Parsed ${parsedRows.length} WordNet definitions.`);
    console.log("Dry run complete; database was not modified.");
    return;
  }

  let prisma;
  if (!prisma) {
    const { PrismaClient } = await import("@prisma/client");
    prisma = new PrismaClient();
  }

  try {
    if (isSkipIfPresent) {
      const [wordCount, definitionCount] = await Promise.all([
        prisma.dictionaryWord.count({ where: { source: "WORDNET" } }),
        prisma.dictionaryDefinition.count({ where: { source: "WORDNET" } }),
      ]);

      if (wordCount >= wordRows.length && definitionCount >= parsedRows.length) {
        console.log(`Dictionary already seeded (${wordCount} words, ${definitionCount} definitions).`);
        return;
      }
    }

    const existingWordCount = await prisma.dictionaryWord.count({
      where: { source: "WORDNET" },
    });

    if (existingWordCount < wordRows.length) {
      let completed = 0;
      for (const rows of chunk(wordRows, WORD_CHUNK_SIZE)) {
        await prisma.dictionaryWord.createMany({
          data: rows,
          skipDuplicates: true,
        });
        completed += rows.length;
        logProgress("WordNet words", completed, wordRows.length);
      }
    } else {
      console.log(`WordNet words already present (${existingWordCount}/${wordRows.length}).`);
    }

    const wordIdByWord = new Map();
    const existingWords = await prisma.dictionaryWord.findMany({
      where: { source: "WORDNET" },
      select: { id: true, word: true },
    });

    for (const word of existingWords) {
      wordIdByWord.set(word.word, word.id);
    }

    const definitionRows = parsedRows
      .map((row) => {
        const wordId = wordIdByWord.get(row.word);
        if (!wordId) return null;
        return {
          wordId,
          id: randomUUID(),
          partOfSpeech: row.partOfSpeech,
          synsetOffset: row.synsetOffset,
          lemma: row.lemma,
          gloss: row.gloss,
          examples: row.examples ?? null,
          source: "WORDNET",
        };
      })
      .filter(Boolean);

    const existingDefinitionCount = await prisma.dictionaryDefinition.count({
      where: { source: "WORDNET" },
    });

    if (existingDefinitionCount < definitionRows.length) {
      let completed = 0;
      for (const rows of chunk(definitionRows, DEFINITION_CHUNK_SIZE)) {
        await prisma.$executeRawUnsafe(
          `
            INSERT INTO "DictionaryDefinition"
              ("id", "wordId", "partOfSpeech", "synsetOffset", "lemma", "gloss", "examples", "source")
            SELECT
              definition_rows."id",
              definition_rows."wordId",
              definition_rows."partOfSpeech"::"DictionaryPartOfSpeech",
              definition_rows."synsetOffset",
              definition_rows."lemma",
              definition_rows."gloss",
              definition_rows."examples",
              'WORDNET'::"DictionarySource"
            FROM jsonb_to_recordset($1::jsonb) AS definition_rows(
              "id" text,
              "wordId" text,
              "partOfSpeech" text,
              "synsetOffset" text,
              "lemma" text,
              "gloss" text,
              "examples" jsonb
            )
            ON CONFLICT ("wordId", "partOfSpeech", "synsetOffset") DO NOTHING
          `,
          JSON.stringify(rows)
        );
        completed += rows.length;
        logProgress("WordNet definitions", completed, definitionRows.length);
      }
    } else {
      console.log(`WordNet definitions already present (${existingDefinitionCount}/${definitionRows.length}).`);
    }

    console.log(`Seeded ${wordRows.length} WordNet words.`);
    console.log(`Seeded ${definitionRows.length} WordNet definitions.`);
  } finally {
    await prisma.$disconnect();
  }
}

main()
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  })
