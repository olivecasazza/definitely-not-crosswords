-- Daily puzzle (B14): one featured Game per UTC calendar day. "date" is the
-- UTC day as YYYY-MM-DD text (lexicographic order == chronological order).
-- Rows are written lazily by game.getDaily on the first request of the day
-- via INSERT ... ON CONFLICT DO NOTHING, so concurrent first requests are safe.

-- CreateTable
CREATE TABLE "DailyPick" (
    "date" TEXT NOT NULL,
    "gameId" TEXT NOT NULL,

    CONSTRAINT "DailyPick_pkey" PRIMARY KEY ("date")
);

-- AddForeignKey
ALTER TABLE "DailyPick" ADD CONSTRAINT "DailyPick_gameId_fkey" FOREIGN KEY ("gameId") REFERENCES "Game"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
