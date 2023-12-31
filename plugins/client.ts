import { createWSClient, wsLink } from '@trpc/client';
import { createTRPCNuxtClient } from 'trpc-nuxt/client'
import { AppRouter } from '~/server/trpc/router';

export default defineNuxtPlugin(async () => {
  const wsClient = createWSClient({
    url: `ws://localhost:3002`,
    WebSocket: process.server ? (await import('ws').then(r => r.default || r)) : globalThis.WebSocket as any
  })

  const client = createTRPCNuxtClient<AppRouter>({
    links: [
      wsLink({
        client: wsClient,
      }),
    ],
  })

  return {
    provide: {
      client,
    },
  }
})
