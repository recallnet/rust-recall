# Recall SDK

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](../LICENSE)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../LICENSE-APACHE)
[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg)](https://github.com/RichardLitt/standard-readme)

> Recall SDK

<!-- omit from toc -->

## Table of Contents

- [Table of Contents](#table-of-contents)
- [Background](#background)
- [Usage](#usage)
- [Contributing](#contributing)
- [License](#license)

## Background

Recall SDK is a library for managing your account and data interactions on Recall. This repo
consists of the following crates:

- [`recall_provider`](../provider): A chain and object provider for Recall.
- [`recall_signer`](../signer): A transaction signer for Recall. This crate has a built-in
  [wallet](../signer/src/wallet.rs) signer implementation that relies on a local private key to sign
  messages.
- [`recall_sdk`](.): The top-level user interface for managing Recall object storage and timehubs.

These crates haven't been published yet, but you can read the Cargo docs by building them locally
from the repo root.

```shell
# Build cargo docs and open in your default browser
make doc
```

## Usage

You can find detailed usage instructions in the
[Recall SDK documentation](https://docs.recall.network/tools/sdk/rust), and review the
[examples](./examples) for simple quickstarts.

## Contributing

PRs accepted.

Small note: If editing the README, please conform to the
[standard-readme](https://github.com/RichardLitt/standard-readme) specification.

## License

MIT OR Apache-2.0, © 2025 Recall Contributors
