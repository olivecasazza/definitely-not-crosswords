import { test as base, expect } from '@nuxt/test-utils/playwright'

// Extend Playwright's page fixture to support the gotoPath method required by instructions
const test = base.extend<{
  page: any
}>({
  page: async ({ page, baseURL }, use) => {
    (page as any).gotoPath = (path: string, options?: any) => {
      const targetUrl = baseURL ? new URL(path, baseURL).toString() : path;
      return page.goto(targetUrl, options);
    };
    await use(page);
  }
})

test('redirects to sign-in page when unauthenticated', async ({ page }) => {
  // Go to the protected home page
  await page.gotoPath('/')

  // Should redirect to a sign-in or login url, or show auth page elements
  await expect(page).toHaveURL(/.*\/api\/auth\/signin.*/)
})
