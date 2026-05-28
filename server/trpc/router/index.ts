import EventEmitter from 'events';
import { router } from '../trpc';
import { messageRouter } from './message';
import pkg from '@prisma/client';
import type { PrismaClient } from '@prisma/client';
const { PrismaClient } = pkg;
import { gameListRouter } from './gameList';
import { activeGameRouter } from './activeGame';
import { generatorRouter } from './generator';
import { statsRouter } from './stats';
import { userRouter } from './user';

export const ee = new EventEmitter();
export const prisma = new PrismaClient();

export const appRouter = router({
    message: messageRouter,
    activeGame: activeGameRouter,
    gameList: gameListRouter,
    generator: generatorRouter,
    stats: statsRouter,
    user: userRouter
});

export type AppRouter = typeof appRouter;
