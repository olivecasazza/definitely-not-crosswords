-- CreateEnum
CREATE TYPE "QuestionDirectionEnum" AS ENUM ('DOWN', 'ACROSS');

-- CreateEnum
CREATE TYPE "GameActionTypeEnum" AS ENUM ('correctGuess', 'incorrectGuess', 'placeholder');

-- CreateTable
CREATE TABLE "Account" (
    "id" TEXT NOT NULL,
    "userId" TEXT NOT NULL,
    "type" TEXT NOT NULL,
    "provider" TEXT NOT NULL,
    "providerAccountId" TEXT NOT NULL,
    "refresh_token" TEXT,
    "access_token" TEXT,
    "expires_at" INTEGER,
    "token_type" TEXT,
    "scope" TEXT,
    "id_token" TEXT,
    "session_state" TEXT,

    CONSTRAINT "Account_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Session" (
    "id" TEXT NOT NULL,
    "sessionToken" TEXT NOT NULL,
    "userId" TEXT NOT NULL,
    "expires" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "Session_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "User" (
    "id" TEXT NOT NULL,
    "name" TEXT,
    "email" TEXT,
    "emailVerified" TIMESTAMP(3),
    "image" TEXT,

    CONSTRAINT "User_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "VerificationToken" (
    "identifier" TEXT NOT NULL,
    "token" TEXT NOT NULL,
    "expires" TIMESTAMP(3) NOT NULL
);

-- CreateTable
CREATE TABLE "Question" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'Question',
    "number" INTEGER NOT NULL,
    "answer" TEXT NOT NULL,
    "questionText" TEXT NOT NULL,
    "rootX" INTEGER NOT NULL,
    "rootY" INTEGER NOT NULL,
    "direction" "QuestionDirectionEnum" NOT NULL DEFAULT 'DOWN',
    "gameId" TEXT NOT NULL,

    CONSTRAINT "Question_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Game" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'Game',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "title" TEXT NOT NULL,
    "published" BOOLEAN NOT NULL DEFAULT false,

    CONSTRAINT "Game_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "ActiveGame" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'ActiveGame',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "gameId" TEXT NOT NULL,

    CONSTRAINT "ActiveGame_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "GameAction" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'GameActions',
    "submittedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "activeGameId" TEXT NOT NULL,
    "actionType" "GameActionTypeEnum" NOT NULL,
    "cordX" INTEGER NOT NULL,
    "cordY" INTEGER NOT NULL,
    "previousState" TEXT,
    "state" TEXT,
    "userId" TEXT NOT NULL,

    CONSTRAINT "GameAction_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "CompletedGame" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'CompletedGame',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "gameId" TEXT NOT NULL,
    "gameStatsId" TEXT NOT NULL,

    CONSTRAINT "CompletedGame_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "GameStats" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'GameStats',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "GameStats_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "GameMember" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'GameMember',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "userId" TEXT NOT NULL,
    "isOwner" BOOLEAN NOT NULL DEFAULT false,
    "activeGameId" TEXT,
    "completedGameId" TEXT,

    CONSTRAINT "GameMember_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "MemberScore" (
    "id" TEXT NOT NULL,
    "type" TEXT NOT NULL DEFAULT 'MemberScore',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "memberId" TEXT NOT NULL,
    "gameStatsId" TEXT,
    "score" INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT "MemberScore_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "Account_provider_providerAccountId_key" ON "Account"("provider", "providerAccountId");

-- CreateIndex
CREATE UNIQUE INDEX "Session_sessionToken_key" ON "Session"("sessionToken");

-- CreateIndex
CREATE UNIQUE INDEX "User_email_key" ON "User"("email");

-- CreateIndex
CREATE UNIQUE INDEX "VerificationToken_token_key" ON "VerificationToken"("token");

-- CreateIndex
CREATE UNIQUE INDEX "VerificationToken_identifier_token_key" ON "VerificationToken"("identifier", "token");

-- AddForeignKey
ALTER TABLE "Account" ADD CONSTRAINT "Account_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Session" ADD CONSTRAINT "Session_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Question" ADD CONSTRAINT "Question_gameId_fkey" FOREIGN KEY ("gameId") REFERENCES "Game"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "ActiveGame" ADD CONSTRAINT "ActiveGame_gameId_fkey" FOREIGN KEY ("gameId") REFERENCES "Game"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "GameAction" ADD CONSTRAINT "GameAction_activeGameId_fkey" FOREIGN KEY ("activeGameId") REFERENCES "ActiveGame"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "GameAction" ADD CONSTRAINT "GameAction_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "CompletedGame" ADD CONSTRAINT "CompletedGame_gameId_fkey" FOREIGN KEY ("gameId") REFERENCES "Game"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "CompletedGame" ADD CONSTRAINT "CompletedGame_gameStatsId_fkey" FOREIGN KEY ("gameStatsId") REFERENCES "GameStats"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "GameMember" ADD CONSTRAINT "GameMember_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "GameMember" ADD CONSTRAINT "GameMember_activeGameId_fkey" FOREIGN KEY ("activeGameId") REFERENCES "ActiveGame"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "GameMember" ADD CONSTRAINT "GameMember_completedGameId_fkey" FOREIGN KEY ("completedGameId") REFERENCES "CompletedGame"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "MemberScore" ADD CONSTRAINT "MemberScore_memberId_fkey" FOREIGN KEY ("memberId") REFERENCES "GameMember"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "MemberScore" ADD CONSTRAINT "MemberScore_gameStatsId_fkey" FOREIGN KEY ("gameStatsId") REFERENCES "GameStats"("id") ON DELETE CASCADE ON UPDATE CASCADE;
