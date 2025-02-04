# Recall SDK

[![License](https://img.shields.io/github/license/recallnet/rust-recall.svg)](../LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg)](https://github.com/RichardLitt/standard-readme)

> Recall SDK

<!-- omit from toc -->

## Table of Contents

- [Table of Contents](#table-of-contents)
- [Background](#background)
  - [Prerequisites](#prerequisites)
- [Usage](#usage)
- [Contributing](#contributing)
- [License](#license)

## Background

Recall SDK is a library for managing your account and data machines.

- _Machine manager_:
  This singleton machine is responsible for creating new buckets and/or timehubs.
- _Bucket machines_:
  These are key-value stores that allow you to push and retrieve data in a familiar S3-like fashion.
  Buckets support byte range requests and advanced queries based on key prefix, delimiter, start key, and
  limit.
- _Timehub machines_:
  An timehub is a [Merkle Mountain Range (MMR)](https://docs.grin.mw/wiki/chain-state/merkle-mountain-range/)-based
  verifiable anchoring system for state updates.
  You can push values up to 500KiB and retrieve them by index, along with the block timestamp of
  the block in which the value was included.

Read more about data machines [here](../README.md).

The SDK consists of the following crates:

- [`recall_provider`](../provider): A chain and object provider for Recall.
- [`recall_signer`](../signer): A transaction signer for Recall.
  This crate has a built-in [wallet](../signer/src/wallet.rs) signer implementation that relies on a local private key
  to sign messages.
- [`recall_sdk`](.): The top-level user interface for managing Recall object storage and timehubs.

The `recall` crates haven't been published yet, but you can read the Cargo docs by building them locally from the repo
root.

```shell
# Build cargo docs and open in your default browser
make doc
```

### Prerequisites

All data is signed onchain as transactions, so you'll need to set up an account (ECDSA, secp256k1) to use Recall network.
For example, any EVM-compatible wallet will work, or you can run
the [`account_deposit.rs`](./examples/account_deposit.rs) example to create a private key for you.

Then, make sure your account is funded with RECALL, so you can pay to execute a transaction (you can use the
faucet [here](https://faucet.calibnet.chainsafe-fil.io/funds.html)).
Follow the [examples](./examples) to get up and running.

## Usage

Checkout the SDK [examples](./examples).
The `recall` crates haven't been published yet, but you can use `recall_sdk` as a git dependencies.

```toml
[dependencies]
recall_sdk = { git = "https://github.com/recallnet/rust-recall.git" }
```

> [!NOTE]
> To use this crate in another crate, include this patch
> for [`merkle-tree-rs`](https://github.com/consensus-shipyard/merkle-tree-rs) in your `Cargo.toml`.
>
> ```toml
> [patch.crates-io]
> # Contains some API changes that the upstream has not merged.
> merkle-tree-rs = { git = "https://github.com/consensus-shipyard/merkle-tree-rs.git", branch = "dev" }
> ```

This issue will be fixed when the `recall` crates get published soon.

## Contributing

PRs accepted.

Small note: If editing the README, please conform to
the [standard-readme](https://github.com/RichardLitt/standard-readme) specification.

## License

MIT OR Apache-2.0, © 2025 Recall Contributors
