import { getServerSession } from '#auth';
import { lemonSqueezySetup, createCheckout } from '@lemonsqueezy/lemonsqueezy.js';

export default defineEventHandler(async (event) => {
  const session = await getServerSession(event);
  if (!session?.user?.email) {
    throw createError({ statusCode: 401, statusMessage: 'Unauthorized' });
  }

  const prisma = event.context.prisma;
  const user = await prisma.user.findUnique({
    where: { email: session.user.email },
    select: { id: true },
  });

  if (!user) {
    throw createError({ statusCode: 404, statusMessage: 'User not found' });
  }

  const { lemonSqueezy } = useRuntimeConfig();

  lemonSqueezySetup({ apiKey: lemonSqueezy.apiKey });

  const { data, error } = await createCheckout(lemonSqueezy.storeId, lemonSqueezy.variantId, {
    checkoutData: {
      custom: { user_id: user.id },
    },
  });

  if (error) {
    throw createError({ statusCode: 500, statusMessage: 'Failed to create checkout' });
  }

  return { checkoutUrl: data!.data.attributes.url };
});
