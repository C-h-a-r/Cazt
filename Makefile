.PHONY: dev server web test lint build up down fmt

dev:
	$(MAKE) -j2 server web

server:
	cd server && cargo run

web:
	cd frontend && npm install && npm run dev

test:
	cd server && cargo test
	cd frontend && npm test

lint:
	cd server && cargo fmt --check && cargo clippy --all-targets -- -D warnings
	cd frontend && npm run lint

fmt:
	cd server && cargo fmt
	cd frontend && npx eslint src --fix

build:
	cd frontend && npm run build
	cd server && cargo build --release

up:
	docker compose up -d --build

down:
	docker compose down
