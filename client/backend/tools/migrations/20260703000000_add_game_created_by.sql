-- AlterTable
ALTER TABLE "Game" ADD COLUMN "createdById" TEXT;

-- AddForeignKey
ALTER TABLE "Game" ADD CONSTRAINT "Game_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON UPDATE CASCADE ON DELETE SET NULL;

-- CreateIndex
CREATE INDEX "Game_createdById_idx" ON "Game"("createdById");
