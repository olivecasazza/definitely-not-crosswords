import { test, expect } from "@playwright/test";

const ADMIN_EMAIL = process.env.E2E_ADMIN_EMAIL;
const ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD;
const E2E_BASE_URL = process.env.E2E_BASE_URL ?? "https://crosswords-staging.casazza.io";

test.describe("RBAC — job creation", () => {
  test("POST /api/jobs returns 403 when unauthenticated", async ({
    request,
  }) => {
    const res = await request.post(`${E2E_BASE_URL}/api/jobs`, {
      data: {
        params: { topic: "test", width: 11, height: 11 },
      },
    });
    expect(res.status()).toBe(403);
  });

  test("POST /api/jobs returns 403 when session lacks job:create", async ({
    request,
  }) => {
    const res = await request.post(`${E2E_BASE_URL}/api/jobs`, {
      data: {
        params: { topic: "test", width: 11, height: 11 },
      },
      headers: {
        Authorization: "Bearer invalid-token-without-job-create-capability",
      },
    });
    expect(res.status()).toBe(403);
  });

  test("POST /api/jobs returns 201 for authenticated user with job:create", async ({
    page,
    request,
  }) => {
    const email = ADMIN_EMAIL;
    const password = ADMIN_PASSWORD;
    test.skip(
      !email || !password,
      "E2E_ADMIN_EMAIL / E2E_ADMIN_PASSWORD not set",
    );

    await page.goto(`${E2E_BASE_URL}/auth/login`);
    await page.locator('input[type="email"]').fill(email!);
    await page.locator('input[type="password"]').fill(password!);
    await page.getByRole("button", { name: /^sign in/i }).click();
    await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });

    const jobsRes = await page.request.post(`${E2E_BASE_URL}/api/jobs`, {
      data: {
        params: { topic: "rbac-test-topic", width: 11, height: 11 },
      },
    });

    expect(jobsRes.status()).toBe(201);
    const body = await jobsRes.json();
    expect(body).toHaveProperty("jobId");
  });
});
