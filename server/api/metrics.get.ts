import { register, httpRequestsTotal } from '../../lib/metrics';

export default defineEventHandler(async (event) => {
  // Track this metrics scrape request in the counter
  httpRequestsTotal.inc({
    method: event.node.req.method || 'GET',
    route: event.node.req.url || '/api/metrics',
    status_code: 200
  });

  // Set the response content-type to the format Prometheus expects
  setResponseHeaders(event, {
    'Content-Type': register.contentType
  });

  // Return raw string output of all registry metrics
  return await register.metrics();
});
