-- Ordering guard for Lemon Squeezy webhooks. Stores the timestamp of the last
-- webhook event applied to this row so stale / replayed / out-of-order events
-- (e.g. a retried subscription_payment_success arriving after subscription_expired)
-- cannot overwrite a newer terminal status and hand Pro back.
ALTER TABLE "Subscription" ADD COLUMN "lastEventAt" TIMESTAMP(3);
