.PHONY: all build install test test-sdk test-cli test-all doc clean lint check-fmt check-clippy run-localnet stop-localnet

RECALL_LOCALNET_IMAGE ?= "textile/recall-localnet:latest"

RECALL_NETWORK_CONFIG_FILE ?= /tmp/networks.toml

RECALL_NETWORK ?= localnet

RECALL_PRIVATE_KEY ?= 0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97

RECALL_CLI ?= ./target/release/recall

all: lint test-all doc

build:
	cargo build --release

install:
	cargo install --locked --path cli

test:
	cargo test --locked --workspace --exclude recall_sdk_tests

run-sdk-tests:
	cargo test --locked -p recall_sdk_tests

run-cli-tests:
	RECALL_NETWORK=${RECALL_NETWORK} \
	RECALL_NETWORK_CONFIG_FILE=${RECALL_NETWORK_CONFIG_FILE} \
	RECALL_CLI=${RECALL_CLI} \
	RECALL_PRIVATE_KEY=${RECALL_PRIVATE_KEY} \
	./scripts/run-cli-tests.sh

run-all-tests:
	cargo test --locked --workspace

test-sdk: run-localnet run-sdk-tests
	$(MAKE) stop-localnet

test-cli: build run-localnet run-cli-tests
	$(MAKE) stop-localnet

test-all: build run-localnet run-all-tests run-cli-tests
	$(MAKE) stop-localnet

doc:
	cargo doc --locked --no-deps --workspace --exclude recall_cli --open

clean:
	cargo clean

lint: \
	check-fmt \
	check-clippy

check-fmt:
	cargo fmt --all --check

check-clippy:
	cargo clippy --no-deps --tests -- -D clippy::all

run-localnet:
	$(MAKE) stop-localnet
	docker run --privileged --rm -d --name recall-localnet \
		-p 8545:8545 \
		-p 8645:8645 \
		-p 26657:26657  \
		${RECALL_LOCALNET_IMAGE}
	./scripts/check-localnet-container.sh
	docker cp recall-localnet:/workdir/localnet-data/networks.toml ${RECALL_NETWORK_CONFIG_FILE}

stop-localnet:
	docker rm -f recall-localnet || true
