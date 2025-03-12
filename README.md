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

## Contributing

PRs accepted.

Small note: If editing the README, please conform to the
[standard-readme](https://github.com/RichardLitt/standard-readme) specification.

## License

MIT OR Apache-2.0, © 2025 Recall Contributors
