# Atajos para el trabajo diario. Cada objetivo solo encadena comandos
# estándar (cargo, pnpm, docker compose); no esconde lógica.

.PHONY: help bootstrap up up-apps down logs check test lint build fmt smoke e2e protocol verify dlq

help: ## Muestra esta ayuda
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'

bootstrap: ## Genera certificados TLS y secretos de desarrollo (una vez)
	infra/dev/bootstrap.sh
	pnpm install

up: bootstrap ## Levanta solo la infraestructura (Valkey + Postgres)
	docker compose up -d --wait

up-apps: bootstrap ## Levanta todo el sistema en contenedores
	docker compose --profile apps up -d --build --wait

down: ## Detiene todo (conserva los datos)
	docker compose --profile apps down

logs: ## Sigue los logs de todos los servicios
	docker compose --profile apps logs -f

fmt: ## Formatea Rust y TypeScript
	cargo fmt --all
	pnpm lint:fix

lint: ## Lint de Rust y TypeScript
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	pnpm lint

test: ## Pruebas de Rust y de la PWA
	cargo test --workspace --locked
	pnpm test

build: ## Build de Rust y de la PWA (verifica el presupuesto)
	cargo build --workspace --locked
	pnpm build

smoke: ## Prueba de punta a punta contra el sistema levantado (make up-apps)
	node scripts/smoke-ws.mjs

e2e: ## Pruebas E2E con Playwright y axe contra el sistema levantado (make up-apps)
	cd apps/pwa && pnpm exec playwright test

verify: ## Verifica la integridad del registro completo (rol auditor)
	docker compose run --rm admin verify

dlq: ## Muestra la cola de errores del registro
	docker compose run --rm admin dlq list

protocol: ## Regenera los tipos TypeScript del contrato desde crates/protocol
	UPDATE_TS_BINDINGS=1 cargo test -p protocol --test typescript

check: lint test build ## Todo lo que corre el CI
	pnpm typecheck
