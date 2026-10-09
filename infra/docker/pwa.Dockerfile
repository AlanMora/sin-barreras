# syntax=docker/dockerfile:1.7
# PWA ciudadana: build con Node y servida por Nginx sin privilegios.

FROM node:24-alpine AS build
WORKDIR /src
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/pwa/package.json apps/pwa/
RUN --mount=type=cache,id=pnpm-store,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile --filter @sin-barreras/pwa
COPY apps/pwa apps/pwa
ARG VITE_FALLBACK_PHONE=+523322967625
ENV VITE_FALLBACK_PHONE=${VITE_FALLBACK_PHONE}
# El build incluye la verificación del presupuesto: si se excede, falla aquí.
RUN pnpm --filter @sin-barreras/pwa build

# Imagen oficial de Nginx Inc. que corre como usuario no-root (puerto 8080).
FROM nginxinc/nginx-unprivileged:1.29-alpine
COPY infra/nginx/pwa.conf /etc/nginx/conf.d/default.conf
COPY infra/nginx/security-headers.conf /etc/nginx/snippets/security-headers.conf
COPY --from=build /src/apps/pwa/dist /usr/share/nginx/html
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q --spider http://127.0.0.1:8080/healthz || exit 1
