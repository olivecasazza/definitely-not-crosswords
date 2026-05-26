-- CreateEnum
CREATE TYPE "DictionarySource" AS ENUM ('WORDNET');

-- CreateEnum
CREATE TYPE "DictionaryPartOfSpeech" AS ENUM ('NOUN', 'VERB', 'ADJECTIVE', 'ADVERB');

-- CreateTable
CREATE TABLE "DictionaryWord" (
    "id" TEXT NOT NULL,
    "word" TEXT NOT NULL,
    "length" INTEGER NOT NULL,
    "source" "DictionarySource" NOT NULL DEFAULT 'WORDNET',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "DictionaryWord_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "DictionaryDefinition" (
    "id" TEXT NOT NULL,
    "wordId" TEXT NOT NULL,
    "partOfSpeech" "DictionaryPartOfSpeech" NOT NULL,
    "synsetOffset" TEXT NOT NULL,
    "lemma" TEXT NOT NULL,
    "gloss" TEXT NOT NULL,
    "examples" JSONB,
    "source" "DictionarySource" NOT NULL DEFAULT 'WORDNET',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "DictionaryDefinition_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "DictionaryWord_word_key" ON "DictionaryWord"("word");

-- CreateIndex
CREATE INDEX "DictionaryWord_length_idx" ON "DictionaryWord"("length");

-- CreateIndex
CREATE INDEX "DictionaryWord_source_length_idx" ON "DictionaryWord"("source", "length");

-- CreateIndex
CREATE INDEX "DictionaryDefinition_partOfSpeech_idx" ON "DictionaryDefinition"("partOfSpeech");

-- CreateIndex
CREATE UNIQUE INDEX "DictionaryDefinition_wordId_partOfSpeech_synsetOffset_key" ON "DictionaryDefinition"("wordId", "partOfSpeech", "synsetOffset");

-- AddForeignKey
ALTER TABLE "DictionaryDefinition" ADD CONSTRAINT "DictionaryDefinition_wordId_fkey" FOREIGN KEY ("wordId") REFERENCES "DictionaryWord"("id") ON DELETE CASCADE ON UPDATE CASCADE;
