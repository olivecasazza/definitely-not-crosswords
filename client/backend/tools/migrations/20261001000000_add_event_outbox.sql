CREATE TABLE "EventOutbox" (
    "id" TEXT NOT NULL,
    "payload" JSONB NOT NULL,
    "origin" TEXT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "EventOutbox_pkey" PRIMARY KEY ("id")
);

CREATE INDEX "EventOutbox_createdAt_idx" ON "EventOutbox"("createdAt");
