import { getServerSession } from '#auth';
import { SubscriptionStatus } from '@prisma/client';

const FREE_LIMIT = 5;

export default defineEventHandler(async (event) => {
  const session = await getServerSession(event);
  if (!session?.user?.email) {
    throw createError({ statusCode: 401, statusMessage: 'Unauthorized' });
  }

  const prisma = event.context.prisma;
  const user = await prisma.user.findUnique({
    where: { email: session.user.email },
    select: {
      id: true,
      subscription: true,
      generationQuota: true,
    },
  });

  if (!user) {
    throw createError({ statusCode: 404, statusMessage: 'User not found' });
  }

  const subscription = user.subscription;
  const isPro =
    !!subscription &&
    (subscription.status === SubscriptionStatus.ACTIVE ||
      subscription.status === SubscriptionStatus.CANCELLED);

  // Quota with lazy monthly reset
  const quota = user.generationQuota;
  let used = 0;
  let resetsAt: Date;

  if (quota) {
    const now = new Date();
    const resetDate = new Date(quota.monthResetAt);
    const isCurrentMonth =
      resetDate.getUTCFullYear() === now.getUTCFullYear() &&
      resetDate.getUTCMonth() === now.getUTCMonth();

    used = isCurrentMonth ? quota.usedThisMonth : 0;

    // Next reset is the 1st of the next month
    resetsAt = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1));
  } else {
    const now = new Date();
    resetsAt = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1));
  }

  return {
    isPro,
    subscription: subscription
      ? {
          status: subscription.status,
          currentPeriodEnd: subscription.currentPeriodEnd?.toISOString() ?? null,
          lemonSqueezyId: subscription.lemonSqueezyId,
        }
      : null,
    quota: {
      used,
      limit: isPro ? Infinity : FREE_LIMIT,
      resetsAt: resetsAt.toISOString(),
    },
  };
});
