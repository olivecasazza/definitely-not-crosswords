import { test, expect, type Page } from "@playwright/test";

// Billing canary: prove a Pro click reaches a real Lemon Squeezy checkout.
//
// `GET /api/config` reporting `features.proCheckout: true` proves the three
// LEMONSQUEEZY_* credentials are PRESENT in the pod — `ls_configured()` reads
// the process env (client/backend/server/src/checkout.rs). It does not prove
// the API key is VALID: an expired or revoked key satisfies the flag and only
// fails at the first outbound `POST https://api.lemonsqueezy.com/v1/checkouts`,
// which surfaces as a 500 on our own `/api/checkout`.
//
// Nothing cheaper can check it. `POST /api/checkout` authenticates the session
// cookie before it reads a single credential, so an unauthenticated probe
// returns 401 whether or not the key works — a false green. So this spec does
// the real thing: sign in, click the priced control the flag is supposed to
// gate, and assert the browser lands on a lemonsqueezy.com checkout.
//
// Scope discipline — it CREATES a checkout (a draft) and STOPS THERE. Never
// fill in or submit the Lemon Squeezy form: staging runs the ~90%-off beta code
// (PRO_CHECKOUT_DISCOUNT_CODE), so a completed order is a real charge AND fires
// the live /api/webhooks/lemonsqueezy handler against the test account.
//
// Needs the staging test account; self-skips without it, exactly like
// demo.spec.ts / verify-layout.spec.ts / fresh-start.spec.ts.

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;
test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

// Sign-in + a cold Lemon Squeezy page load (which may sit behind a bot
// challenge) can outrun the config's 60s.
test.setTimeout(180_000);

/** The priced CTA. Exact copy from components/pro_upgrade.rs. */
const CTA = /upgrade to pro/i;

/** Lemon Squeezy serves every checkout from its own domain, never ours. */
const LS_HOST = /\.lemonsqueezy\.com$/i;

/** The two ids the staging ExternalSecret carries (README "Plans & pricing"). */
const STORE_ID = "390247";
const VARIANT_ID = "1718877";

/**
 * A checkout URL, not just a Lemon Squeezy host: `/checkout/buy/<uuid>` today,
 * `/embed/checkout/...` for the embedded flavour. Anchored on the path only so
 * a host change doesn't silently turn the assertion into a no-op. We assert
 * "some checkout path" rather than one exact shape because the path is Lemon
 * Squeezy's to change, and a canary that reds on vendor cosmetics is worse than
 * useless. The identity assertions are the status code (a 200 came back from LS
 * for THIS store + variant) and the host.
 */
const LS_CHECKOUT_URL = /lemonsqueezy\.com\/[^/]*checkout/i;

/** Sign in via the login form directly (no nav dependency). */
async function signIn(page: Page, email: string, password: string) {
  await page.goto("/auth/login");
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole("button", { name: /^sign in/i }).click();
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
}

