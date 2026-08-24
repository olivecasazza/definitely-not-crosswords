/*
  Warnings:

  - You are about to drop the column `image` on the `User` table. All the data in the column will be lost.
  - Made the column `previousState` on table `GameAction` required. This step will fail if there are existing NULL values in that column.
  - Made the column `state` on table `GameAction` required. This step will fail if there are existing NULL values in that column.

*/
-- AlterTable
ALTER TABLE "GameAction" ALTER COLUMN "previousState" SET NOT NULL,
ALTER COLUMN "state" SET NOT NULL;

-- AlterTable
ALTER TABLE "MemberScore" ADD COLUMN     "correctGuesses" INTEGER NOT NULL DEFAULT 0,
ADD COLUMN     "incorrectGuesses" INTEGER NOT NULL DEFAULT 0;

-- AlterTable
ALTER TABLE "User" DROP COLUMN "image";

-- CreateTable
CREATE TABLE "UserSprite" (
    "id" TEXT NOT NULL,
    "blob" BYTEA NOT NULL,
    "userId" TEXT NOT NULL,

    CONSTRAINT "UserSprite_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "UserSprite_userId_key" ON "UserSprite"("userId");

-- AddForeignKey
ALTER TABLE "UserSprite" ADD CONSTRAINT "UserSprite_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
