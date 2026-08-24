-- Repair drift left behind by the one-time Prisma -> sqlx adoption.
--
-- The pre-sqlx production database had only 7 Prisma migrations applied (up to
-- 20260529082000_add_generation_job_metadata). By the time `migrate` first ran
-- against it (2026-06-30) the sqlx migrations directory already held 9, and the
-- old adoption path baselined *all* of them as applied without checking that
-- their objects existed. That silently swallowed two migrations that had never
-- run anywhere on prod:
--
--   20260529083000_add_user_vip_pass   -> "User"."vipPass"
--   20260529090000_add_discounts       -> "Discount" + its two enums
--
-- Because both are recorded as applied in _sqlx_migrations, sqlx will never
-- replay them. This forward migration re-applies exactly their DDL, guarded so
-- it is a clean no-op on any database that already has the objects (staging,
-- local dev, fresh installs) and repairs the ones that do not (production).
--
-- Column types, defaults, nullability, indexes and constraint names are copied
-- verbatim from the two original migrations so the repaired schema is
-- indistinguishable from a from-scratch one.

-- ── 20260529083000_add_user_vip_pass ─────────────────────────────────────────

-- Manual "VIP pass" override that grants Pro access without a paid subscription.
ALTER TABLE "User" ADD COLUMN IF NOT EXISTS "vipPass" BOOLEAN NOT NULL DEFAULT false;

-- ── 20260529090000_add_discounts ─────────────────────────────────────────────

-- CreateEnum (CREATE TYPE has no IF NOT EXISTS; guard on the catalog instead)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = 'public' AND t.typname = 'DiscountAmountType'
    ) THEN
        CREATE TYPE "DiscountAmountType" AS ENUM ('PERCENT', 'FIXED');
    END IF;
END
$$;

-- CreateEnum
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = 'public' AND t.typname = 'DiscountDuration'
    ) THEN
        CREATE TYPE "DiscountDuration" AS ENUM ('ONCE', 'FOREVER', 'REPEATING');
    END IF;
END
$$;

-- CreateTable
CREATE TABLE IF NOT EXISTS "Discount" (
    "id" TEXT NOT NULL,
    "code" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "lemonSqueezyId" TEXT,
    "amountType" "DiscountAmountType" NOT NULL DEFAULT 'PERCENT',
    "amount" INTEGER NOT NULL,
    "duration" "DiscountDuration" NOT NULL DEFAULT 'ONCE',
    "maxRedemptions" INTEGER,
    "timesRedeemed" INTEGER NOT NULL DEFAULT 0,
    "expiresAt" TIMESTAMP(3),
    "isActive" BOOLEAN NOT NULL DEFAULT true,
    "testMode" BOOLEAN NOT NULL DEFAULT false,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "Discount_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX IF NOT EXISTS "Discount_code_key" ON "Discount"("code");

-- CreateIndex
CREATE UNIQUE INDEX IF NOT EXISTS "Discount_lemonSqueezyId_key" ON "Discount"("lemonSqueezyId");

-- CreateIndex
CREATE INDEX IF NOT EXISTS "Discount_isActive_idx" ON "Discount"("isActive");
