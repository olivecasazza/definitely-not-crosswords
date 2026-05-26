import { createWSClient, wsLink, httpBatchLink, splitLink } from '@trpc/client';
import { createTRPCNuxtClient } from 'trpc-nuxt/client'
import type { AppRouter } from '~/server/trpc/router';

export default defineNuxtPlugin(async () => {
  const wsClient = createWSClient({
    url: `ws://localhost:3002`,
    WebSocket: process.server ? (await import('ws').then(r => r.default || r)) : globalThis.WebSocket as any
  })

  const client = createTRPCNuxtClient<AppRouter>({
    links: [
      splitLink({
        condition: (op) => op.type === 'subscription',
        true: wsLink({
          client: wsClient,
        }),
        false: httpBatchLink({
          url: '/api/trpc',
        }),
      }),
    ],
  })

  return {
    provide: {
      client,
    },
  }
})
