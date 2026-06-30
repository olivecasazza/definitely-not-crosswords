-- CreateEnum
CREATE TYPE "DiscountAmountType" AS ENUM ('PERCENT', 'FIXED');

-- CreateEnum
CREATE TYPE "DiscountDuration" AS ENUM ('ONCE', 'FOREVER', 'REPEATING');

-- CreateTable
CREATE TABLE "Discount" (
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
CREATE UNIQUE INDEX "Discount_code_key" ON "Discount"("code");

-- CreateIndex
CREATE UNIQUE INDEX "Discount_lemonSqueezyId_key" ON "Discount"("lemonSqueezyId");

-- CreateIndex
CREATE INDEX "Discount_isActive_idx" ON "Discount"("isActive");
