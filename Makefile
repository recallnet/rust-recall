.PHONY: all build install test clean lint check-fmt check-clippy

all: lint build test doc

build:
	cargo build --release

install:
	cargo install --locked --path cli

test:
	cargo test --locked --workspace

doc:
	cargo doc --locked --no-deps --workspace --exclude hoku_cli --open

clean:
	cargo clean

lint: \
	check-fmt \
	check-clippy

check-fmt:
	cargo fmt --all --check

check-clippy:
	cargo clippy --no-deps --tests -- -D clippy::all
