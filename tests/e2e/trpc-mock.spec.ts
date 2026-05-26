import { test, expect } from '@nuxt/test-utils/playwright'

test.describe('tRPC Interception and Mocking', () => {
  test('should mock stats.getGlobalLeaderboard and display custom leaderboard', async ({ page }) => {
    // Intercept outbound tRPC calls to stats.getGlobalLeaderboard
    await page.route('**/api/trpc/stats.getGlobalLeaderboard*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          {
            result: {
              data: [
                {
                  id: 'mock-user-1',
                  name: 'Agent Mock',
                  email: 'agent@mock.com',
                  gamesPlayed: 5,
                  totalScore: 9999,
                  totalCorrect: 200,
                  totalIncorrect: 10,
                  accuracy: 95,
                },
              ],
            },
          },
        ]),
      })
    })

    // Navigate to /stats
    await page.goto('/stats')

    // Wait for the podium/leaderboard to load.
    // The name "Agent Mock" should be visible in the first-place podium card.
    const agentMockName = page.locator('text=Agent Mock').first()
    await expect(agentMockName).toBeVisible()

    // Assert that the mocked career score (9999 pts) appears on the page.
    const agentMockScore = page.locator('text=9999 pts')
    await expect(agentMockScore).toBeVisible()
  })
})
