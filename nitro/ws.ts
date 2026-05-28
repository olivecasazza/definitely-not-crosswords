import { applyWSSHandler } from '@trpc/server/adapters/ws';
import WebSocket, { WebSocketServer as WSWebSocketServer } from 'ws';
import { appRouter } from '~/server/trpc/router';
import { activeWebsockets } from '../lib/metrics';

export default defineNitroPlugin((nitro) => {
  const WebSocketServer = WebSocket.Server || WSWebSocketServer;

  // We attach the WebSocket server directly to Nuxt's HTTP server in the 'listen' hook
  nitro.hooks.hook('listen', (server) => {
    console.log('🔌 Dynamic WebSocket attachment starting...');

    const wss = new WebSocketServer({ noServer: true });

    const handler = applyWSSHandler({
      wss,
      router: appRouter,
    });

    // Initialize the websocket gauge
    activeWebsockets.set(0);

    wss.on('connection', (ws) => {
      activeWebsockets.set(wss.clients.size);
      console.log(`➕➕ Dynamic WS Connection (${wss.clients.size})`);
      
      ws.once('close', () => {
        activeWebsockets.set(wss.clients.size);
        console.log(`➖➖ Dynamic WS Connection (${wss.clients.size})`);
      });
    });

    // Intercept upgrade requests BEFORE Nitro's default listeners by prepending the listener
    server.prependListener('upgrade', (request, socket, head) => {
      const url = request.url || '';
      if (url.includes('/api/trpc-ws')) {
        wss.handleUpgrade(request, socket, head, (ws) => {
          wss.emit('connection', ws, request);
        });
      }
    });

    console.log('✅ Dynamic WebSocket Server attached to Nuxt HTTP server');

    nitro.hooks.hookOnce('close', async () => {
      handler.broadcastReconnectNotification();
      wss.close();
    });
  });
});
