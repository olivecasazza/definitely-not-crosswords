import client from 'prom-client';

// Create a Registry
const register = new client.Registry();

// Add default labels
register.setDefaultLabels({
  app: 'definitely-not-crosswords'
});

// Enable default metrics collection
client.collectDefaultMetrics({ register });

// Define custom metrics
export const activeWebsockets = new client.Gauge({
  name: 'definitely_not_crosswords_active_websockets',
  help: 'Number of currently active WebSocket connections to the game server',
  registers: [register],
});

export const httpRequestsTotal = new client.Counter({
  name: 'definitely_not_crosswords_http_requests_total',
  help: 'Total number of HTTP requests made to the application',
  labelNames: ['method', 'route', 'status_code'],
  registers: [register],
});

export { register };
