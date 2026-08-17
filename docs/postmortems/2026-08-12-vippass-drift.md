# Postmortem: GH-#60 — Admin View User Panel: missing `vipPass` column

- **Date detected:** 2026-08-10 (issue opened)
- **Date fixed (source):** 2026-08-12 — fix commit `d231f06`
- **Date fixed (prod runtime):** 2026-08-12 — first release carrying the migration, `v0.1.40` (release commit `524a7b7`, release PR [#64](https://github.com/olivecasazza/definitely-not-crosswords/pull/64))
- **Date closed (GitHub):** this PR — GH-#60 stayed OPEN between 2026-08-10 and today because no commit or PR body contained `Closes #60`. The runtime was repaired by `d231f06`; the issue metadata was not.
- **Severity:** P0 — broke login + signup + every Pro-gated surface.

## What users saw

```
{"code":-32600,"data":{"code":"BAD_REQUEST","httpStatus":400},
 "message":"error returned from database: column \"vipPass\" does not exist"}
```

The admin panel surfaced it first because the admin `view_user` query
selects every column on `User`, including `vipPass`. Any other surface
that did the same (or joined on it) would have failed the same way.

## Blast radius on prod (verified against the deployed schema)

- `user.register` fails outright — nobody can sign up.
- `subscription.getStatus` fails for every logged-in user — all Pro
  gating is broken.
- Every `discount.*` procedure and `/admin/discounts` is down.
- Generator quota, team Pro checks, `user.createForAdmin` all fail.
- Checkout without a code still worked, because `PRO_CHECKOUT_DISCOUNT_CODE`
  is unset on prod.

Staging has all the objects (it was built fresh against the post-handover
sqlx set), which is why this bug only manifested on prod.

## Root cause

`adopt_existing_schema_if_needed()`, written for the one-time Prisma→sqlx
handover on 2026-06-30, baselined **every** migration it knew about as
applied whenever `_sqlx_migrations` was absent but a `User` table existed.
It did not verify that any of those migrations' objects existed.

The pre-sqlx production database had only 7 Prisma migrations applied
(`_prisma_migrations` ends at `20260529082000`). By handover the `sqlx`
directory held 9. The extra two were silently swallowed:

- `20260529083000_add_user_vip_pass` → `User."vipPass"`
- `20260529090000_add_discounts` → `Discount` table + its two enums

The smoking gun in the data: the first 9 rows of prod's `_sqlx_migrations`
share one 6 ms timestamp with `execution_time = 0`, which is the baseline
INSERT. Everything from `20260703000000` onward carries a real execution
time, meaning it actually ran.

## Why sqlx can never replay those two orphaned migrations

They are recorded as `success = true` with valid checksums. Editing them
would break checksum validation on every healthy database. So the only
honest fix is a new forward migration that re-applies their DDL
idempotently — that is what `d231f06` does:

- `ALTER TABLE "User" ADD COLUMN IF NOT EXISTS "vipPass" BOOLEAN NOT NULL DEFAULT false;`
- `CREATE TYPE IF NOT EXISTS`-style guards for `DiscountAmountType` and `DiscountDuration` (enums can't `IF NOT EXISTS`, so guarded via `pg_type` lookup)
- `CREATE TABLE IF NOT EXISTS "Discount" (...)` with the original column types, defaults, nullability, constraint name
- `CREATE UNIQUE INDEX IF NOT EXISTS "Discount_code_key"` and friends

Result: clean no-op on staging / local dev / fresh installs, real repair
on prod. Once applied, the prod schema is indistinguishable from one
built from scratch.

## Why the adoption path was deleted, not fixed

Both live databases now carry real histories, so the function was
unreachable-but-armed. Leaving it in was a bug-in-waiting: any
resurrected pre-sqlx database would re-trigger this exact corruption
silently. Deleting it means a future pre-sqlx database will fail loudly
on `CREATE TABLE … already exists` instead of being baselined blind.

## Guardrail shipped in `f638427`

Before this incident, the first place any migration ran was a real
database. Nothing tested them; nothing detected drift. `f638427` adds a
GitHub Actions job that, on every PR:

1. Spins up `postgres:16` in a service container.
2. Applies every migration to an empty database.
3. Asserts the applied count equals the number of migration files.
4. Asserts no row is recorded with `execution_time = 0` — the exact
   signature of a migration marked applied without running, which is
   what corrupted prod.
5. Re-runs and asserts a clean no-op (matters because `migrate` runs on
   every pod start, so a non-idempotent migration breaks the next rollout
   rather than the one that introduced it).
6. Asserts the end-state objects exist, since a migration can apply
   successfully and still leave the schema wrong.

It runs in `build-and-push`, so an image with broken migrations is never
published.

## What we learned

1. **"Marked as applied" is not "applied."** Adoption shims that touch a
   migrations table without verifying objects are a sharp tool. Either
   point-in-time stamp the adoption and then delete it, or guard each
   baseline by checking the objects it claims to have created.
2. **Release-drift detection belongs in CI, not in `initContainers`.**
   The migrate init only sees its own pod; it cannot detect that the
   schema is already broken before it ran.
3. **Always link fixes to the issues they close.** `d231f06` fixed prod
   but never carried `Closes #60`. The runtime was repaired six days
   before the issue metadata caught up. Future fix commits should
   include the GitHub issue reference in the body to make
   `Closes` automation work, and a release deploy is the right time to
   sweep the issue queue for fixes that shipped without closing their
   tickets.

## Artifacts

- Issue: <https://github.com/olivecasazza/definitely-not-crosswords/issues/60>
- Fix: <https://github.com/olivecasazza/definitely-not-crosswords/commit/d231f06>
- CI guard: <https://github.com/olivecasazza/definitely-not-crosswords/commit/f638427>
- Release: <https://github.com/olivecasazza/definitely-not-crosswords/releases/tag/v0.1.40>
