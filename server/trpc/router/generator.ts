import { TRPCError } from "@trpc/server";
import { observable } from "@trpc/server/observable";
import { z } from "zod";
import { prisma } from ".";
import { adminProcedure, router } from "../trpc";
import {
  generateCrosswordFromDictionary,
  type GenerationProgressCallback,
  type GenerationProgressEvent,
} from "../../services/crossword/generateCrossword";
import { roleHasCapability } from "../../../lib/auth/roles";

const generationParamsSchema = z.object({
  topic: z.string().trim().min(1),
  width: z.number().int().min(3).max(50),
  height: z.number().int().min(3).max(50),
  minWordLength: z.number().int().min(2).max(50),
  maxWordLength: z.number().int().min(2).max(50),
  targetWords: z.number().int().min(1).max(250).default(42),
  runs: z.number().int().min(1).max(100).default(20),
  maxAttempts: z.number().int().min(1).max(1000).default(180),
  topics: z.array(z.string().trim().min(1)).min(1).optional(),
});

const generatedQuestionSchema = z.object({
  number: z.number().int().positive(),
  answer: z.string().trim().min(1).regex(/^[A-Za-z]+$/),
  questionText: z.string().trim().min(1),
  rootX: z.number().int().min(0),
  rootY: z.number().int().min(0),
  direction: z.enum(["ACROSS", "DOWN"]),
});

function validateGenerationParams(params: z.infer<typeof generationParamsSchema>) {
  if (params.minWordLength > params.maxWordLength) {
    throw new TRPCError({
      code: "BAD_REQUEST",
      message: "minWordLength cannot be greater than maxWordLength.",
    });
  }

  if (params.maxWordLength > Math.max(params.width, params.height)) {
    throw new TRPCError({
      code: "BAD_REQUEST",
      message: "maxWordLength cannot exceed the larger grid dimension.",
    });
  }
}

/**
 * Check whether a user is allowed to generate. Users with generator management
 * capability and Pro users are unlimited; free users get 5 per calendar month.
 * Throws TRPCError FORBIDDEN when the free-tier limit is reached.
 */
async function checkQuota(userId: string, role: string): Promise<{ isUnlimited: boolean }> {
  if (roleHasCapability(role, "generator:manage")) {
    return { isUnlimited: true };
  }

  const subscription = await prisma.subscription.findUnique({
    where: { userId },
    select: { status: true },
  });
  const isPro = subscription?.status === 'ACTIVE' || subscription?.status === 'CANCELLED';

  if (!isPro) {
    const now = new Date();
    let quota = await prisma.generationQuota.findUnique({
      where: { userId },
    });

    if (!quota) {
      quota = await prisma.generationQuota.create({
        data: { userId },
      });
    }

    // Lazy monthly reset
    const resetDate = new Date(quota.monthResetAt);
    const isCurrentMonth =
      resetDate.getUTCFullYear() === now.getUTCFullYear() &&
      resetDate.getUTCMonth() === now.getUTCMonth();

    if (!isCurrentMonth) {
      quota = await prisma.generationQuota.update({
        where: { id: quota.id },
        data: { usedThisMonth: 0, monthResetAt: now },
      });
    }

    if (quota.usedThisMonth >= 5) {
      throw new TRPCError({
        code: 'FORBIDDEN',
        message: 'Monthly generation limit reached. Upgrade to Pro for unlimited generations.',
      });
    }
  }

  return { isUnlimited: isPro };
}

/** Increment the monthly generation counter for a free-tier user. */
async function incrementQuota(userId: string): Promise<void> {
  await prisma.generationQuota.update({
    where: { userId },
    data: { usedThisMonth: { increment: 1 } },
  });
}

/**
 * Events streamed to the admin UI over the `runGeneration` subscription:
 * `started` first (carries the job id), then forwarded pipeline progress/log
 * events, terminating in exactly one `completed` or `failed`.
 */
