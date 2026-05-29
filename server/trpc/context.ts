import type { inferAsyncReturnType } from '@trpc/server'
import { EventEmitter } from 'events';
import type { IncomingMessage } from 'node:http';
import type { H3Event } from 'h3';
import { getServerSession } from '#auth';
import { getToken } from 'next-auth/jwt';
import pkg from '@prisma/client';
import type { PrismaClient } from '@prisma/client';
const { PrismaClient } = pkg;

const prisma = new PrismaClient();

type ContextRequest = H3Event | IncomingMessage;

async function findUserByEmail(email?: string | null) {
  if (!email) return null;

  const dbUser = await prisma.user.findUnique({
    where: { email },
    select: { id: true, email: true, role: true },
  });

  if (!dbUser?.email) return null;

  return { id: dbUser.id, email: dbUser.email, role: dbUser.role };
}

function isH3Event(input: ContextRequest): input is H3Event {
  return "node" in input;
}

export const createContext = async (input?: ContextRequest) => {
  const ee = new EventEmitter();

  let user: { id: string; email: string; role: string } | null = null;

  if (input) {
    try {
      if (isH3Event(input)) {
        const session = await getServerSession(input);
        user = await findUserByEmail(session?.user?.email);
      }

      if (!user) {
        const req = isH3Event(input) ? input.node.req : input;
        const token = await getToken({ req, secret: process.env.NEXTAUTH_SECRET });
        user = await findUserByEmail(token?.email);
      }
    } catch {
      // Session retrieval failed — user stays null (unauthenticated)
    }
  }

  return { ee, user, prisma };
}

export type Context = inferAsyncReturnType<typeof createContext>;
