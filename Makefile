deps:
	cargo install cargo-watch
	cargo install cargo-make

node:
	cd docker && docker-compose up -V

dev:
	cd packages/server && \
	cargo watch -x "make run"
