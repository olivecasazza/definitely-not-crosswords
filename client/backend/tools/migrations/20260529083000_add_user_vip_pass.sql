-- Manual "VIP pass" override that grants Pro access without a paid subscription.
ALTER TABLE "User" ADD COLUMN "vipPass" BOOLEAN NOT NULL DEFAULT false;
