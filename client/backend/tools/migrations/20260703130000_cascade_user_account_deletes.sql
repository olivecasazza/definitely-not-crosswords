-- Make account self-deletion work.
--
-- `user.deleteAccount` deletes a "User" row, but three foreign keys referenced
-- "User" with ON DELETE RESTRICT, so Postgres refused to delete any user who
-- had ever made a guess (GameAction), owned a sprite (UserSprite), or queued a
-- generation job (CrosswordGenerationJob) — deletion errored out for every real
-- user. Change those three FKs to ON DELETE CASCADE so deleting a User removes
-- its dependent rows, matching account-deletion intent.

-- GameAction.userId
ALTER TABLE "GameAction" DROP CONSTRAINT IF EXISTS "GameAction_userId_fkey";
ALTER TABLE "GameAction" ADD CONSTRAINT "GameAction_userId_fkey"
    FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- UserSprite.userId
ALTER TABLE "UserSprite" DROP CONSTRAINT IF EXISTS "UserSprite_userId_fkey";
ALTER TABLE "UserSprite" ADD CONSTRAINT "UserSprite_userId_fkey"
    FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- CrosswordGenerationJob.createdById
ALTER TABLE "CrosswordGenerationJob" DROP CONSTRAINT IF EXISTS "CrosswordGenerationJob_createdById_fkey";
ALTER TABLE "CrosswordGenerationJob" ADD CONSTRAINT "CrosswordGenerationJob_createdById_fkey"
    FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