export type GenerationEvent =
  | { type: "started"; jobId: string; at: number }
  | ({ at: number } & GenerationProgressEvent)
  | {
      type: "completed";
      jobId: string;
      gameId: string;
      title: string;
      questionCount: number;
      metrics: Record<string, unknown>;
      at: number;
    }
  | { type: "failed"; jobId: string | null; error: string; at: number };

type PersistedGenerationEvent = GenerationEvent | ({ at: number } & GenerationProgressEvent);

function compactGenerationParams(params: z.infer<typeof generationParamsSchema>) {
  return {
    topic: params.topic,
    grid: `${params.width}x${params.height}`,
    minWordLength: params.minWordLength,
    maxWordLength: params.maxWordLength,
    targetWords: params.targetWords,
    runs: params.runs,
    maxAttempts: params.maxAttempts,
  };
}

/**
 * Shared job lifecycle: create the job row, run the (instrumented) pipeline,
 * persist the resulting game, and mark the job SUCCEEDED/FAILED. Used by both
 * the blocking `generateDraftGame` mutation and the streaming `runGeneration`
 * subscription so the two never diverge.
 */
async function executeGeneration(
  adminId: string,
  params: z.infer<typeof generationParamsSchema>,
  title: string | undefined,
  hooks?: {
    onEvent?: GenerationProgressCallback;
    onJobCreated?: (jobId: string) => void;
  }
) {
  const startedAt = new Date();
  const eventLog: PersistedGenerationEvent[] = [];
  let persistLogChain = Promise.resolve();

  const job = await prisma.crosswordGenerationJob.create({
    data: {
      status: "RUNNING",
      title,
      topic: params.topic,
      width: params.width,
      height: params.height,
      minWordLength: params.minWordLength,
      maxWordLength: params.maxWordLength,
      params,
      metadata: {
        requestedTitle: title ?? null,
        params: compactGenerationParams(params),
      },
      eventLog: [],
      startedAt,
      createdBy: { connect: { id: adminId } },
    },
  });
  hooks?.onJobCreated?.(job.id);

  const persistEventLog = () => {
    const snapshot = eventLog.slice();
    persistLogChain = persistLogChain
      .then(() =>
        prisma.crosswordGenerationJob.update({
          where: { id: job.id },
          data: { eventLog: snapshot as any },
        })
      )
      .then(() => undefined)
      .catch(() => undefined);
  };

  const pushEvent = (event: PersistedGenerationEvent) => {
    eventLog.push(event);
    persistEventLog();
  };

  try {
    pushEvent({ type: "started", jobId: job.id, at: startedAt.getTime() });

    const generated = await generateCrosswordFromDictionary(prisma, params, (event) => {
      hooks?.onEvent?.(event);
      pushEvent({ ...event, at: Date.now() });
    });
    await persistLogChain;
    const resolvedTitle = title ?? generated.title;
    const completedAt = new Date();
    const durationMs = completedAt.getTime() - startedAt.getTime();

    const game = await prisma.$transaction(async (tx) => {
      const createdGame = await tx.game.create({
        data: {
          title: resolvedTitle,
          source: "GENERATED",
          published: false,
          questions: {
            createMany: {
              data: generated.questions.map((question) => ({
                number: question.number,
                answer: question.answer.toUpperCase(),
                questionText: question.questionText,
                rootX: question.rootX,
                rootY: question.rootY,
                direction: question.direction,
              })),
            },
          },
        },
        include: { questions: true },
      });

      await tx.crosswordGenerationJob.update({
        where: { id: job.id },
        data: {
          status: "SUCCEEDED",
          title: resolvedTitle,
          metrics: generated.metrics,
          metadata: {
            requestedTitle: title ?? null,
            resolvedTitle,
            params: compactGenerationParams(params),
            questionCount: generated.questions.length,
            resultGameId: createdGame.id,
          },
          eventLog: [
            ...eventLog,
            {
              type: "completed",
              jobId: job.id,
              gameId: createdGame.id,
              title: resolvedTitle,
              questionCount: generated.questions.length,
              metrics: generated.metrics,
              at: completedAt.getTime(),
            },
          ] as any,
          completedAt,
          durationMs,
          resultGame: { connect: { id: createdGame.id } },
        },
      });

      return createdGame;
    });

    return { jobId: job.id, game, metrics: generated.metrics };
  } catch (error) {
    await persistLogChain;
    const completedAt = new Date();
    const durationMs = completedAt.getTime() - startedAt.getTime();
    const message = error instanceof Error ? error.message : String(error);
    await prisma.crosswordGenerationJob.update({
      where: { id: job.id },
      data: {
        status: "FAILED",
        error: message,
        metadata: {
          requestedTitle: title ?? null,
          params: compactGenerationParams(params),
          failedAt: completedAt.toISOString(),
        },
        eventLog: [
          ...eventLog,
          { type: "failed", jobId: job.id, error: message, at: completedAt.getTime() },
        ] as any,
        completedAt,
        durationMs,
      },
    });
    throw error;
  }
}

