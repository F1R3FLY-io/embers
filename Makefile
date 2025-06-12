deps:
	cargo install cargo-watch
	cargo install cargo-make
dev:
	cd packages/server && \
	cargo watch -x "make run"
