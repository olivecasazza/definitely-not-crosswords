# Stage 1: Build the Nuxt 3 application with Prisma
FROM node:22-slim AS builder

# Install system dependencies (openssl is required by Prisma)
RUN apt-get update && apt-get install -y --no-install-recommends \
    openssl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install pnpm
RUN npm install -g pnpm@11

WORKDIR /app

# Copy dependency definition files
COPY package.json pnpm-lock.yaml .npmrc pnpm-workspace.yaml* ./

# Install dependencies (frozen-lockfile ensures reproducible builds)
RUN pnpm install --frozen-lockfile

# Copy Prisma schema and generate Prisma client
COPY prisma ./prisma/
RUN pnpm prisma generate

# Copy the rest of the application code
COPY . .

# Build the Nuxt 3 application
ENV NODE_ENV=production
RUN pnpm run build

# Stage 2: Production runner
FROM node:22-slim AS runner

# Install system dependencies (openssl is required by Prisma)
RUN apt-get update && apt-get install -y --no-install-recommends \
    openssl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy standalone .output directory from builder stage
COPY --from=builder /app/.output /app/.output
# Copy prisma migrations and schema in case they're needed for migrations in production
COPY --from=builder /app/prisma /app/prisma
# Copy OpenTelemetry startup script to app directory
COPY --from=builder /app/otel.cjs /app/otel.cjs
# Symlink standalone node_modules so that root-level scripts can resolve dependencies
RUN ln -s /app/.output/server/node_modules /app/node_modules

# Expose Nuxt default port and WebSocket port
EXPOSE 3000
EXPOSE 3002

# Run in production mode
ENV NODE_ENV=production
ENV PORT=3000

# Start Nuxt standalone server
CMD [ "node", ".output/server/index.mjs" ]
