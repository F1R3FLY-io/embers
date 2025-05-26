apt:
	sudo apt-get update && \
	sudo apt install -y libpq-dev && \
	npm -g i concurrently && \
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
	. "$$HOME/.cargo/bin" && \
	. "$$HOME/.cargo/env" && \
	cargo install cargo-make && \
	cargo install taplo-cli && \
	rustup toolchain install nightly

init-f1r3sky-backend-ts:
	cd f1r3sky-backend-ts && \
	pnpm i
	cd f1r3sky-backend-ts/packages/dev-env &&  \
	npm run build

init-f1r3sky-backend-rs:
	cp .env f1r3sky-backend-rs/

init: apt # init-f1r3sky-backend-ts init-f1r3sky-backend-rs
	docker pull f1r3flyindustries/firefly-sky
	docker tag f1r3flyindustries/firefly-sky ghcr.io/f1r3fly-io/rnode:latest

build-server-ts:
	cd f1r3sky-backend-ts/packages/dev-env &&  npm run build

start-server-ts:
	cd f1r3sky-backend-ts && \
	ENABLE_PDS=0 SECOND_NETWORK=0 make run-dev-env

start-server-rs:
	echo "!!! DONT FORGET TO `make docker-up` OUTSIDE OF DEVCONTAINER !!!"
	cd f1r3sky-backend-rs/rsky-pds && \
	docker compose -f ../docker/docker-compose.yaml --profile multiple-networks up --build -d && \
	cargo run

start-client:
	cd f1r3sky/web && yarn web

get-did: start-server-ts
	echo "Copy Bsky Appview#1 DID and past as PDS_BSKY_APP_VIEW_DID to .env"

start:
	npx concurrently "make start-client" "make start-server-rs"

docker-up:
	cd f1r3sky-backend-rs/docker && \
	docker-compose -f docker-compose.yaml up -d --build

docker-down:
	cd f1r3sky-backend-rs/docker && \
	docker-compose -f docker-compose.yaml down

docker-logs:
	cd f1r3sky-backend-rs/docker && \
	docker-compose -f docker-compose.yaml logs -f
