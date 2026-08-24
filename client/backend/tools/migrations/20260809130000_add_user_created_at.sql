-- Signup timestamp for the admin "Joined" column and newest-users feed.
-- Nullable with a default: rows created after this migration get a real
-- timestamp; pre-existing users stay NULL (rendered as "—") rather than all
-- claiming to have joined on migration day.
ALTER TABLE "User" ADD COLUMN "createdAt" TIMESTAMP(3) DEFAULT CURRENT_TIMESTAMP;