test("a Pro click starts a real Lemon Squeezy checkout (no purchase)", async ({
  page,
  baseURL,
}) => {
  // ── 1. What the deploy claims, before touching the UI ──────────────────────
  // Read the flag first so a missing flag is a diagnosis, not a mystery
  // timeout waiting on a button that will never render.
  const cfgRes = await page.request.get("/api/config");
  expect(cfgRes.status(), "GET /api/config").toBe(200);
  const proCheckout = (await cfgRes.json())?.features?.proCheckout;
  const staging = !process.env.E2E_BASE_URL || /staging/i.test(baseURL ?? "");
  if (proCheckout !== true) {
    if (staging) {
      // Staging is where billing was turned on (DEF-167). A missing flag there
      // means the LEMONSQUEEZY_* injection is gone — the exact "frontend and
      // deploy disagree" state this canary exists to catch. Skip is a lie here.
      throw new Error(
        `staging ${baseURL}/api/config reports proCheckout=${JSON.stringify(proCheckout)} — ` +
          `the LEMONSQUEEZY_* injection is missing, so no purchase control can render (DEF-167)`,
      );
    }
    test.skip(
      true,
      `billing is off on this deployment (proCheckout=${proCheckout}) — no purchase control is rendered, by design`,
    );
  }

  // ── 2. The priced CTA, and it is VISIBLE ───────────────────────────────────
  await signIn(page, EMAIL!, PASSWORD!);
  await expect(
    page.locator('header a.navlink[href="/profile"]'),
    "signed-in header has a Profile link",
  ).toBeVisible({ timeout: 20_000 });
  await page.locator('header a.navlink[href="/profile"]').click();
  await expect(page).toHaveURL(/\/profile/, { timeout: 15_000 });
  await expect(page.getByText(/current plan/i).first()).toBeVisible({ timeout: 20_000 });

  const cta = page.getByRole("button", { name: CTA });
  // The "active" chip means this account is already Pro: there is no purchase
  // control to click, and buying a second subscription would be a real charge.
  const proChip = page.getByText(/^active$/i);
  await expect(proChip.or(cta).first()).toBeVisible({ timeout: 20_000 });
  if (await proChip.count()) {
    test.skip(true, "the e2e account is already Pro — there is no upgrade control to click");
  }

  // smoke.spec.ts only checks that `proCheckout` is a boolean, which says
  // nothing about any control. THIS is the assertion that ties the flag to a
  // rendered, priced, clickable button: flag true + no visible button means
  // the wasm bundle and the deploy disagree.
  await expect(cta, "the priced Pro CTA must be rendered, not just flagged").toBeVisible();
  await expect(cta, "the CTA must carry the price").toContainText("$10/year");
  await expect(cta, "the CTA must be enabled").toBeEnabled();

  // ── 3. Click it and watch where the browser actually goes ──────────────────
  let checkoutStatus: number | undefined;
  page.on("response", (r) => {
    if (r.request().method() === "POST" && new URL(r.url()).pathname === "/api/checkout") {
      checkoutStatus = r.status();
    }
  });

  await cta.click();

  // Two possible outcomes, and the bad one has to be loud:
  //   • 200 -> the handler assigns location.href and we leave for LS.
  //   • 500 -> the frontend never navigates; it renders its "unavailable"
  //     banner (components/pro_upgrade.rs deliberately hides the raw status).
  // Racing the two turns that 500 into a named failure instead of a bare
  // waitForURL timeout with nothing to point at.
  const outcome = await Promise.race([
    page.waitForURL(LS_CHECKOUT_URL, { timeout: 90_000 }).then(() => "checkout" as const),
    page
      .getByText(/checkout is unavailable/i)
      .waitFor({ state: "visible", timeout: 90_000 })
      .then(() => "error-banner" as const),
  ]);
  expect(
    outcome,
    `POST /api/checkout returned ${checkoutStatus} and the page never reached Lemon Squeezy — ` +
      `a 500 here means the LEMONSQUEEZY_* credentials in the staging pod are present but not valid`,
  ).toBe("checkout");
  expect(checkoutStatus, "the click must have reached POST /api/checkout").toBe(200);

  // ── 4. A real checkout, for the real store ─────────────────────────────────
  // A 200 above is the store/variant proof: our server only builds a checkout
  // from a successful LS API call naming LEMONSQUEEZY_STORE_ID (390247) and
  // LEMONSQUEEZY_VARIANT_ID (1718877) — an unknown or wrong id comes back 422
  // and the handler returns 500, which is the failure asserted against above.
  // The host + path below tie the *browser* to that checkout rather than to
  // anything we served.
  const landed = new URL(page.url());
  expect(landed.host, `landed on ${landed.origin} — not a Lemon Squeezy host`).toMatch(LS_HOST);
  expect(landed.pathname, "a checkout URL, not a store homepage").not.toBe("/");

  // `?__checkout=` is a session token for a half-created order: report the host
  // and path only, never the query string, in the log and the test report.
  const redacted = `${landed.origin}${landed.pathname}`;
  console.log(`pro-checkout: landed on ${redacted} (query redacted)`);
  test.info().annotations.push({
    type: "lemon-squeezy-checkout",
    description: redacted,
  });
  // The numeric store/variant are NOT in LS's checkout URL (it carries the
  // variant's UUID and the store's subdomain) — but the checkout page embeds
  // LS's own payload, which does carry them. That turns "some checkout on LS"
  // into "the checkout for store 390247, variant 1718877": the two literals
  // below are the ids this repo's README and the staging ExternalSecret name,
  // and a checkout created against a different store cannot render them.
  // (First measured on run 36378794830: both present, twice each.)
  const html = await page.content();
  expect(
    html.includes(STORE_ID),
    `the Lemon Squeezy checkout page does not mention store ${STORE_ID} — the landed checkout is not ours`,
  ).toBe(true);
  expect(
    html.includes(VARIANT_ID),
    `the Lemon Squeezy checkout page does not mention variant ${VARIANT_ID} — the landed checkout is not the Pro plan`,
  ).toBe(true);

  // ── 5. STOP. No purchase. ─────────────────────────────────────────────────
  // The end state IS the assertion: still sitting on Lemon Squeezy's checkout
  // page, having never submitted anything. Completing it would buy a real
  // ~$1 subscription on staging and hit the live webhook handler.
  await expect(page).toHaveURL(LS_CHECKOUT_URL);
});
