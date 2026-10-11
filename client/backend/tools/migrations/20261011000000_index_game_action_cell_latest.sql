-- The write path asks "for each cell, the newest action" in two places that run
-- on every `addActions`: the pre-write snapshot (`load_cell_states`, per game)
-- and the leaderboard completion (across games). Both are
--   SELECT DISTINCT ON (cordX, cordY) ... ORDER BY cordX, cordY, submittedAt DESC, id DESC
-- over `GameAction`. Without a matching index Postgres sorts the whole action
-- log of the game on every write. Under four concurrent writers that sort
-- measured ~5.7s and held the per-game advisory lock the entire time, which is
-- what pushed concurrent `addActions` calls past their budget and breached the
-- k6 `trpc_error_rate` threshold (DEF-372 follow-up).
CREATE INDEX "GameAction_cell_latest_idx"
  ON "GameAction" ("activeGameId", "cordX", "cordY", "submittedAt" DESC, "id" DESC);
