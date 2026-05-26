-- CreateEnum
CREATE TYPE "UserRole" AS ENUM ('USER', 'ADMIN');

-- CreateEnum
CREATE TYPE "GameSource" AS ENUM ('MANUAL', 'SCRAPED', 'GENERATED');

-- CreateEnum
CREATE TYPE "GenerationStatus" AS ENUM ('QUEUED', 'RUNNING', 'SUCCEEDED', 'FAILED');

-- AlterTable
ALTER TABLE "User" ADD COLUMN "role" "UserRole" NOT NULL DEFAULT 'USER';

-- AlterTable
ALTER TABLE "Game" ADD COLUMN "source" "GameSource" NOT NULL DEFAULT 'MANUAL';

-- CreateTable
CREATE TABLE "CrosswordGenerationJob" (
    "id" TEXT NOT NULL,
    "status" "GenerationStatus" NOT NULL DEFAULT 'QUEUED',
    "topic" TEXT NOT NULL,
    "width" INTEGER NOT NULL,
    "height" INTEGER NOT NULL,
    "minWordLength" INTEGER NOT NULL,
    "maxWordLength" INTEGER NOT NULL,
    "params" JSONB NOT NULL,
    "metrics" JSONB,
    "error" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "createdById" TEXT NOT NULL,
    "resultGameId" TEXT,

    CONSTRAINT "CrosswordGenerationJob_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "CrosswordGenerationJob_resultGameId_key" ON "CrosswordGenerationJob"("resultGameId");

-- CreateIndex
CREATE INDEX "CrosswordGenerationJob_status_createdAt_idx" ON "CrosswordGenerationJob"("status", "createdAt");

-- CreateIndex
CREATE INDEX "CrosswordGenerationJob_createdById_createdAt_idx" ON "CrosswordGenerationJob"("createdById", "createdAt");

-- AddForeignKey
ALTER TABLE "CrosswordGenerationJob" ADD CONSTRAINT "CrosswordGenerationJob_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "CrosswordGenerationJob" ADD CONSTRAINT "CrosswordGenerationJob_resultGameId_fkey" FOREIGN KEY ("resultGameId") REFERENCES "Game"("id") ON DELETE SET NULL ON UPDATE CASCADE;
