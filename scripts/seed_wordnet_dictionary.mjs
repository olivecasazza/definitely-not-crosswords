import fs from "node:fs";
import path from "node:path";

const WORDNET_DICT_DIR = "data/crossword/wordnet/dict";
const MIN_WORD_LENGTH = 2;
const MAX_WORD_LENGTH = 50;
const CHUNK_SIZE = 1000;
const isDryRun = process.argv.includes("--dry-run");

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

  const { PrismaClient } = await import("@prisma/client");
  const prisma = new PrismaClient();

  try {
    for (const rows of chunk(wordRows, CHUNK_SIZE)) {
      await prisma.dictionaryWord.createMany({
        data: rows,
        skipDuplicates: true,
      });
    }

    const wordIdByWord = new Map();
    for (const words of chunk([...wordRowsByWord.keys()], CHUNK_SIZE)) {
      const existingWords = await prisma.dictionaryWord.findMany({
        where: { word: { in: words } },
        select: { id: true, word: true },
      });
      for (const word of existingWords) {
        wordIdByWord.set(word.word, word.id);
      }
    }

    const definitionRows = parsedRows
      .map((row) => {
        const wordId = wordIdByWord.get(row.word);
        if (!wordId) return null;
        return {
          wordId,
          partOfSpeech: row.partOfSpeech,
          synsetOffset: row.synsetOffset,
          lemma: row.lemma,
          gloss: row.gloss,
          examples: row.examples ?? undefined,
          source: "WORDNET",
        };
      })
      .filter(Boolean);

    for (const rows of chunk(definitionRows, CHUNK_SIZE)) {
      await prisma.dictionaryDefinition.createMany({
        data: rows,
        skipDuplicates: true,
      });
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
