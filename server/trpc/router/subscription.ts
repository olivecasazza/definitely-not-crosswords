import { prisma } from '.';
import { protectedProcedure, router } from '../trpc';
import { SubscriptionStatus } from '@prisma/client';

const FREE_LIMIT = 5;

export const subscriptionRouter = router({
  getStatus: protectedProcedure.query(async ({ ctx }) => {
    const user = await prisma.user.findUnique({
      where: { id: ctx.user.id },
      select: {
        subscription: true,
        generationQuota: true,
      },
    });

    const subscription = user?.subscription;
    const isPro =
      !!subscription &&
      (subscription.status === SubscriptionStatus.ACTIVE ||
        subscription.status === SubscriptionStatus.CANCELLED);

    const quota = user?.generationQuota;
    const now = new Date();
    let used = 0;

    if (quota) {
      const resetDate = new Date(quota.monthResetAt);
      const isCurrentMonth =
        resetDate.getUTCFullYear() === now.getUTCFullYear() &&
        resetDate.getUTCMonth() === now.getUTCMonth();
      used = isCurrentMonth ? quota.usedThisMonth : 0;
    }

    return {
      isPro,
      subscription: subscription
        ? {
            status: subscription.status,
            currentPeriodEnd: subscription.currentPeriodEnd?.toISOString() ?? null,
          }
        : null,
      quota: {
        used,
        limit: isPro ? null : FREE_LIMIT, // null = unlimited
        resetsAt: new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1)).toISOString(),
      },
    };
  }),
});
