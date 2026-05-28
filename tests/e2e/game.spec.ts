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

test.describe('Crossword Game E2E and TDD Tests', () => {
  test.beforeEach(async ({ page, baseURL }) => {
    // Pipe browser console messages to node console for debuggability
    page.on('console', msg => {
      console.log(`[BROWSER CONSOLE] ${msg.type().toUpperCase()}: ${msg.text()}`);
    });

    // Pipe unhandled page exceptions to node console
    page.on('pageerror', err => {
      console.error(`[BROWSER EXCEPTION] ${err.name}: ${err.message}\n${err.stack}`);
    });

    // Mock the next-auth session and csrf API endpoints to ensure the app sees us as authenticated
    await page.route('**/api/auth/session', async (route) => {
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
    });

    await page.route('**/api/auth/csrf', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ csrfToken: 'mock-csrf-token' })
      });
    });

    // Inject active session cookie with the dynamic hostname used by the Nuxt test server
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
      }
    ]);

    // Mock the tRPC HTTP queries and mutations since they go over httpBatchLink
    await page.route('**/api/trpc/activeGame.get*', async (route) => {
      const mockState = await page.evaluate(() => (window as any).__MOCK_STATE__);
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{ result: { data: mockState?.activeGame || null } }])
      });
    });

    await page.route('**/api/trpc/generator.listJobs*', async (route) => {
      const mockState = await page.evaluate(() => (window as any).__MOCK_STATE__);
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{ result: { data: mockState?.jobs || [] } }])
      });
    });

    await page.route('**/api/trpc/generator.getJob*', async (route) => {
      const mockState = await page.evaluate(() => (window as any).__MOCK_STATE__);
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{
          result: {
            data: {
              id: 'mock-job-id',
              status: 'SUCCEEDED',
              topic: 'Space Exploration and Science',
              width: 21,
              height: 21,
              createdAt: new Date().toISOString(),
              resultGame: mockState?.activeGame?.game || null
            }
          }
        }])
      });
    });

    await page.route('**/api/trpc/stats.getCompletedGame*', async (route) => {
      const mockState = await page.evaluate(() => (window as any).__MOCK_STATE__);
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{
          result: {
            data: {
              id: 'mock-completed-game-id',
              createdAt: new Date().toISOString(),
              game: mockState?.activeGame?.game || null,
              gameStats: {
                id: 'mock-stats-id',
                memberScores: [
                  {
                    id: 'score-1',
                    score: 40,
                    correctGuesses: 4,
                    incorrectGuesses: 0,
                    member: mockState?.activeGame?.gameMembers?.[0]
                  },
                  {
                    id: 'score-2',
                    score: 30,
                    correctGuesses: 4,
                    incorrectGuesses: 5,
                    member: mockState?.activeGame?.gameMembers?.[1]
                  }
                ]
              }
            }
          }
        }])
      });
    });

    await page.route('**/api/trpc/activeGame.addActions*', async (route) => {
      const postData = route.request().postDataJSON();
      const input = Array.isArray(postData) ? postData[0] : postData;
      const actionsToRegister = input?.actions || [];

      const newActions = await page.evaluate((acts) => {
        const state = (window as any).__MOCK_STATE__;
        if (!state) return [];
        const newActs = acts.map((act: any) => ({
          id: `mock-action-${Math.random()}`,
          ...act,
          userId: 'user-1',
          type: 'GameAction',
          submittedAt: new Date().toISOString()
        }));
        state.activeGame.actions = [
          ...(state.activeGame.actions || []),
          ...newActs
        ];
        (window as any).__TRIGGER_SUBSCRIPTION__('activeGame.onAddActions', newActs);
        return newActs;
      }, actionsToRegister);

      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{ result: { data: newActions } }])
      });
    });

    await page.route('**/api/trpc/activeGame.complete*', async (route) => {
      await page.evaluate(() => {
        const completedPayload = {
          activeGameId: 'mock-active-game-id',
          completedGameId: 'mock-completed-game-id'
        };
        (window as any).__TRIGGER_SUBSCRIPTION__('activeGame.onGameCompleted', completedPayload);
      });

      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{ result: { data: { id: 'mock-completed-game-id' } } }])
      });
    });

    await page.route('**/api/trpc/generator.publishGeneratedGame*', async (route) => {
      await page.evaluate(() => {
        const state = (window as any).__MOCK_STATE__;
        if (state && state.activeGame && state.activeGame.game) {
          state.activeGame.game.published = true;
        }
      });

      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{ result: { data: { published: true } } }])
      });
    });

    // Add init script to mock WebSocket-based tRPC calls cleanly.
    // This provides a completely robust TDD environment and isolated testing environment.
    await page.addInitScript(() => {
      const OriginalWebSocket = window.WebSocket;

      const mockState = {
        activeGame: {
          id: 'mock-active-game-id',
          type: 'ActiveGame',
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          gameId: 'mock-game-id',
          actions: [] as any[],
          gameMembers: [
            {
              id: 'member-1',
              activeGameId: 'mock-active-game-id',
              completedGameId: null,
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              isOwner: true,
              type: 'MEMBER',
              userId: 'user-1',
              user: {
                id: 'user-1',
                name: 'Olive Casazza',
                email: 'olive.casazza@gmail.com'
              }
            },
            {
              id: 'member-2',
              activeGameId: 'mock-active-game-id',
              completedGameId: null,
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              isOwner: false,
              type: 'MEMBER',
              userId: 'user-2',
              user: {
                id: 'user-2',
                name: 'Co-Player',
                email: 'coplayer@gmail.com'
              }
            }
          ],
          game: {
            id: 'mock-game-id',
            title: 'Space Exploration Crossword',
            source: 'GENERATED',
            published: false,
            questions: [
              {
                id: 'q1',
                number: 1,
                direction: 'ACROSS',
                answer: 'MOON',
                questionText: "Earth's natural satellite",
                rootX: 0,
                rootY: 0,
              },
              {
                id: 'q2',
                number: 1,
                direction: 'DOWN',
                answer: 'MARS',
                questionText: 'The red planet',
                rootX: 0,
                rootY: 0,
              }
            ]
          }
        },
        jobs: [] as any[],
        activeSubscriptions: {} as Record<number, { path: string; input: any }>
      };

      (window as any).__MOCK_STATE__ = mockState;
      (window as any).__MOCK_SOCKETS__ = [];

      class MockWebSocket extends EventTarget {
        url: string;
        readyState: number;
        onopen: any = null;
        onmessage: any = null;
        onerror: any = null;
        onclose: any = null;

        constructor(url: string, protocols?: string | string[]) {
          super();
          this.url = url;
          this.readyState = 0; // CONNECTING
          console.log(`[Mock WS] Constructor called with url: ${url}`);
          (window as any).__MOCK_SOCKETS__.push(this);

          // Simulate connection open
          setTimeout(() => {
            if (this.readyState === 0) {
              this.readyState = 1; // OPEN
              const openEvent = new Event('open');
              this.dispatchEvent(openEvent);
              if (typeof this.onopen === 'function') {
                this.onopen(openEvent);
              }
            }
          }, 10);
        }

        send(data: string) {
          if (typeof data !== 'string') {
            return;
          }
          console.log(`[Mock WS] Sending data: ${data}`);
          try {
            const parsed = JSON.parse(data);
            
            const handleMessage = (msg: any) => {
              const { id, method, params } = msg;
              console.log(`[Mock WS] Received method: ${method}, path: ${params?.path}, id: ${id}`);

              if (method === 'query') {
                const path = params?.path;
                const input = params?.input;

                if (path === 'activeGame.get') {
                  this.sendResponse(id, mockState.activeGame);
                  return true;
                }

                if (path === 'generator.listJobs') {
                  this.sendResponse(id, mockState.jobs);
                  return true;
                }

                if (path === 'generator.getJob') {
                  const job = mockState.jobs.find(j => j.id === input.id) || {
                    id: input.id,
                    status: 'SUCCEEDED',
                    topic: 'space exploration',
                    width: 21,
                    height: 21,
                    createdAt: new Date().toISOString(),
                    resultGame: mockState.activeGame.game
                  };
                  this.sendResponse(id, job);
                  return true;
                }

                if (path === 'stats.getCompletedGame') {
                  this.sendResponse(id, {
                    id: input.id,
                    createdAt: new Date().toISOString(),
                    game: mockState.activeGame.game,
                    gameStats: {
                      id: 'mock-stats-id',
                      memberScores: [
                        {
                          id: 'score-1',
                          score: 40,
                          correctGuesses: 4,
                          incorrectGuesses: 0,
                          member: mockState.activeGame.gameMembers[0]
                        },
                        {
                          id: 'score-2',
                          score: 30,
                          correctGuesses: 4,
                          incorrectGuesses: 5,
                          member: mockState.activeGame.gameMembers[1]
                        }
                      ]
                    }
                  });
                  return true;
                }
              }

              if (method === 'mutation') {
                const path = params?.path;
                const input = params?.input;

                if (path === 'activeGame.addActions') {
                  const newActions = input.actions.map((act: any) => ({
                    id: `mock-action-${Math.random()}`,
                    ...act,
                    userId: 'user-1',
                    type: 'GameAction',
                    submittedAt: new Date().toISOString()
                  }));
                  mockState.activeGame.actions = [
                    ...mockState.activeGame.actions,
                    ...newActions
                  ];

                  (window as any).__TRIGGER_SUBSCRIPTION__('activeGame.onAddActions', newActions);
                  this.sendResponse(id, newActions);
                  return true;
                }

                if (path === 'activeGame.complete') {
                  const completedPayload = {
                    activeGameId: input.id,
                    completedGameId: 'mock-completed-game-id'
                  };
                  (window as any).__TRIGGER_SUBSCRIPTION__('activeGame.onGameCompleted', completedPayload);
                  this.sendResponse(id, { id: 'mock-completed-game-id' });
                  return true;
                }

                if (path === 'generator.publishGeneratedGame') {
                  mockState.activeGame.game.published = true;
                  this.sendResponse(id, { ...mockState.activeGame.game, published: true });
                  return true;
                }
              }

              if (method === 'subscription') {
                const path = params?.path;
                const input = params?.input;

                console.log(`[Mock WS] Registering subscription to path: ${path} at id: ${id}`);
                mockState.activeSubscriptions[id] = { path, input };
                this.sendSubscriptionStarted(id);

                if (path === 'generator.runGeneration') {
                  setTimeout(() => {
                    this.sendSubscriptionData(id, { type: 'started', jobId: 'mock-job-id', at: Date.now() });
                  }, 50);

                  setTimeout(() => {
                    this.sendSubscriptionData(id, { type: 'progress', stage: 'generating_grid', current: 5, total: 10, message: 'Placing words...', at: Date.now() });
                  }, 150);

                  setTimeout(() => {
                    const mockJob = {
                      id: 'mock-job-id',
                      status: 'SUCCEEDED',
                      topic: input.params.topic,
                      width: input.params.width,
                      height: input.params.height,
                      createdAt: new Date().toISOString(),
                      resultGame: mockState.activeGame.game
                    };
                    mockState.jobs.push(mockJob);

                    this.sendSubscriptionData(id, {
                      type: 'completed',
                      jobId: 'mock-job-id',
                      gameId: 'mock-active-game-id',
                      title: 'Space Exploration Crossword',
                      questionCount: 2,
                      metrics: {},
                      at: Date.now()
                    });
                  }, 300);
                }
                return true;
              }

              if (method === 'subscription.unsubscribe') {
                console.log(`[Mock WS] Unsubscribing id: ${id}`);
                delete mockState.activeSubscriptions[id];
                return true;
              }

              return false;
            };

            if (Array.isArray(parsed)) {
              console.log(`[Mock WS] Received batch array of length ${parsed.length}`);
              let handledAny = false;
              for (const item of parsed) {
                const handled = handleMessage(item);
                if (handled) handledAny = true;
              }
              if (handledAny) return;
            } else {
              const handled = handleMessage(parsed);
              if (handled) return;
            }
          } catch (e) {
            console.error('[Mock WS] Error in send:', e);
          }
        }

        sendResponse(id: number, data: any) {
          const resp = {
            id,
            result: {
              type: 'data',
              data
            }
          };
          setTimeout(() => {
            const msgEvent = new MessageEvent('message', {
              data: JSON.stringify(resp),
              origin: this.url
            });
            this.dispatchEvent(msgEvent);
            if (typeof this.onmessage === 'function') {
              this.onmessage(msgEvent);
            }
          }, 20);
        }

        sendSubscriptionStarted(id: number) {
          const resp = {
            id,
            result: {
              type: 'started'
            }
          };
          setTimeout(() => {
            const msgEvent = new MessageEvent('message', {
              data: JSON.stringify(resp),
              origin: this.url
            });
            this.dispatchEvent(msgEvent);
            if (typeof this.onmessage === 'function') {
              this.onmessage(msgEvent);
            }
          }, 10);
        }

        sendSubscriptionData(id: number, data: any) {
          const resp = {
            id,
            result: {
              type: 'data',
              data
            }
          };
          setTimeout(() => {
            const msgEvent = new MessageEvent('message', {
              data: JSON.stringify(resp),
              origin: this.url
            });
            this.dispatchEvent(msgEvent);
            if (typeof this.onmessage === 'function') {
              this.onmessage(msgEvent);
            }
          }, 10);
        }

        close() {
          this.readyState = 3; // CLOSED
          const closeEvent = new Event('close');
          this.dispatchEvent(closeEvent);
          if (typeof this.onclose === 'function') {
            this.onclose(closeEvent);
          }
        }
      }

      (window as any).__TRIGGER_SUBSCRIPTION__ = (path: string, data: any) => {
        console.log(`[__TRIGGER_SUBSCRIPTION__] Path: ${path}, Sockets count: ${(window as any).__MOCK_SOCKETS__?.length}`);
        console.log(`Active subscriptions:`, JSON.stringify(mockState.activeSubscriptions));
        if (!(window as any).__MOCK_SOCKETS__) return;
        for (const socket of (window as any).__MOCK_SOCKETS__) {
          for (const [subId, sub] of Object.entries(mockState.activeSubscriptions)) {
            if (sub.path === path) {
              console.log(`[__TRIGGER_SUBSCRIPTION__] Matching path ${path} found. Emitting data to subId: ${subId}`);
              socket.sendSubscriptionData(Number(subId), data);
            }
          }
        }
      };

      window.WebSocket = MockWebSocket as any;
    });
  });

  test('should generate a crossword game through admin generator page', async ({ page }) => {
    // Navigate to admin crossword generator page
    await page.gotoPath('/admin/generator')

    // Verify page header is visible
    const pageHeader = page.locator('text=CROSSWORD GENERATOR')
    await expect(pageHeader).toBeVisible()

    // Verify form input topics is populated or we can fill it
    const topicInput = page.locator('#topic')
    await expect(topicInput).toBeVisible()
    await topicInput.fill('Space Exploration and Science')

    // Click "Generate" to trigger the mock subscription stream
    const generateBtn = page.locator('button:has-text("Generate")')
    await generateBtn.click()

    // Assert that the progress bar / log appears
    const progressContainer = page.locator('text=Placing words...')
    await expect(progressContainer).toBeVisible()

    // Wait for generator completed state: "View Games" button appears
    const viewGamesBtn = page.locator('button:has-text("View Games")')
    await expect(viewGamesBtn).toBeVisible({ timeout: 5000 })

    // Verify that the generated job appears in the jobs table
    const jobTopicCell = page.locator('table tbody tr td').filter({ hasText: 'Space Exploration and Science' })
    await expect(jobTopicCell).toBeVisible()

    // Click publish game
    const publishBtn = page.locator('button:has-text("Publish")').first()
    await expect(publishBtn).toBeVisible()
    await publishBtn.click()

    // After publishing, the button text updates or disappears/disables
    const viewGamesBtnAfter = page.locator('button:has-text("View Games")')
    await expect(viewGamesBtnAfter).toBeVisible()
  })

  test('should load the active game, allow inputting answers, support real-time sync, and redirect when completed', async ({ page }) => {
    // Go to the specific active game room
    await page.gotoPath('/game/mock-active-game-id')

    // 1. Board and Clue rendering: Assert that cells and clues appear correctly
    const acrossHeader = page.locator('text=Across')
    await expect(acrossHeader).toBeVisible()

    const downHeader = page.locator('text=Down')
    await expect(downHeader).toBeVisible()

    const clue1 = page.locator("text=Earth's natural satellite")
    await expect(clue1).toBeVisible()

    // Click "Down" to filter Down clues and verify "The red planet" is visible
    await page.locator('button:has-text("Down")').click()
    const clue2 = page.locator('text=The red planet')
    await expect(clue2).toBeVisible()

    // Click "Across" to filter Across clues back
    await page.locator('button:has-text("Across")').click()

    // 2. Solving / Clue input: Simulating filling cells or typing letters, and verifying if a cell state updates
    // Click on the first clue to select it
    await clue1.click()

    // The active clue card should appear and contain 4 input boxes (since MOON is 4 letters)
    const clueCard = page.locator('text=CLUE 1 • 4 LETTERS')
    await expect(clueCard).toBeVisible()

    const characterInputs = page.locator('input[type="text"]')
    await expect(characterInputs).toHaveCount(4)

    // Type the correct guess: M-O-O-N
    await characterInputs.nth(0).fill('M')
    await characterInputs.nth(1).fill('O')
    await characterInputs.nth(2).fill('O')
    await characterInputs.nth(3).fill('N')

    // Click Guess
    const guessBtn = page.locator('button:has-text("Guess")')
    await guessBtn.click()

    // Verify that the board displays the correct guessed letters (M, O, O, N)
    // The first clue "Earth's natural satellite" in the list should display bubbles with letters
    const cellBubbles = page.locator('text=Earth\'s natural satellite').locator('xpath=..').locator('text=M')
    await expect(cellBubbles).toBeVisible()

    // Wait until the subscriptions are registered in mockState before triggering them
    await page.waitForFunction(() => {
      const state = (window as any).__MOCK_STATE__;
      if (!state || !state.activeSubscriptions) return false;
      const subs = Object.values(state.activeSubscriptions) as any[];
      return subs.some(s => s.path === 'activeGame.onAddActions') &&
             subs.some(s => s.path === 'activeGame.onGameCompleted');
    }, { timeout: 10000 });

    // 3. Real-time multiplayer synchronization: Simulating a multiplayer environment by mocking WebSocket message
    // Simulate player 2 submitting "MARS" (root coordinate is same 0,0, but direction DOWN)
    // Coords for MARS: M(0,0), A(0,1), R(0,2), S(0,3)
    await page.evaluate(() => {
      (window as any).__TRIGGER_SUBSCRIPTION__('activeGame.onAddActions', [
        {
          id: 'action-remote-1',
          activeGameId: 'mock-active-game-id',
          cordX: 0,
          cordY: 1,
          actionType: 'correctGuess',
          previousState: '',
          state: 'A',
          userId: 'user-2',
          submittedAt: new Date().toISOString()
        },
        {
          id: 'action-remote-2',
          activeGameId: 'mock-active-game-id',
          cordX: 0,
          cordY: 2,
          actionType: 'correctGuess',
          previousState: '',
          state: 'R',
          userId: 'user-2',
          submittedAt: new Date().toISOString()
        },
        {
          id: 'action-remote-3',
          activeGameId: 'mock-active-game-id',
          cordX: 0,
          cordY: 3,
          actionType: 'correctGuess',
          previousState: '',
          state: 'S',
          userId: 'user-2',
          submittedAt: new Date().toISOString()
        }
      ]);

      // Trigger the real-time game completed event, simulating player 2 completing the board
      (window as any).__TRIGGER_SUBSCRIPTION__('activeGame.onGameCompleted', {
        activeGameId: 'mock-active-game-id',
        completedGameId: 'mock-completed-game-id'
      });
    });

    // Since the game was successfully completed by both players (MOON + MARS completed),
    // it will trigger the store's complete mutation and navigate automatically to /game/mock-completed-game-id/completed.
    // Let's assert the redirection and victory screen.
    await page.waitForURL(/.*\/game\/.*\/completed/)

    // 5. Completed Game: Simulating a completed game flow and verifying the scoreboard/statistics
    const victoryText = page.locator('text=Crossword Solved!')
    await expect(victoryText).toBeVisible()

    // Leaderboard matches the mock standings
    const oliveScoreboardCell = page.locator('text=Olive Casazza').first()
    await expect(oliveScoreboardCell).toBeVisible()

    const coplayerScoreboardCell = page.locator('text=Co-Player').first()
    await expect(coplayerScoreboardCell).toBeVisible()

    // Performance statistics
    const statsDashboardLink = page.locator('text=Career Stats Dashboard')
    await expect(statsDashboardLink).toBeVisible()
  })
})
