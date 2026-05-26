import EventEmitter from 'events';
import { router } from '../trpc';
import { messageRouter } from './message';
import { PrismaClient } from '@prisma/client';
import { gameListRouter } from './gameList';
import { activeGameRouter } from './activeGame';
import { generatorRouter } from './generator';
import { statsRouter } from './stats';

export const ee = new EventEmitter();
export const prisma = new PrismaClient();

export const appRouter = router({
    message: messageRouter,
    activeGame: activeGameRouter,
    gameList: gameListRouter,
    generator: generatorRouter,
    stats: statsRouter
});

export type AppRouter = typeof appRouter;
