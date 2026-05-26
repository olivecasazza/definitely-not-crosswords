import { test, expect } from '@nuxt/test-utils/playwright'

test.describe('tRPC Interception and Mocking', () => {
  test('should mock stats.getGlobalLeaderboard and display custom leaderboard', async ({ page }) => {
    // Add init script to mock tRPC queries over WebSockets (since app uses wsLink)
    await page.addInitScript(() => {
      const OriginalWebSocket = window.WebSocket;
      class MockWebSocket extends OriginalWebSocket {
        send(data: string | ArrayBufferLike | Blob | ArrayBufferView) {
          if (typeof data === 'string') {
            try {
              const parsed = JSON.parse(data);
              if (parsed.method === 'query' && parsed.params?.path === 'stats.getGlobalLeaderboard') {
                const responseId = parsed.id;
                const mockResponse = {
                  id: responseId,
                  result: {
                    type: 'data',
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
                };
                
                setTimeout(() => {
                  const messageEvent = new MessageEvent('message', {
                    data: JSON.stringify(mockResponse),
                    origin: this.url,
                  });
                  this.dispatchEvent(messageEvent);
                }, 50);
                
                return;
              }
            } catch (e) {
              // ignore
            }
          }
          super.send(data);
        }
      }
      window.WebSocket = MockWebSocket as any;
    });

    // Also support fallback HTTP intercept just in case client links change to HTTP
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
      });
    });

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
