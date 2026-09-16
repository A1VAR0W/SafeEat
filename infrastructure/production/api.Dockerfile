FROM node:22-bookworm-slim AS build
WORKDIR /build
COPY package*.json ./
COPY apps/api/package.json apps/api/package.json
COPY apps/mobile/package.json apps/mobile/package.json
COPY apps/web/package.json apps/web/package.json
COPY packages packages
RUN npm ci --workspace @safeeat/api --include-workspace-root --no-audit --no-fund
COPY apps/api apps/api
COPY tsconfig.base.json ./
RUN npm run build:api

FROM node:22-bookworm-slim AS dependencies
WORKDIR /app
COPY package*.json ./
COPY apps/api/package.json apps/api/package.json
COPY apps/mobile/package.json apps/mobile/package.json
COPY apps/web/package.json apps/web/package.json
COPY packages packages
RUN npm ci --omit=dev --workspace @safeeat/api --no-audit --no-fund && npm cache clean --force

FROM node:22-bookworm-slim
ENV NODE_ENV=production PORT=4000 MIGRATIONS_DIR=/app/migrations
WORKDIR /app
COPY --from=dependencies /app/node_modules ./node_modules
COPY --from=dependencies /app/package.json ./package.json
COPY --from=build /build/apps/api/dist ./dist
COPY apps/api/src/db/migrations ./migrations
USER node
EXPOSE 4000
HEALTHCHECK --interval=20s --timeout=4s --start-period=20s CMD node -e "fetch('http://127.0.0.1:4000/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node","dist/server.js"]
