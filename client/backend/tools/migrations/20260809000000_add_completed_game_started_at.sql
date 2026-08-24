-- Solve-time support (B10): when the play session began. Copied from the
-- ActiveGame row's "createdAt" at completion time, since completing a game
-- deletes the ActiveGame. Nullable — historical rows predate this column and
-- stay NULL (no backfill).
ALTER TABLE "CompletedGame" ADD COLUMN "startedAt" TIMESTAMP(3);
