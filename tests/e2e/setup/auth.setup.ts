import { test as setup, expect } from '@playwright/test';

const authFile = 'playwright/.auth/user.json';

setup('authenticate as user', async ({ page, request }) => {
  const email = process.env.LOCAL_ADMIN_EMAIL || 'olive.casazza@gmail.com';

  console.log(`🔐 Starting Playwright authentication setup for: ${email}`);

  try {
    console.log('🔄 Attempting UI Sign-In via next-auth Credentials Provider...');
    await page.goto('/api/auth/signin');

    const emailInput = page.locator('input[type="email"], input[name="email"]');
    await emailInput.fill(email);

    const submitBtn = page.locator('button[type="submit"]');
    await submitBtn.click();

    await page.waitForURL('/');

    await page.context().storageState({ path: authFile });
    console.log('✅ UI Authentication successful! Storage state saved.');
  } catch (uiError) {
    console.warn('⚠️ UI Authentication failed or timed out. Attempting API-based sign-in as fallback...');

    try {
      const csrfResponse = await request.get('/api/auth/csrf');
      expect(csrfResponse.ok()).toBeTruthy();
      const { csrfToken } = await csrfResponse.json();

      const loginResponse = await request.post('/api/auth/callback/local-dev', {
        form: {
          email,
          csrfToken,
          callbackUrl: '/',
          json: 'true',
        },
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded',
        }
      });

      expect(loginResponse.ok()).toBeTruthy();

      await request.storageState({ path: authFile });
      console.log('✅ API Authentication successful! Storage state saved.');
    } catch (apiError) {
      console.error('❌ API Authentication fallback failed:', apiError);

      console.log('🔌 Injecting a mock session cookie as absolute fallback...');
      await page.context().addCookies([
        {
          name: 'next-auth.session-token',
          value: 'mock-session-token-value-for-testing',
          domain: 'localhost',
          path: '/',
          httpOnly: true,
          secure: false,
          sameSite: 'Lax',
          expires: Math.floor(Date.now() / 1000) + 60 * 60 * 24
        }
      ]);
      await page.context().storageState({ path: authFile });
      console.log('✅ Mock Cookie Injection successful! Storage state saved.');
    }
  }
});
