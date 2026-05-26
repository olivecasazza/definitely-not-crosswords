import { test, expect } from '@nuxt/test-utils/playwright'

test('visual regression test for signin page', async ({ page }) => {
  // Navigate to the auth redirect sign-in page
  await page.goto('/api/auth/signin?callbackUrl=%2F')

  // Wait for the network to be idle to avoid loading flicker
  await page.waitForLoadState('networkidle')

  // Take a visual snapshot and compare it with threshold tolerances
  await expect(page).toHaveScreenshot('signin-page.png', {
    maxDiffPixelRatio: 0.05,
    animations: 'disabled',
  })
})
