import { test, expect } from '@playwright/test'

test('redirects to sign-in page when unauthenticated', async ({ page }) => {
  // Go to the protected home page
  await page.goto('/')

  // Should redirect to a sign-in or login url, or show auth page elements
  await expect(page).toHaveURL(/.*\/api\/auth\/signin.*/)
})
