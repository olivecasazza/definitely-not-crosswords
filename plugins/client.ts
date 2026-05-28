import { createWSClient, wsLink, httpBatchLink, splitLink } from '@trpc/client';
import { createTRPCNuxtClient } from 'trpc-nuxt/client'
import type { AppRouter } from '~/server/trpc/router';

export default defineNuxtPlugin(async () => {
  let wsUrl = `ws://localhost:3002/api/trpc-ws`;
  let httpUrl = `/api/trpc`;

  if (process.client) {
    const wsProtocol = window.location.protocol === "https:" ? "wss" : "ws";
    wsUrl = `${wsProtocol}://${window.location.host}/api/trpc-ws`;
    httpUrl = `/api/trpc`;
  } else {
    const port = process.env.PORT || 3000;
    wsUrl = `ws://localhost:${port}/api/trpc-ws`;
    httpUrl = `http://localhost:${port}/api/trpc`;
  }

  const wsClient = createWSClient({
    url: wsUrl,
    lazy: true,
    WebSocket: process.server ? (await import('ws').then(r => r.default || r)) : globalThis.WebSocket as any
  })

  const client = createTRPCNuxtClient<AppRouter>({
    links: [
      splitLink({
        condition(op) {
          return op.type === 'subscription';
        },
        true: wsLink({
          client: wsClient,
        }),
        false: httpBatchLink({
          url: httpUrl,
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
