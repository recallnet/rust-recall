.PHONY: all build test lint check-fmt check-clippy

all: lint build test

build:
	cargo build --release

install:
	cargo install --locked --path cli

test:
	cargo test --locked --workspace

clean:
	cargo clean

lint: \
	check-fmt \
	check-clippy

check-fmt:
	cargo fmt --all --check

check-clippy:
	cargo clippy --no-deps --tests -- -D clippy::all

doc-local:
	cargo doc --no-deps --workspace --exclude adm_cli --open
