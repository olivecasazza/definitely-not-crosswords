import type { inferAsyncReturnType } from '@trpc/server'
import { EventEmitter } from 'events';
import type { H3Event } from 'h3';
import { getServerSession } from '#auth';
import pkg from '@prisma/client';
import type { PrismaClient } from '@prisma/client';
const { PrismaClient } = pkg;

const prisma = new PrismaClient();

export const createContext = async (event?: H3Event) => {
  const ee = new EventEmitter();

  let user: { id: string; email: string; role: string } | null = null;

  if (event) {
    try {
      const session = await getServerSession(event);
      if (session?.user?.email) {
        const dbUser = await prisma.user.findUnique({
          where: { email: session.user.email },
          select: { id: true, email: true, role: true },
        });
        if (dbUser && dbUser.email) {
          user = { id: dbUser.id, email: dbUser.email, role: dbUser.role };
        }
      }
    } catch {
      // Session retrieval failed — user stays null (unauthenticated)
    }
  }

  return { ee, user, prisma };
}

export type Context = inferAsyncReturnType<typeof createContext>;
