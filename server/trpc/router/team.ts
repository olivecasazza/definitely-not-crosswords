import { z } from "zod";
import { protectedProcedure, publicProcedure, router } from "../trpc";
import { prisma } from ".";

/// Pro = active/cancelled subscription or a vipPass (same rule as
/// subscription.ts / generator.ts). Creating a team is gated on this.
async function userIsPro(userId: string): Promise<boolean> {
  const [user, subscription] = await Promise.all([
    prisma.user.findUnique({ where: { id: userId }, select: { vipPass: true } }),
    prisma.subscription.findUnique({ where: { userId }, select: { status: true } }),
  ]);
  return (
    subscription?.status === "ACTIVE" ||
    subscription?.status === "CANCELLED" ||
    !!user?.vipPass
  );
}

export const teamRouter = router({
  // Creating a team is a Pro feature; joining one is free.
  create: protectedProcedure
    .input(z.object({ name: z.string().min(2).max(40) }))
    .mutation(async ({ ctx, input }) => {
      if (!(await userIsPro(ctx.user.id))) {
        throw new Error("Creating a team is a Pro feature — upgrade to Pro to create teams.");
      }
      const name = input.name.trim();
      const existing = await prisma.team.findUnique({ where: { name } });
      if (existing) throw new Error("A team with that name already exists.");
      return prisma.team.create({
        data: {
          name,
          ownerId: ctx.user.id,
          members: { create: { userId: ctx.user.id } },
        },
        include: { _count: { select: { members: true } } },
      });
    }),

  list: publicProcedure.query(async () => {
    const teams = await prisma.team.findMany({
      include: {
        _count: { select: { members: true } },
        owner: { select: { name: true, email: true } },
      },
      orderBy: { createdAt: "desc" },
    });
    return teams.map((t) => ({
      id: t.id,
      name: t.name,
      owner: t.owner.name || t.owner.email || "Unknown",
      memberCount: t._count.members,
    }));
  }),

  myTeams: protectedProcedure.query(async ({ ctx }) => {
    const memberships = await prisma.teamMember.findMany({
      where: { userId: ctx.user.id },
      include: { team: { include: { _count: { select: { members: true } } } } },
    });
    return memberships.map((m) => ({
      id: m.team.id,
      name: m.team.name,
      memberCount: m.team._count.members,
      isOwner: m.team.ownerId === ctx.user.id,
    }));
  }),

  join: protectedProcedure
    .input(z.object({ teamId: z.string() }))
    .mutation(async ({ ctx, input }) => {
      await prisma.teamMember.upsert({
        where: { teamId_userId: { teamId: input.teamId, userId: ctx.user.id } },
        create: { teamId: input.teamId, userId: ctx.user.id },
        update: {},
      });
      return { joined: true };
    }),

  leave: protectedProcedure
    .input(z.object({ teamId: z.string() }))
    .mutation(async ({ ctx, input }) => {
      await prisma.teamMember.deleteMany({
        where: { teamId: input.teamId, userId: ctx.user.id },
      });
      return { left: true };
    }),

  // Team leaderboard: sum members' completed-game scores (mirrors the global
  // player leaderboard aggregation in stats.getGlobalLeaderboard).
  getTeamLeaderboard: publicProcedure.query(async () => {
    const teams = await prisma.team.findMany({
      include: {
        members: {
          include: {
            user: {
              include: {
                gameMember: {
                  where: { completedGameId: { not: null } },
                  include: { memberScore: true },
                },
              },
            },
          },
        },
      },
    });

    const board = teams.map((team) => {
      let totalScore = 0;
      let gamesPlayed = 0;
      let totalCorrect = 0;
      let totalIncorrect = 0;
      team.members.forEach((m) => {
        m.user.gameMember.forEach((gm) => {
          gamesPlayed++;
          gm.memberScore.forEach((ms) => {
            totalScore += ms.score;
            totalCorrect += ms.correctGuesses;
            totalIncorrect += ms.incorrectGuesses;
          });
        });
      });
      const totalGuesses = totalCorrect + totalIncorrect;
      const accuracy = totalGuesses > 0 ? Math.round((totalCorrect / totalGuesses) * 100) : 0;
      return {
        id: team.id,
        name: team.name,
        memberCount: team.members.length,
        totalScore,
        gamesPlayed,
        accuracy,
      };
    });

    return board.sort(
      (a, b) =>
        b.totalScore - a.totalScore ||
        b.gamesPlayed - a.gamesPlayed ||
        a.name.localeCompare(b.name),
    );
  }),
});
