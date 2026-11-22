# Control Center de HiQNet Platform: build estático + Caddy con proxy same-origin.
# El contexto de build es la raíz del repositorio.
FROM node:24.22.0-bookworm-slim AS build
RUN corepack enable
WORKDIR /app
COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY apps/control-center/package.json apps/control-center/
COPY apps/portal/package.json apps/portal/
RUN pnpm install --frozen-lockfile
COPY . .
RUN pnpm --filter @hiqnet/control-center build

FROM caddy:2.10-alpine
COPY deploy/railway/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /app/apps/control-center/dist /srv
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD wget -qO- http://127.0.0.1:8080/healthz >/dev/null 2>&1 || exit 1
