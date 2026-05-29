import { defineStore } from 'pinia';

export const useSubscriptionStore = defineStore('subscription', () => {
  const { $client } = useNuxtApp();

  const isPro = ref(false);
  const quotaUsed = ref(0);
  const quotaLimit = ref<number | null>(5);
  const quotaResetsAt = ref<string | null>(null);
  const subscriptionStatus = ref<string | null>(null);
  const loading = ref(false);

  const quotaRemaining = computed(() => {
    if (quotaLimit.value === null) return Infinity;
    return Math.max(0, quotaLimit.value - quotaUsed.value);
  });

  const isQuotaExhausted = computed(() => {
    if (isPro.value) return false;
    return quotaRemaining.value <= 0;
  });

  async function fetchStatus() {
    loading.value = true;
    try {
      const status = await $client.subscription.getStatus.query();
      isPro.value = status.isPro;
      quotaUsed.value = status.quota.used;
      quotaLimit.value = status.quota.limit;
      quotaResetsAt.value = status.quota.resetsAt;
      subscriptionStatus.value = status.subscription?.status ?? null;
    } catch {
      // Silently fail — user may not be authenticated
    } finally {
      loading.value = false;
    }
  }

  async function openCheckout() {
    try {
      const { checkoutUrl } = await $fetch('/api/checkout', { method: 'POST' });
      if (checkoutUrl && window.LemonSqueezy) {
        window.LemonSqueezy.Url.Open(checkoutUrl);
      } else if (checkoutUrl) {
        window.open(checkoutUrl, '_blank');
      }
    } catch (error) {
      console.error('Failed to create checkout:', error);
    }
  }

  return {
    isPro,
    quotaUsed,
    quotaLimit,
    quotaRemaining,
    quotaResetsAt,
    isQuotaExhausted,
    subscriptionStatus,
    loading,
    fetchStatus,
    openCheckout,
  };
});
