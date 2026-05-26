import { ActiveGame, CompletedGame, Game, GameAction } from "@prisma/client";
import { observable } from "@trpc/server/observable";
import { z } from "zod";
import { ee, prisma } from ".";
import { publicProcedure, router } from "../trpc";

export const gameListRouter = router({
  onAdd: publicProcedure.subscription(() => {
    return observable<GameAction[]>((emit) => {
      const onAdd = (data: GameAction[]) => emit.next(data);
      ee.on("add-game", onAdd);
      return () => ee.off("add-game", onAdd);
    });
  }),

  get: publicProcedure
    .input(
      z.object({
        email: z.string().email(),
      })
    )
    .query(async ({ input }) => {
      const activeGames = await prisma.activeGame.findMany({
        include: { game: true },
        where: { gameMembers: { some: { user: { email: input.email } } } },
      });
      const completedGames = await prisma.completedGame.findMany({
        include: { game: true },
        where: { gameMembers: { some: { user: { email: input.email } } } },
      });

      const filterIds = [
        ...completedGames.map((c) => c.game.id),
        ...activeGames.map((a) => a.game.id),
      ];
      const games = await prisma.game.findMany({
        where: {
          id: { notIn: filterIds },
          published: true,
        },
      });
      const combinedGames = [...games, ...completedGames, ...activeGames];
      return combinedGames;
    }),
});
