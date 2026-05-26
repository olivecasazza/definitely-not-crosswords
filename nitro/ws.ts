import { applyWSSHandler } from '@trpc/server/adapters/ws';
import WebSocket, { WebSocketServer as WSWebSocketServer } from 'ws';
import { appRouter } from '~/server/trpc/router';
import { activeWebsockets } from '../lib/metrics';

export default defineNitroPlugin((nitro) => {
  const WebSocketServer = WebSocket.Server || WSWebSocketServer;
  const wss = new WebSocketServer({
    port: 3002,
  });

  const handler = applyWSSHandler({ wss, router: appRouter });

  // Initialize the websocket gauge
  activeWebsockets.set(0);

  wss.on('connection', (ws) => {
    activeWebsockets.set(wss.clients.size);
    console.log(`➕➕ Connection (${wss.clients.size})`);
    
    ws.once('close', () => {
      activeWebsockets.set(wss.clients.size);
      console.log(`➖➖ Connection (${wss.clients.size})`);
    });
  });
  
  console.log('✅ WebSocket Server listening on ws://localhost:3002');

  nitro.hooks.hookOnce('close', async () => {
    handler.broadcastReconnectNotification();
    wss.close();
  })
})
