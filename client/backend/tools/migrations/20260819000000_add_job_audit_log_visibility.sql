-- Audit log + per-job visibility ACLs (DEF-72)

-- 1. Job visibility enum
CREATE TYPE "JobVisibility" AS ENUM ('TEAM', 'PUBLIC');

-- 2. Add visibility to CrosswordGenerationJob
ALTER TABLE "CrosswordGenerationJob" ADD COLUMN "visibility" "JobVisibility" NOT NULL DEFAULT 'TEAM';

-- 3. Audit log table
CREATE TABLE "JobAuditLog" (
    "id" TEXT NOT NULL,
    "jobId" TEXT NOT NULL,
    "actorId" TEXT NOT NULL,
    "eventType" TEXT NOT NULL, -- job_created | job_started | job_completed | job_failed | job_cancelled | acl_changed
    "payload" JSONB NOT NULL DEFAULT '{}',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "JobAuditLog_pkey" PRIMARY KEY ("id")
);

-- Index for looking up audit trail by job
CREATE INDEX "JobAuditLog_jobId_createdAt_idx" ON "JobAuditLog"("jobId", "createdAt");
