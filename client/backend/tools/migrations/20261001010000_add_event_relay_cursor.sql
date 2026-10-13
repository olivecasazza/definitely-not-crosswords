-- Replay half of the event outbox (DEF-275): the durable per-pod position that
-- lets a pod recover events published while its listener was disconnected.
-- `pg_notify` is not durable, so the rows those events left behind were simply
-- unread. See client/backend/server/src/pg_events.rs for the relay that reads
-- them.

ALTER TABLE "EventOutbox" ADD COLUMN "seq" BIGINT;

-- Existing rows are at most OUTBOX_RETENTION (600s) old and no cursor exists
-- yet, so nothing can replay them; numbering them in insertion order only
-- keeps the column dense for anything that inspects it later.
WITH numbered AS (
    SELECT "id", row_number() OVER (ORDER BY "createdAt", "id") AS seq
    FROM "EventOutbox"
)
UPDATE "EventOutbox" AS e
SET "seq" = numbered.seq
FROM numbered
WHERE numbered."id" = e."id";

ALTER TABLE "EventOutbox" ALTER COLUMN "seq" SET NOT NULL;

-- A BIGSERIAL would not do. `nextval` runs before commit, so a writer whose
-- transaction commits late can leave a hole behind a higher number, and a hole
-- lets a reader's cursor advance past a row that was never delivered to it.
-- One counter row, bumped inside the writer's own transaction, serialises
-- writers instead: `seq` order is then commit order with no gaps.
CREATE TABLE "EventOutboxSeq" (
    "id" INTEGER NOT NULL,
    "seq" BIGINT NOT NULL,
    CONSTRAINT "EventOutboxSeq_pkey" PRIMARY KEY ("id")
);

-- Seeded at the current head, not at 0: a relay that first connects must start
-- where the last row is, not replay everything still in retention.
INSERT INTO "EventOutboxSeq" ("id", "seq")
VALUES (1, (SELECT coalesce(max("seq"), 0) FROM "EventOutbox"));

-- Every replay and every prune walks this.
CREATE INDEX "EventOutbox_seq_idx" ON "EventOutbox"("seq");

-- Where each pod's relay got to. Keyed by the pod (EVENT_RELAY_ID, defaulting
-- to HOSTNAME) rather than by process: a restarted pod has the same key, so it
-- resumes instead of replaying history its subscribers already received.
CREATE TABLE "EventRelayCursor" (
    "relayId" TEXT NOT NULL,
    "seq" BIGINT NOT NULL,
    "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "EventRelayCursor_pkey" PRIMARY KEY ("relayId")
);
