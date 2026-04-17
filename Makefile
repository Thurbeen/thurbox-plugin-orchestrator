.PHONY: fmt fmt-check clippy test build install clean

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all

build:
	cargo build --all

ci: fmt-check clippy test build

install:
	./scripts/install.sh

clean:
	cargo clean
