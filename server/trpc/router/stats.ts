import { z } from "zod";
import { publicProcedure, router } from "../trpc";
import { prisma } from ".";

export const statsRouter = router({
  // Get details of a specific completed game by ID
  getCompletedGame: publicProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ input }) => {
      return await prisma.completedGame.findUnique({
        where: { id: input.id },
        include: {
          game: true,
          gameStats: {
            include: {
              memberScores: {
                include: {
                  member: {
                    include: {
                      user: true,
                    },
                  },
                },
              },
            },
          },
        },
      });
    }),

  // Get all users for the comparison dropdown
  getAllPlayers: publicProcedure
    .input(z.object({ excludeEmail: z.string().optional() }))
    .query(async ({ input }) => {
      const players = await prisma.user.findMany({
        where: input.excludeEmail ? { email: { not: input.excludeEmail } } : {},
        select: {
          id: true,
          name: true,
          email: true,
        },
        orderBy: { name: "asc" },
      });
      return players;
    }),

  // Get global leaderboard of all players
  getGlobalLeaderboard: publicProcedure.query(async () => {
    const users = await prisma.user.findMany({
      include: {
        gameMember: {
          where: { completedGameId: { not: null } },
          include: {
            memberScore: true,
          },
        },
      },
    });

    const leaderboard = users.map((user) => {
      let gamesPlayed = 0;
      let totalScore = 0;
      let totalCorrect = 0;
      let totalIncorrect = 0;

      user.gameMember.forEach((gm) => {
        gamesPlayed++;
        gm.memberScore.forEach((ms) => {
          totalScore += ms.score;
          totalCorrect += ms.correctGuesses;
          totalIncorrect += ms.incorrectGuesses;
        });
      });

      const totalGuesses = totalCorrect + totalIncorrect;
      const accuracy = totalGuesses > 0 ? Math.round((totalCorrect / totalGuesses) * 100) : 0;

      return {
        id: user.id,
        name: user.name || user.email || "Anonymous Player",
        email: user.email,
        gamesPlayed,
        totalScore,
        totalCorrect,
        totalIncorrect,
        accuracy,
      };
    });

    // Sort by score descending, then by games played descending, then by name
    return leaderboard.sort((a, b) => {
      if (b.totalScore !== a.totalScore) return b.totalScore - a.totalScore;
      if (b.gamesPlayed !== a.gamesPlayed) return b.gamesPlayed - a.gamesPlayed;
      return a.name.localeCompare(b.name);
    });
  }),

  // Get deep statistics for a single user by email
  getUserStats: publicProcedure
    .input(z.object({ email: z.string().email() }))
    .query(async ({ input }) => {
      const user = await prisma.user.findUnique({
        where: { email: input.email },
        include: {
          gameMember: {
            where: { completedGameId: { not: null } },
            include: {
              completedGame: {
                include: {
                  game: true,
                  gameStats: {
                    include: {
                      memberScores: {
                        include: {
                          member: {
                            include: {
                              user: true,
                            },
                          },
                        },
                      },
                    },
                  },
                },
              },
              memberScore: true,
            },
          },
        },
      });

      if (!user) {
        throw new Error("User not found");
      }

      // Calculate career aggregates
      let gamesPlayed = 0;
      let totalScore = 0;
      let totalCorrect = 0;
      let totalIncorrect = 0;

      user.gameMember.forEach((gm) => {
        gamesPlayed++;
        gm.memberScore.forEach((ms) => {
          totalScore += ms.score;
          totalCorrect += ms.correctGuesses;
          totalIncorrect += ms.incorrectGuesses;
        });
      });

      const totalGuesses = totalCorrect + totalIncorrect;
      const accuracy = totalGuesses > 0 ? Math.round((totalCorrect / totalGuesses) * 100) : 0;

      // Compile recent games list with ranking inside each game
      const recentGames = user.gameMember.map((gm) => {
        const cg = gm.completedGame;
        if (!cg) return null;

        // Find user's score in this game
        const userScoreRecord = cg.gameStats.memberScores.find(
          (ms) => ms.member.userId === user.id
        );
        const userScore = userScoreRecord?.score || 0;
        const userCorrect = userScoreRecord?.correctGuesses || 0;
        const userIncorrect = userScoreRecord?.incorrectGuesses || 0;

        // Sort all member scores to find rankings
        const sortedScores = [...cg.gameStats.memberScores].sort((a, b) => b.score - a.score);
        const rankIndex = sortedScores.findIndex((ms) => ms.member.userId === user.id);
        const rank = rankIndex !== -1 ? rankIndex + 1 : 1;

        return {
          id: cg.id,
          title: cg.game.title,
          createdAt: cg.createdAt,
          score: userScore,
          correctGuesses: userCorrect,
          incorrectGuesses: userIncorrect,
          rank,
          totalParticipants: sortedScores.length,
          allScores: sortedScores.map((s) => ({
            playerName: s.member.user.name || s.member.user.email || "Anonymous",
            score: s.score,
          })),
        };
      }).filter(Boolean);

      // Sort recent games by completed date descending
      recentGames.sort((a, b) => new Date(b!.createdAt).getTime() - new Date(a!.createdAt).getTime());

      // Get rank dynamically by running the leaderboard query
      const allUsers = await prisma.user.findMany({
        include: {
          gameMember: {
            where: { completedGameId: { not: null } },
            include: { memberScore: true },
          },
        },
      });

      const scores = allUsers.map((u) => {
        let scoreSum = 0;
        u.gameMember.forEach((gm) => {
          gm.memberScore.forEach((ms) => {
            scoreSum += ms.score;
          });
        });
        return { userId: u.id, score: scoreSum };
      }).sort((a, b) => b.score - a.score);

      const rankIndex = scores.findIndex((s) => s.userId === user.id);
      const globalRank = rankIndex !== -1 ? rankIndex + 1 : scores.length;

      return {
        profile: {
          id: user.id,
          name: user.name,
          email: user.email,
        },
        gamesPlayed,
        totalScore,
        totalCorrect,
        totalIncorrect,
        accuracy,
        globalRank,
        totalPlayers: scores.length,
        recentGames,
      };
    }),

  // Get Head-to-Head comparison between the current user and an opponent
  getHeadToHead: publicProcedure
    .input(
      z.object({
        userEmail: z.string().email(),
        opponentId: z.string(),
      })
    )
    .query(async ({ input }) => {
      const user = await prisma.user.findUnique({
        where: { email: input.userEmail },
      });
      const opponent = await prisma.user.findUnique({
        where: { id: input.opponentId },
      });

      if (!user || !opponent) {
        throw new Error("User or opponent not found");
      }

      // Find all completed games BOTH participated in
      const commonGames = await prisma.completedGame.findMany({
        where: {
          AND: [
            { gameMembers: { some: { userId: user.id } } },
            { gameMembers: { some: { userId: opponent.id } } },
          ],
        },
        include: {
          game: true,
          gameStats: {
            include: {
              memberScores: {
                include: {
                  member: {
                    include: {
                      user: true,
                    },
                  },
                },
              },
            },
          },
        },
      });

      let gamesPlayed = commonGames.length;
      let userWins = 0;
      let opponentWins = 0;
      let ties = 0;

      let userTotalScore = 0;
      let opponentTotalScore = 0;

      let userTotalCorrect = 0;
      let opponentTotalCorrect = 0;
      let userTotalIncorrect = 0;
      let opponentTotalIncorrect = 0;

      const matches = commonGames.map((cg) => {
        const userScoreRecord = cg.gameStats.memberScores.find(
          (ms) => ms.member.userId === user.id
        );
        const opponentScoreRecord = cg.gameStats.memberScores.find(
          (ms) => ms.member.userId === opponent.id
        );

        const userScore = userScoreRecord?.score || 0;
        const opponentScore = opponentScoreRecord?.score || 0;

        userTotalScore += userScore;
        opponentTotalScore += opponentScore;

        userTotalCorrect += userScoreRecord?.correctGuesses || 0;
        opponentTotalCorrect += opponentScoreRecord?.correctGuesses || 0;
        userTotalIncorrect += userScoreRecord?.incorrectGuesses || 0;
        opponentTotalIncorrect += opponentScoreRecord?.incorrectGuesses || 0;

        let result = "TIE";
        if (userScore > opponentScore) {
          userWins++;
          result = "WIN";
        } else if (opponentScore > userScore) {
          opponentWins++;
          result = "LOSS";
        } else {
          ties++;
        }

        return {
          gameId: cg.id,
          title: cg.game.title,
          createdAt: cg.createdAt,
          userScore,
          opponentScore,
          result,
        };
      });

      // Sort matches by date descending
      matches.sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());

      const userGuesses = userTotalCorrect + userTotalIncorrect;
      const userAccuracy = userGuesses > 0 ? Math.round((userTotalCorrect / userGuesses) * 100) : 0;

      const opponentGuesses = opponentTotalCorrect + opponentTotalIncorrect;
      const opponentAccuracy = opponentGuesses > 0 ? Math.round((opponentTotalCorrect / opponentGuesses) * 100) : 0;

      return {
        opponentName: opponent.name || opponent.email || "Opponent",
        gamesPlayed,
        record: {
          wins: userWins,
          losses: opponentWins,
          ties,
        },
        scores: {
          userTotal: userTotalScore,
          userAvg: gamesPlayed > 0 ? Math.round(userTotalScore / gamesPlayed) : 0,
          opponentTotal: opponentTotalScore,
          opponentAvg: gamesPlayed > 0 ? Math.round(opponentTotalScore / gamesPlayed) : 0,
        },
        accuracy: {
          user: userAccuracy,
          opponent: opponentAccuracy,
          userCorrect: userTotalCorrect,
          opponentCorrect: opponentTotalCorrect,
        },
        matches,
      };
    }),
});
