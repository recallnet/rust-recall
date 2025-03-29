# Rust Recall

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](./LICENSE-APACHE)
[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg)](https://github.com/RichardLitt/standard-readme)

> Rust interfaces & tooling for Recall

## Table of Contents

- [Background](#background)
- [Usage](#usage)
- [Development](#development)
- [Contributing](#contributing)
- [License](#license)

> [!CAUTION] 
> Recall is currently an alpha testnet, and the network is subject to fortnightly changes
> with rolling updates. Please be aware that the network may be reset at any time, and **data may be
> deleted every two weeks**. A more stable testnet will be released in the future that won't have
> this limitation.

## Background

[Recall](https://docs.recall.network/) is a decentralized platform for testing, verifying, and
evolving AI agents—powering trustless, machine-verifiable decision. This repository contains a Rust
SDK and CLI for interacting with the Recall network.

## Usage

First, clone the repository:

```bash
git clone https://github.com/recallnet/rust-recall.git
cd rust-recall
```

If you want to build and install the CLI, run the following:

```bash
make install
```

You can find detailed usage instructions and available commands in the
[CLI documentation](https://docs.recall.network/tools/cli). If you're looking to build with the Rust
SDK, you can also find more information in the
[Recall SDK documentation](https://docs.recall.network/tools/sdk/rust).

## Development

When developing against a local network, be sure to set the `--network` (or `NETWORK`) to `devnet`.
This presumes you have a local-only setup running, provided by the
[`ipc`](https://github.com/recallnet/ipc) repo and custom contracts in
[`builtin-actors`](https://github.com/recallnet/builtin-actors).

All the available commands include:

- Build all crates: `make build`
- Install the CLI: `make install`
- Run tests: `make test`
- Run linter: `make lint`
- Run formatter: `make check-fmt`
- Run clippy: `make check-clippy`
- Do all of the above: `make all`
- Clean dependencies: `make clean`

## Testing

Some of the tests require a Docker container to be running for `localnet`, and so will fail if you run `make test`
without starting the container first. You can start this container using the following command:

```bash
docker run --privileged --rm --name recall-localnet \
  -p 8545:8545 \
  -p 8645:8645 \
  -p 26657:26657  \
  textile/recall-localnet:latest
```

If you'd like to test against a specific IPC commit, look for the corresponding `localnet` image in the
[Docker Hub repository](https://hub.docker.com/r/textile/recall-localnet/tags) using the first 7 characters of the IPC
commit hash. For example, for commit `8c6792f5c306420a6915e9a83fefb10520417a8b`, the corresponding `localnet` image
would be tagged `sha-8c6792f-*` (in this case, `sha-8c6792f-be1693d`). You can then run the following command:

```bash
docker run --privileged --rm -d --name recall-localnet \
  -p 8545:8545 \
  -p 8645:8645 \
  -p 26657:26657  \
  textile/recall-localnet:sha-8c6792f-be1693d
```

Note that it can take several minutes for the `localnet` container to start up and be ready for testing. You can check
the status of the container using the following command:

```text
docker logs -f recall-localnet
```

The following logs should appear when the container is ready:

```bash
All containers started. Waiting for termination signal...
```

Also note that some tests (e.g. the SDK tests) require additional environment variables to be set. You can set these
environment variables in your shell before running the tests. For example, you can run the following command:

```bash
export RECALL_PRIVATE_KEY=0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97
```

### Adding New Integration Tests

All the tests in the repo are written as Rust unit tests, even the integration tests. New integration tests can be added
to the `sdk/tests` directory. The tests are run using the `cargo test` command, but note that you will need to set the
`RECALL_PRIVATE_KEY` environment variable and start the `localnet` container before running the tests.

## Contributing

PRs accepted.

Small note: If editing the README, please conform to the
[standard-readme](https://github.com/RichardLitt/standard-readme) specification.

## License

MIT OR Apache-2.0, © 2025 Recall Contributors
