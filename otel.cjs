const { NodeSDK } = require('@opentelemetry/sdk-node');
const { getNodeAutoInstrumentations } = require('@opentelemetry/auto-instrumentations-node');
const { PrometheusExporter } = require('@opentelemetry/exporter-prometheus');

const port = process.env.OTEL_METRICS_PORT || 9464;

// Initialize the Prometheus exporter
const exporter = new PrometheusExporter({
  port: parseInt(port),
  endpoint: '/metrics',
}, () => {
  console.log(`📊 OpenTelemetry Prometheus Exporter listening on http://localhost:${port}/metrics`);
});

// Initialize OpenTelemetry SDK
const sdk = new NodeSDK({
  metricReader: exporter,
  instrumentations: [
    getNodeAutoInstrumentations({
      // Disable noisy filesystem instrumentation
      '@opentelemetry/instrumentation-fs': { enabled: false },
      // Customize http instrumentation to filter out `/api/metrics` scrapes if desired
      '@opentelemetry/instrumentation-http': {
        ignoreIncomingPaths: [
          '/api/metrics',
          '/api/healthz',
          '/metrics'
        ]
      }
    }),
  ]
});

sdk.start();

console.log('✅ OpenTelemetry SDK initialized successfully');
