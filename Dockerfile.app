# Use the same Node.js version as your cms Dockerfile for consistency
FROM node:22.12.0-alpine AS base

WORKDIR /app

# Install dependencies only when needed
FROM base AS deps
RUN apk add --no-cache libc6-compat

# Copy the entire monorepo into the container
COPY . .

# Install dependencies for the app and its workspace dependencies using pnpm
RUN corepack enable pnpm && pnpm install --frozen-lockfile --filter=app...

# Rebuild the source code only when needed
FROM base AS builder
WORKDIR /app

# Install Turbo globally
RUN yarn global add turbo@^2.3.3

# Copy the entire monorepo to ensure a clean state for building
COPY . .

# Disable Next.js telemetry during the build
ENV NEXT_TELEMETRY_DISABLED 1

# Install dependencies again in the builder stage (optional if using caching effectively)
RUN corepack enable pnpm && pnpm install --frozen-lockfile --filter=app...

# Build the app using Turbo
# RUN --mount=type=secret,id=ALL_SECRETS \
#   eval "$(base64 -d /run/secrets/ALL_SECRETS)" && \
RUN pnpm run build --filter=app

# Production image, copy the built app and run it
FROM node:22.12.0-alpine AS runner
WORKDIR /app

ENV NODE_ENV production
ENV NEXT_TELEMETRY_DISABLED 1

# Create non-root user
RUN addgroup --system --gid 1001 nodejs
RUN adduser --system --uid 1001 nextjs

# Copy built application from builder
COPY --from=builder /app/apps/app/.next/standalone ./
COPY --from=builder /app/apps/app/.next/static ./apps/app/.next/static

# Set correct permissions
RUN chown -R nextjs:nodejs ./

USER nextjs

EXPOSE 3000

CMD ["node", "apps/app/server.js"]

