import { publicProcedure, router } from "../trpc";
import { z } from "zod";
import { prisma } from ".";
import { TRPCError } from "@trpc/server";

export const userRouter = router({
  signup: publicProcedure
    .input(
      z.object({
        email: z.string().email(),
        name: z.string().min(2),
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

      const user = await prisma.user.create({
        data: {
          email: input.email,
          name: input.name,
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
