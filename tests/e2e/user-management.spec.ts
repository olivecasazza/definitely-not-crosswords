import { test as base, expect } from '@nuxt/test-utils/playwright';

// Extend Playwright's page fixture to support the gotoPath method required by instructions
const test = base.extend<{
  page: any;
}>({
  page: async ({ page, baseURL }, use) => {
    (page as any).gotoPath = (path: string, options?: any) => {
      const targetUrl = baseURL ? new URL(path, baseURL).toString() : path;
      return page.goto(targetUrl, options);
    };
    await use(page);
  }
});

test.describe('User Management E2E Flow', () => {
  test.beforeEach(async ({ page, baseURL }) => {
    page.on('console', msg => console.log('💻 BROWSER CONSOLE:', msg.text()));
    page.on('pageerror', err => console.log('❌ BROWSER ERROR:', err.message));

    // Intercept any next-auth requests targeting localhost:3000 and rewrite to baseURL
    await page.route('**/api/auth/**', async (route) => {
      const url = route.request().url();
      if (url.includes('http://localhost:3000')) {
        const target = url.replace('http://localhost:3000', baseURL || '');
        console.log(`🔀 Rewriting Auth URL: ${url} -> ${target}`);
        await route.continue({ url: target });
      } else {
        await route.continue();
      }
    });
  });

  test.describe('Unauthenticated Actions (Signup & Verification & Signin)', () => {
    // Clear storageState for this block so we start completely signed-out
    test.use({ storageState: { cookies: [], origins: [] } });

    test('should register, verify email, and sign in successfully', async ({ page }) => {
      const testEmail = `e2e-user-${Math.random().toString(36).substring(7)}@test.com`;
      const testName = 'E2E Test User';

      // 1. Register a new user
      console.log('📝 Navigating to Signup Page...');
      await page.gotoPath('/auth/signup');
      await expect(page.locator('h1')).toHaveText('Create Account');

      await page.locator('#name').fill(testName);
      await page.locator('#email').fill(testEmail);
      await page.locator('button[type="submit"]').click();

      // Verify signup success
      console.log('✅ Signup submitted. Waiting for verification link...');
      const verificationLink = page.locator('#verification-link');
      await expect(verificationLink).toBeVisible({ timeout: 10000 });

      // 2. Email Verification Flow
      console.log('🔗 Clicking email verification link...');
      await verificationLink.click();

      // Wait for success status
      const successTitle = page.locator('#verification-success-title');
      await expect(successTitle).toBeVisible({ timeout: 10000 });
      await expect(successTitle).toHaveText('Email Verified!');

      // 3. User Sign-In Flow
      console.log('🔑 Navigating to Sign-In Page...');
      await page.gotoPath('/api/auth/signin');

      const emailInput = page.locator('input[type="email"], input[name="email"]');
      await emailInput.fill(testEmail);
      await page.locator('button:has-text("Sign in with Local Dev")').click();

      // Verify redirect to homepage after login
      console.log('🏠 Verifying redirect to home page...');
      await page.waitForURL('/', { timeout: 15000 });
      await expect(page).toHaveURL(/.*\//);

      // Verify the sign-out button is visible in the header
      const signOutBtn = page.locator('button:has-text("Sign Out")');
      await expect(signOutBtn).toBeVisible();
    });
  });

  test.describe('Authenticated Actions (Profile Update & Sign-Out & Account Deletion)', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      // Smart next-auth session interceptor with URL rewrite fallback
      await page.route('**/api/auth/session', async (route) => {
        const requestUrl = route.request().url();
        const targetUrl = requestUrl.includes('localhost:3000') 
          ? requestUrl.replace('http://localhost:3000', baseURL || '') 
          : requestUrl;

        const cookies = await page.context().cookies();
        const sessionCookie = cookies.find(c => c.name.includes('session-token'));
        if (sessionCookie && sessionCookie.value === 'mock-session-token-value-for-testing') {
          await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
              user: {
                name: 'Olive Casazza',
                email: 'olive.casazza@gmail.com',
                role: 'ADMIN'
              },
              expires: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString()
            })
          });
        } else {
          await route.continue({ url: targetUrl });
        }
      });

      // Inject active session cookie dynamically for both localhost and the specific test server hostname
      const host = baseURL ? new URL(baseURL).hostname : '127.0.0.1';
      await page.context().addCookies([
        {
          name: 'next-auth.session-token',
          value: 'mock-session-token-value-for-testing',
          domain: host,
          path: '/',
          httpOnly: true,
          secure: false,
          sameSite: 'Lax',
          expires: Math.floor(Date.now() / 1000) + 60 * 60 * 24
        },
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
    });
    
    test('should update profile name on the profile page', async ({ page }) => {
      console.log('👤 Navigating to Profile Page...');
      await page.gotoPath('/profile');
      
      // Wait deterministically for Vue hydration to complete
      await page.waitForFunction(() => (window as any).__nuxt_hydrated === true, { timeout: 15000 });

      await expect(page.locator('h3:has-text("Profile Settings")')).toBeVisible();

      // Wait for Vue hydration by checking the default mock name is populated
      const nameInput = page.locator('#profile-name-input');
      await expect(nameInput).toHaveValue('Olive Casazza', { timeout: 10000 });

      // Fill in a new display name
      const newName = 'Olive Updated';
      await page.locator('#profile-name-input').fill(newName);
      await page.locator('button:has-text("Update Profile")').click();

      // Verify success alert and name update
      const successAlert = page.locator('#profile-success-alert');
      await expect(successAlert).toBeVisible({ timeout: 8000 });
      await expect(successAlert).toHaveText(/Profile updated successfully/);

      const displayName = page.locator('#profile-display-name');
      await expect(displayName).toHaveText(newName);
    });

    test('should sign out successfully using AppHeader "Sign Out" button', async ({ page }) => {
      console.log('🚪 Testing Sign Out...');
      await page.gotoPath('/');
      
      const signOutBtn = page.locator('button:has-text("Sign Out")').first();
      await expect(signOutBtn).toBeVisible();
      await signOutBtn.click();

      // Should redirect back to home lobby page / as unauthenticated
      console.log('🔄 Verifying sign-out redirect...');
      await page.waitForURL('/');
      await expect(signOutBtn).not.toBeVisible();
    });

    test('should delete user account permanently from profile page', async ({ page }) => {
      // Create a fresh user specifically for deletion so we do not disrupt global/persistent session
      const deleteEmail = `e2e-delete-${Math.random().toString(36).substring(7)}@test.com`;
      const deleteName = 'Delete Target';

      console.log('🧹 Creating a temporary user to test account deletion...');
      // Use clean session for signup & login
      const browser = page.context();
      await browser.clearCookies();
      
      await page.gotoPath('/auth/signup');
      await page.locator('#name').fill(deleteName);
      await page.locator('#email').fill(deleteEmail);
      await page.locator('button[type="submit"]').click();

      const verificationLink = page.locator('#verification-link');
      await expect(verificationLink).toBeVisible();
      await verificationLink.click();

      await expect(page.locator('#verification-success-title')).toBeVisible();

      // Sign-in
      await page.gotoPath('/api/auth/signin');
      await page.locator('input[type="email"]').fill(deleteEmail);
      await page.locator('button:has-text("Sign in with Local Dev")').click();
      await page.waitForURL('/');

      // Go to profile and trigger deletion
      console.log('🗑️ Navigating to profile to delete account...');
      await page.gotoPath('/profile');
      
      // Wait deterministically for Vue hydration to complete
      await page.waitForFunction(() => (window as any).__nuxt_hydrated === true, { timeout: 15000 });

      // Safeguard: Wait for Vue hydration by verifying the input has populated
      const nameInput = page.locator('#profile-name-input');
      await expect(nameInput).toHaveValue(deleteName, { timeout: 10000 });
      
      const deleteBtn = page.locator('#delete-account-btn');
      await expect(deleteBtn).toBeVisible();
      await deleteBtn.click();

      const confirmBtn = page.locator('#confirm-delete-btn');
      await expect(confirmBtn).toBeVisible();
      await confirmBtn.click();

      // Check callback URL redirect to signup page
      console.log('👋 Verifying post-deletion redirect...');
      await page.waitForURL(/.*\/auth\/signup.*/, { timeout: 15000 });
      await expect(page).toHaveURL(/.*\/auth\/signup.*/);
    });
  });

});
