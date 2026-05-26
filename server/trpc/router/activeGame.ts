import { observable } from "@trpc/server/observable";
import { publicProcedure, router } from "../trpc";
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
  addActions: publicProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        userEmail: z.string(),
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
    .mutation(async (opts) => {
      const user = await prisma.user.findUnique({
        where: { email: opts.input.userEmail },
        select: { id: true },
      });
      const createdActions = opts.input.actions.map((a) => {
        return {
          id: crypto.randomUUID(),
          ...a,
          userId: user?.id || "",
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
});
