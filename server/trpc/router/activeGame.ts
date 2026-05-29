import { observable } from "@trpc/server/observable";
import { protectedProcedure, publicProcedure, router } from "../trpc";
import { z } from "zod";
import { ee, prisma } from ".";
import { GameAction, Prisma } from "@prisma/client";
import crypto from "node:crypto";

export const activeGameRouter = router({
  onAddActions: publicProcedure.subscription(() => {
    return observable<GameAction[]>((emit) => {
      const onAdd = (data: GameAction[]) => emit.next(data);
      ee.on("add-game", onAdd);
      return () => ee.off("add-game", onAdd);
    });
  }),
  onGameCompleted: publicProcedure.subscription(() => {
    return observable<{ activeGameId: string; completedGameId: string }>((emit) => {
      const onComplete = (data: { activeGameId: string; completedGameId: string }) => emit.next(data);
      ee.on("game-completed", onComplete);
      return () => ee.off("game-completed", onComplete);
    });
  }),
  addActions: protectedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        actions: z
          .object({
            activeGameId: z.string(),
            cordX: z.number(),
            cordY: z.number(),
            actionType: z.string(),
            previousState: z.string(),
            state: z.string(),
          })
          .array(),
      })
    )
    .mutation(async ({ input, ctx }) => {
      const createdActions = input.actions.map((a) => {
        return {
          id: crypto.randomUUID(),
          ...a,
          userId: ctx.user.id,
          type: "GameAction",
          submittedAt: new Date(),
        } as GameAction;
      });
      await prisma.gameAction.createMany({
        data: createdActions,
      });
      ee.emit("add-game", createdActions);
      return createdActions;
    }),

  get: publicProcedure
    .input(
      z.object({
        id: z.string(),
      })
    )
    .query(async ({ input }) => {
      return await prisma.activeGame.findUnique({
        where: { id: input.id },
        include: {
          actions: true,
          gameMembers: true,
          game: {
            include: {
              questions: true,
            },
          },
        },
      });
    }),

  complete: publicProcedure
    .input(
      z.object({
        id: z.string().uuid(),
      })
    )
    .mutation(async ({ input }) => {
      const activeGameId = input.id;
      const activeGame = await prisma.activeGame.findUnique({
        where: { id: activeGameId },
        include: {
          actions: true,
          gameMembers: {
            include: {
              user: true,
            },
          },
          game: {
            include: {
              questions: true,
            },
          },
        },
      });

      if (!activeGame) {
        throw new Error("Active game not found");
      }

      // Calculate scores and guess statistics for each member
      const memberStats = activeGame.gameMembers.map((member) => {
        const userActions = activeGame.actions.filter((a) => a.userId === member.userId);
        const correctGuesses = userActions.filter((a) => a.actionType === "correctGuess").length;
        const incorrectGuesses = userActions.filter((a) => a.actionType === "incorrectGuess").length;
        const score = Math.max(0, correctGuesses * 10 - incorrectGuesses * 2);

        return {
          memberId: member.id,
          userId: member.userId,
          score,
          correctGuesses,
          incorrectGuesses,
        };
      });

      // Create the GameStats and CompletedGame inside a transaction
      const result = await prisma.$transaction(async (tx) => {
        const gameStats = await tx.gameStats.create({
          data: {},
        });

        const completedGame = await tx.completedGame.create({
          data: {
            gameId: activeGame.gameId,
            gameStatsId: gameStats.id,
          },
        });

        // Create the MemberScores and link to gameStats
        for (const stats of memberStats) {
          await tx.memberScore.create({
            data: {
              score: stats.score,
              correctGuesses: stats.correctGuesses,
              incorrectGuesses: stats.incorrectGuesses,
              memberId: stats.memberId,
              gameStatsId: gameStats.id,
            },
          });
        }

        // Update all GameMembers: disconnect activeGame and connect completedGame
        for (const member of activeGame.gameMembers) {
          await tx.gameMember.update({
            where: { id: member.id },
            data: {
              completedGameId: completedGame.id,
              activeGameId: null,
            },
          });
        }

        // Delete the ActiveGame record (cascades and deletes GameActions)
        await tx.activeGame.delete({
          where: { id: activeGameId },
        });

        return completedGame;
      });

      // Emit real-time notification
      ee.emit("game-completed", {
        activeGameId,
        completedGameId: result.id,
      });

      return result;
    }),
});

