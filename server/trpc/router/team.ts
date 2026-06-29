import { z } from "zod";
import { protectedProcedure, publicProcedure, router } from "../trpc";
import { prisma } from ".";

const FREE_MAX_SIZE = 4;
const PRO_MAX_SIZE = 10;

/// Pro = active/cancelled subscription or a vipPass (same rule as
/// subscription.ts / generator.ts). Determines a new team's max size.
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

async function isMember(teamId: string, userId: string): Promise<boolean> {
  const m = await prisma.teamMember.findUnique({
    where: { teamId_userId: { teamId, userId } },
  });
  return !!m;
}

export const teamRouter = router({
  // Anyone can create a team; max size is Pro-tiered.
  create: protectedProcedure
    .input(
      z.object({
        name: z.string().min(2).max(40),
        visibility: z.enum(["PUBLIC", "PRIVATE"]).default("PUBLIC"),
      }),
    )
    .mutation(async ({ ctx, input }) => {
      const name = input.name.trim();
      if (await prisma.team.findUnique({ where: { name } })) {
        throw new Error("A team with that name already exists.");
      }
      const maxSize = (await userIsPro(ctx.user.id)) ? PRO_MAX_SIZE : FREE_MAX_SIZE;
      return prisma.team.create({
        data: {
          name,
          ownerId: ctx.user.id,
          visibility: input.visibility,
          maxSize,
          members: { create: { userId: ctx.user.id } },
        },
        include: { _count: { select: { members: true } } },
      });
    }),

  setVisibility: protectedProcedure
    .input(z.object({ teamId: z.string(), visibility: z.enum(["PUBLIC", "PRIVATE"]) }))
    .mutation(async ({ ctx, input }) => {
      const team = await prisma.team.findUnique({ where: { id: input.teamId } });
      if (!team || team.ownerId !== ctx.user.id) {
        throw new Error("Only the team owner can change visibility.");
      }
      await prisma.team.update({
        where: { id: input.teamId },
        data: { visibility: input.visibility },
      });
      return { ok: true };
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
      maxSize: t.maxSize,
      visibility: t.visibility,
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
      maxSize: m.team.maxSize,
      visibility: m.team.visibility,
      isOwner: m.team.ownerId === ctx.user.id,
    }));
  }),

  // Public teams only; private teams require an invite.
  join: protectedProcedure
    .input(z.object({ teamId: z.string() }))
    .mutation(async ({ ctx, input }) => {
      const team = await prisma.team.findUnique({
        where: { id: input.teamId },
        include: { _count: { select: { members: true } } },
      });
      if (!team) throw new Error("Team not found.");
      if (team.visibility === "PRIVATE") {
        throw new Error("This team is invite-only.");
      }
      if (await isMember(team.id, ctx.user.id)) return { joined: true };
      if (team._count.members >= team.maxSize) {
        throw new Error("This team is full.");
      }
      await prisma.teamMember.create({
        data: { teamId: team.id, userId: ctx.user.id },
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

  // A member invites someone by username or email.
  invite: protectedProcedure
    .input(z.object({ teamId: z.string(), identifier: z.string().min(1) }))
    .mutation(async ({ ctx, input }) => {
      if (!(await isMember(input.teamId, ctx.user.id))) {
        throw new Error("Only team members can invite.");
      }
      const team = await prisma.team.findUnique({
        where: { id: input.teamId },
        include: { _count: { select: { members: true } } },
      });
      if (!team) throw new Error("Team not found.");
      if (team._count.members >= team.maxSize) throw new Error("This team is full.");

      const id = input.identifier.trim();
      const invitee = await prisma.user.findFirst({
        where: { OR: [{ username: id }, { email: id.toLowerCase() }] },
        select: { id: true },
      });
      if (!invitee) throw new Error("No user found with that username or email.");
      if (await isMember(team.id, invitee.id)) {
        throw new Error("That user is already on the team.");
      }

      await prisma.teamInvite.upsert({
        where: { teamId_inviteeId: { teamId: team.id, inviteeId: invitee.id } },
        create: {
          teamId: team.id,
          inviteeId: invitee.id,
          invitedById: ctx.user.id,
          status: "PENDING",
        },
        update: { status: "PENDING", invitedById: ctx.user.id },
      });
      return { invited: true };
    }),

  // Invites awaiting the current user's response.
  myInvites: protectedProcedure.query(async ({ ctx }) => {
    const invites = await prisma.teamInvite.findMany({
      where: { inviteeId: ctx.user.id, status: "PENDING" },
      include: {
        team: { select: { id: true, name: true } },
        invitedBy: { select: { name: true, email: true } },
      },
      orderBy: { createdAt: "desc" },
    });
    return invites.map((i) => ({
      id: i.id,
      teamId: i.team.id,
      teamName: i.team.name,
      invitedBy: i.invitedBy.name || i.invitedBy.email || "Someone",
    }));
  }),

  respondToInvite: protectedProcedure
    .input(z.object({ inviteId: z.string(), accept: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const invite = await prisma.teamInvite.findUnique({
        where: { id: input.inviteId },
        include: { team: { include: { _count: { select: { members: true } } } } },
      });
      if (!invite || invite.inviteeId !== ctx.user.id) {
        throw new Error("Invite not found.");
      }
      if (!input.accept) {
        await prisma.teamInvite.update({
          where: { id: invite.id },
          data: { status: "DECLINED" },
        });
        return { accepted: false };
      }
      if (invite.team._count.members >= invite.team.maxSize) {
        throw new Error("This team is now full.");
      }
      await prisma.$transaction([
        prisma.teamMember.upsert({
          where: { teamId_userId: { teamId: invite.teamId, userId: ctx.user.id } },
          create: { teamId: invite.teamId, userId: ctx.user.id },
          update: {},
        }),
        prisma.teamInvite.update({
          where: { id: invite.id },
          data: { status: "ACCEPTED" },
        }),
      ]);
      return { accepted: true };
    }),

  // Team leaderboard: sum members' completed-game scores.
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
        maxSize: team.maxSize,
        visibility: team.visibility,
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
