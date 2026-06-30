-- Add credential fields used by local sign-up.
ALTER TABLE "User" ADD COLUMN "username" TEXT;
ALTER TABLE "User" ADD COLUMN "password" TEXT;

CREATE UNIQUE INDEX "User_username_key" ON "User"("username");
