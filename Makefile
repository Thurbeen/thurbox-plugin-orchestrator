.PHONY: fmt fmt-check clippy test build install bootstrap clean

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

bootstrap:
	./scripts/bootstrap-admin.sh

clean:
	cargo clean
