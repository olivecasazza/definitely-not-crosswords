-- Persist generator run metadata and progress events so completed jobs can be
-- inspected after the live WebSocket session has ended.
ALTER TABLE "CrosswordGenerationJob"
  ADD COLUMN "title" TEXT,
  ADD COLUMN "metadata" JSONB,
  ADD COLUMN "eventLog" JSONB,
  ADD COLUMN "startedAt" TIMESTAMP(3),
  ADD COLUMN "completedAt" TIMESTAMP(3),
  ADD COLUMN "durationMs" INTEGER;