export const generatorRouter = router({
  generateDraftGame: adminProcedure
    .input(
      z.object({
        title: z.string().trim().min(1).optional(),
        params: generationParamsSchema,
      })
    )
    .mutation(async ({ input, ctx }) => {
      validateGenerationParams(input.params);
      const { isUnlimited } = await checkQuota(ctx.user.id, ctx.user.role);
      const result = await executeGeneration(ctx.user.id, input.params, input.title);
      if (!isUnlimited) await incrementQuota(ctx.user.id);
      return result;
    }),

  // Streaming variant of generateDraftGame: same work, but emits granular
  // progress over a WebSocket subscription so the admin UI can show live
  // stage/progress/log events instead of a multi-minute spinner.
  runGeneration: adminProcedure
    .input(
      z.object({
        title: z.string().trim().min(1).optional(),
        params: generationParamsSchema,
      })
    )
    .subscription(async ({ input, ctx }) => {
      // Authorize + validate before returning the observable, so a rejection
      // surfaces as a client `onError` rather than a stream that emits then ends.
      validateGenerationParams(input.params);
      const { isUnlimited } = await checkQuota(ctx.user.id, ctx.user.role);

      return observable<GenerationEvent>((emit) => {
        let cancelled = false;
        const safeEmit = (event: GenerationEvent) => {
          if (!cancelled) emit.next(event);
        };

        void (async () => {
          let jobId: string | null = null;
          try {
            const result = await executeGeneration(ctx.user.id, input.params, input.title, {
              onJobCreated: (id) => {
                jobId = id;
                safeEmit({ type: "started", jobId: id, at: Date.now() });
              },
              onEvent: (event) => safeEmit({ ...event, at: Date.now() }),
            });

            if (!isUnlimited) await incrementQuota(ctx.user.id);

            safeEmit({
              type: "completed",
              jobId: result.jobId,
              gameId: result.game.id,
              title: result.game.title,
              questionCount: result.game.questions.length,
              metrics: result.metrics,
              at: Date.now(),
            });
          } catch (error) {
            safeEmit({
              type: "failed",
              jobId,
              error: error instanceof Error ? error.message : String(error),
              at: Date.now(),
            });
          } finally {
            if (!cancelled) emit.complete();
          }
        })();

        // The DB job keeps its own status; if the client disconnects we stop
        // emitting but let generation finish and persist on its own.
        return () => {
          cancelled = true;
        };
      });
    }),

  createJob: adminProcedure
    .input(
      z.object({
        params: generationParamsSchema,
      })
    )
    .mutation(async ({ input, ctx }) => {
      validateGenerationParams(input.params);

      return await prisma.crosswordGenerationJob.create({
        data: {
          status: "QUEUED",
          title: null,
          topic: input.params.topic,
          width: input.params.width,
          height: input.params.height,
          minWordLength: input.params.minWordLength,
          maxWordLength: input.params.maxWordLength,
          params: input.params,
          metadata: {
            params: compactGenerationParams(input.params),
          },
          eventLog: [],
          createdBy: { connect: { id: ctx.user.id } },
        },
      });
    }),

  listJobs: adminProcedure
    .input(
      z.object({
        take: z.number().int().min(1).max(100).default(25),
      })
    )
    .query(async ({ input }) => {
      return await prisma.crosswordGenerationJob.findMany({
        take: input.take,
        orderBy: { createdAt: "desc" },
        include: {
          createdBy: { select: { id: true, email: true, name: true } },
          resultGame: { select: { id: true, title: true, published: true, source: true } },
        },
      });
    }),

  getJob: adminProcedure
    .input(
      z.object({
        id: z.string().uuid(),
      })
    )
    .query(async ({ input }) => {
      return await prisma.crosswordGenerationJob.findUnique({
        where: { id: input.id },
        include: {
          createdBy: { select: { id: true, email: true, name: true } },
          resultGame: {
            include: {
              questions: { orderBy: [{ number: "asc" }, { direction: "asc" }] },
            },
          },
        },
      });
    }),

  saveDraftGame: adminProcedure
    .input(
      z.object({
        jobId: z.string().uuid(),
        title: z.string().trim().min(1),
        metrics: z.record(z.unknown()).optional(),
        questions: z.array(generatedQuestionSchema).min(1),
      })
    )
    .mutation(async ({ input }) => {
      const job = await prisma.crosswordGenerationJob.findUnique({
        where: { id: input.jobId },
        select: { id: true, resultGameId: true },
      });

      if (!job) {
        throw new TRPCError({ code: "NOT_FOUND", message: "Generation job was not found." });
      }

      if (job.resultGameId) {
        throw new TRPCError({
          code: "CONFLICT",
          message: "Generation job already has a result game.",
        });
      }

      return await prisma.$transaction(async (tx) => {
        const game = await tx.game.create({
          data: {
            title: input.title,
            source: "GENERATED",
            published: false,
            questions: {
              createMany: {
                data: input.questions.map((question) => ({
                  number: question.number,
                  answer: question.answer.toUpperCase(),
                  questionText: question.questionText,
                  rootX: question.rootX,
                  rootY: question.rootY,
                  direction: question.direction,
                })),
              },
            },
          },
          include: { questions: true },
        });

        await tx.crosswordGenerationJob.update({
          where: { id: input.jobId },
          data: {
            status: "SUCCEEDED",
            title: input.title,
            metrics: input.metrics ?? {},
            metadata: {
              title: input.title,
              questionCount: input.questions.length,
              savedManually: true,
            },
            eventLog: [
              {
                type: "completed",
                jobId: input.jobId,
                gameId: game.id,
                title: input.title,
                questionCount: input.questions.length,
                metrics: input.metrics ?? {},
                at: Date.now(),
              },
            ],
            completedAt: new Date(),
            resultGame: { connect: { id: game.id } },
          },
        });

        return game;
      });
    }),

  markFailed: adminProcedure
    .input(
      z.object({
        jobId: z.string().uuid(),
        error: z.string().trim().min(1),
        metrics: z.record(z.unknown()).optional(),
      })
    )
    .mutation(async ({ input }) => {
      return await prisma.crosswordGenerationJob.update({
        where: { id: input.jobId },
        data: {
          status: "FAILED",
          error: input.error,
          metrics: input.metrics ?? undefined,
          eventLog: [
            {
              type: "failed",
              jobId: input.jobId,
              error: input.error,
              at: Date.now(),
            },
          ],
          completedAt: new Date(),
        },
      });
    }),

  publishGeneratedGame: adminProcedure
    .input(
      z.object({
        gameId: z.string().uuid(),
      })
    )
    .mutation(async ({ input }) => {
      const game = await prisma.game.findUnique({
        where: { id: input.gameId },
        select: { id: true, source: true },
      });

      if (!game) {
        throw new TRPCError({ code: "NOT_FOUND", message: "Game was not found." });
      }

      if (game.source !== "GENERATED") {
        throw new TRPCError({
          code: "BAD_REQUEST",
          message: "Only generated games can be published through this route.",
        });
      }

      return await prisma.game.update({
        where: { id: input.gameId },
        data: { published: true },
      });
    }),
});
