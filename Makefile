# `make dev`: Rust API on :3000 + Vite on :5173 (proxies /api). Ctrl+C stops both.
.PHONY: dev

dev: apps/web/node_modules
	@trap 'kill 0' EXIT; cargo run & (cd apps/web && npm run dev) & wait

apps/web/node_modules: apps/web/package.json
	cd apps/web && npm install && touch node_modules
