import crypto from 'crypto';
import { SubscriptionStatus } from '@prisma/client';

interface LemonSqueezyWebhookPayload {
  meta: {
    event_name: string;
    custom_data: {
      user_id: string;
    };
  };
  data: {
    type: string;
    id: string;
    attributes: {
      status: string;
      customer_id: number;
      renews_at: string | null;
      ends_at: string | null;
    };
  };
}

const eventStatusMap: Record<string, SubscriptionStatus> = {
  subscription_created: SubscriptionStatus.ACTIVE,
  subscription_expired: SubscriptionStatus.EXPIRED,
  subscription_payment_failed: SubscriptionStatus.PAST_DUE,
  subscription_payment_recovered: SubscriptionStatus.ACTIVE,
  subscription_payment_success: SubscriptionStatus.ACTIVE,
};

function resolveStatus(
  eventName: string,
  attributes: LemonSqueezyWebhookPayload['data']['attributes']
): SubscriptionStatus {
  if (eventName === 'subscription_updated') {
    const statusMap: Record<string, SubscriptionStatus> = {
      active: SubscriptionStatus.ACTIVE,
      past_due: SubscriptionStatus.PAST_DUE,
      cancelled: SubscriptionStatus.CANCELLED,
      expired: SubscriptionStatus.EXPIRED,
    };
    return statusMap[attributes.status] ?? SubscriptionStatus.ACTIVE;
  }

  if (eventName === 'subscription_cancelled') {
    return SubscriptionStatus.CANCELLED;
  }

  return eventStatusMap[eventName] ?? SubscriptionStatus.ACTIVE;
}

export default defineEventHandler(async (event) => {
  const body = await readRawBody(event);
  if (!body) {
    throw createError({ statusCode: 400, statusMessage: 'Missing request body' });
  }

  const signature = getHeader(event, 'x-signature');
  if (!signature) {
    throw createError({ statusCode: 401, statusMessage: 'Missing signature' });
  }

  const { lemonSqueezy } = useRuntimeConfig();
  const hmac = crypto
    .createHmac('sha256', lemonSqueezy.webhookSecret)
    .update(body)
    .digest('hex');

  if (!crypto.timingSafeEqual(Buffer.from(hmac), Buffer.from(signature))) {
    throw createError({ statusCode: 401, statusMessage: 'Invalid signature' });
  }

  const payload: LemonSqueezyWebhookPayload = JSON.parse(body);
  const { meta, data } = payload;
  const userId = meta.custom_data.user_id;
  const lemonSqueezyId = data.id;
  const { attributes } = data;

  const status = resolveStatus(meta.event_name, attributes);
  const currentPeriodEnd = attributes.ends_at
    ? new Date(attributes.ends_at)
    : attributes.renews_at
      ? new Date(attributes.renews_at)
      : null;

  const prisma = event.context.prisma;

  await prisma.subscription.upsert({
    where: { lemonSqueezyId },
    create: {
      userId,
      lemonSqueezyId,
      lemonSqueezyCustomerId: String(attributes.customer_id),
      status,
      currentPeriodEnd,
    },
    update: {
      status,
      currentPeriodEnd,
      lemonSqueezyCustomerId: String(attributes.customer_id),
    },
  });

  return { ok: true };
});
