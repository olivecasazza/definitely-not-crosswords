import { adminProcedure, publicProcedure, router } from "../trpc";
import { z } from "zod";
import { prisma } from ".";
import { TRPCError } from "@trpc/server";
import { appRoles, roleCapabilities } from "../../../lib/auth/roles";

export const userRouter = router({
  listForAdmin: adminProcedure.query(async () => {
    return prisma.user.findMany({
      select: {
        id: true,
        email: true,
        username: true,
        name: true,
        role: true,
        emailVerified: true,
      },
      orderBy: [{ role: "asc" }, { email: "asc" }],
    });
  }),

  roleOptions: adminProcedure.query(() => {
    return appRoles.map((role) => ({
      role,
      capabilities: [...roleCapabilities[role]],
    }));
  }),

  upsertFromAdmin: adminProcedure
    .input(
      z.object({
        email: z.string().email(),
        name: z.string().trim().min(2).optional(),
        role: z.enum(appRoles),
      })
    )
    .mutation(async ({ input }) => {
      const email = input.email.trim().toLowerCase();
      const existingUser = await prisma.user.findUnique({
        where: { email },
        select: { id: true, emailVerified: true },
      });

      const user = await prisma.user.upsert({
        where: { email },
        create: {
          email,
          name: input.name,
          role: input.role,
          emailVerified: new Date(),
        },
        update: {
          role: input.role,
          ...(input.name ? { name: input.name } : {}),
          ...(existingUser?.emailVerified ? {} : { emailVerified: new Date() }),
        },
        select: {
          id: true,
          email: true,
          username: true,
          name: true,
          role: true,
          emailVerified: true,
        },
      });

      return { success: true, user };
    }),

  setRole: adminProcedure
    .input(
      z.object({
        userId: z.string().min(1),
        role: z.enum(appRoles),
      })
    )
    .mutation(async ({ input, ctx }) => {
      const targetUser = await prisma.user.findUnique({
        where: { id: input.userId },
        select: { id: true, role: true },
      });

      if (!targetUser) {
        throw new TRPCError({
          code: "NOT_FOUND",
          message: "User not found.",
        });
      }

      if (targetUser.id === ctx.user.id && targetUser.role !== input.role) {
        throw new TRPCError({
          code: "BAD_REQUEST",
          message: "Admins cannot change their own role.",
        });
      }

      if (targetUser.role === "ADMIN" && input.role !== "ADMIN") {
        const adminCount = await prisma.user.count({ where: { role: "ADMIN" } });
        if (adminCount <= 1) {
          throw new TRPCError({
            code: "BAD_REQUEST",
            message: "At least one admin must remain.",
          });
        }
      }

      const user = await prisma.user.update({
        where: { id: input.userId },
        data: { role: input.role },
        select: {
          id: true,
          email: true,
          username: true,
          name: true,
          role: true,
          emailVerified: true,
        },
      });

      return { success: true, user };
    }),

  signup: publicProcedure
    .input(
      z.object({
        email: z.string().email(),
        name: z.string().min(2),
        username: z.string().min(3),
        password: z.string().min(6),
      })
    )
    .mutation(async ({ input }) => {
      const existingUser = await prisma.user.findUnique({
        where: { email: input.email },
      });
      if (existingUser) {
        throw new TRPCError({
          code: "CONFLICT",
          message: "User with this email already exists.",
        });
      }

      const existingUsername = await prisma.user.findUnique({
        where: { username: input.username },
      });
      if (existingUsername) {
        throw new TRPCError({
          code: "CONFLICT",
          message: "User with this username already exists.",
        });
      }

      const user = await prisma.user.create({
        data: {
          email: input.email,
          name: input.name,
          username: input.username,
          password: input.password,
          emailVerified: null,
        },
      });

      const tokenString = "token_" + Math.random().toString(36).substring(2, 15);
      await prisma.verificationToken.create({
        data: {
          identifier: input.email,
          token: tokenString,
          expires: new Date(Date.now() + 24 * 60 * 60 * 1000),
        },
      });

      return {
        success: true,
        userId: user.id,
        verificationToken: tokenString,
      };
    }),

  isUsernameUnique: publicProcedure
    .input(z.object({ username: z.string() }))
    .query(async ({ input }) => {
      if (!input.username || input.username.trim().length < 3) {
        return { unique: true };
      }
      const existingUser = await prisma.user.findUnique({
        where: { username: input.username },
      });
      return { unique: !existingUser };
    }),

  isEmailUnique: publicProcedure
    .input(z.object({ email: z.string() }))
    .query(async ({ input }) => {
      if (!input.email || !input.email.includes("@")) {
        return { unique: true };
      }
      const existingUser = await prisma.user.findUnique({
        where: { email: input.email },
      });
      return { unique: !existingUser };
    }),

  verifyEmail: publicProcedure
    .input(
      z.object({
        token: z.string(),
      })
    )
    .mutation(async ({ input }) => {
      const tokenRecord = await prisma.verificationToken.findUnique({
        where: { token: input.token },
      });

      if (!tokenRecord) {
        throw new TRPCError({
          code: "NOT_FOUND",
          message: "Invalid or expired verification token.",
        });
      }

      if (tokenRecord.expires < new Date()) {
        await prisma.verificationToken.delete({
          where: { token: input.token },
        });
        throw new TRPCError({
          code: "BAD_REQUEST",
          message: "Verification token has expired.",
        });
      }

      await prisma.user.update({
        where: { email: tokenRecord.identifier },
        data: { emailVerified: new Date() },
      });

      await prisma.verificationToken.delete({
        where: { token: input.token },
      });

      return { success: true };
    }),

  getProfile: publicProcedure
    .input(
      z.object({
        email: z.string().email(),
      })
    )
    .query(async ({ input }) => {
      const user = await prisma.user.findUnique({
        where: { email: input.email },
        select: {
          id: true,
          name: true,
          email: true,
          emailVerified: true,
        },
      });

      if (!user) {
        throw new TRPCError({
          code: "NOT_FOUND",
          message: "User not found.",
        });
      }

      return user;
    }),

  updateProfile: publicProcedure
    .input(
      z.object({
        email: z.string().email(),
        name: z.string().min(2),
      })
    )
    .mutation(async ({ input }) => {
      const user = await prisma.user.update({
        where: { email: input.email },
        data: { name: input.name },
      });

      return {
        success: true,
        name: user.name,
      };
    }),

  deleteAccount: publicProcedure
    .input(
      z.object({
        email: z.string().email(),
      })
    )
    .mutation(async ({ input }) => {
      await prisma.user.delete({
        where: { email: input.email },
      });

      return { success: true };
    }),
});
