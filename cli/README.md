# ADM CLI

[![License](https://img.shields.io/github/license/amazingdatamachine/adm.svg)](../LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg)](https://github.com/RichardLitt/standard-readme)

> The Amazing Data Machine (ADM) CLI

## Table of Contents

- [Table of Contents](#table-of-contents)
- [Background](#background)
    - [Prerequisites](#prerequisites)
- [Usage](#usage)
    - [Installation](#installation)
    - [Configuration](#configuration)
    - [Global options](#global-options)
    - [Account management](#account-management)
        - [Create an account](#create-an-account)
        - [Get account info](#get-account-info)
        - [Get account sequence](#get-account-sequence)
        - [Get account balance](#get-account-balance)
        - [Deposit funds](#deposit-funds)
        - [Withdraw funds](#withdraw-funds)
        - [Transfer funds](#transfer-funds)
    - [Machine](#machine)
        - [Get machine info](#get-machine-info)
    - [Object store](#object-store)
        - [Create](#create)
        - [List object stores](#list-object-stores)
        - [Add an object](#add-an-object)
        - [Get an object](#get-an-object)
        - [Delete an object](#delete-an-object)
        - [Query objects](#query-objects)
    - [Accumulator](#accumulator)
        - [Create](#create-1)
        - [List accumulators](#list-accumulators)
        - [Push](#push)
        - [Get leaf](#get-leaf)
        - [Get count](#get-count)
        - [Get peaks](#get-peaks)
        - [Get root](#get-root)
- [Contributing](#contributing)
- [License](#license)

## Background

The ADM CLI is a tool for managing your account and data machines.

- _Object store machines_:
  These are key-value stores that allow you to push and retrieve data in a familiar S3-like fashion.
  Object stores support byte range requests and advanced queries based on key prefix, delimiter, offset, and
  limit.
- _Accumulator machines_:
  An accumulator is a [Merkle Mountain Range (MMR)](https://docs.grin.mw/wiki/chain-state/merkle-mountain-range/)-based
  verifiable anchoring system for state updates.
  You can push values up to 500KiB and retrieve them by index.
  Accumulators support querying for state root, MMR peaks, and total leaf count.

Read more about data machines [here](../README.md).

### Prerequisites

All data is signed onchain as transactions, so you'll need to set up an account (ECDSA, secp256k1) to use the ADM
network. For example, any EVM-compatible wallet will work, or you can run the `adm account create` command to create a
private key for you.

Then, make sure your account is funded with FIL, so you can pay to execute a transaction (you can use the
faucet [here](https://faucet.calibnet.chainsafe-fil.io/funds.html)). When you `deposit` funds from the parent (Filecoin
Calibration) to the child subnet, it will register your account on the subnet. If you ever want to move funds back to
the parent, the `withdraw` command can be used. Note these differ from moving funds intra-subnet, which requires you
to use the `transfer` command. These are described in more detail below.

## Usage

### Installation

To install the CLI, you'll need to download it from source, build, and install it.

```sh
git clone https://github.com/amazingdatamachine/adm
cd adm
make build
make install
```

Once installed, you can run the `adm` command from your terminal.

```sh
adm --help
```

### Configuration

There are two flags required for the majority of the `adm` subcommands:

- `--network`: Specify the chain location with RPC presets and settings that map to either `mainnet`, `testnet`,
  or `devnet`.
- `--private-key`: A wallet private key (ECDSA, secp256k1) for signing transactions.

As a best practice, you should create a `.env` file with the following and run `source .env` to ensure the commands
load these variables.
The default network is `testnet`, so it's not necessary to set the variable unless you're
developing locally (`devnet`).

```
PRIVATE_KEY=your_private_key
NETWORK=testnet
```

Each of the following sections includes examples that presume you've completed this setup step.
Thus, the `--private-key` and `--network` flags will not be shown in most demonstrations.

One small note on all the getter methods and the `--height` flag:

- By default, it uses the latest `committed` block on the network.
- You can also specify `pending` including any pending state changes
- For historical queries, you can a specific block number to query the data.

Also, all commands that send mutating transactions default to broadcasting them in `commit` mode, but `sync` and `async`
modes are also possible.

### Global options

All the global flags can also be passed as all-caps, snake case environment variables
(e.g., `--rpc-url` => `RPC_URL`) that are set and sourced in a `.env` file.

| Flag              | Description                                                                                |
| ----------------- | ------------------------------------------------------------------------------------------ |
| `-n`, `--network` | Network presets for subnet and RPC: `mainnet`, `testnet`, or `devnet` (default: `testnet`) |
| `-s`, `--subnet`  | The ID of the target subnet.                                                               |
| `--rpc-url`       | Node CometBFT RPC URL.                                                                     |
| `-v, --verbosity` | Logging verbosity (`0`: error; `1`: warn; `2`: info; `3`: debug; `4`: trace).              |
| `-q, --quiet`     | Silence logging (default: `false`).                                                        |
| `-h, --help`      | Print help.                                                                                |
| `-V, --version`   | Print version.                                                                             |

### Account management

Interaction with the ADM network requires an account (ECDSA, secp256k1). As with any blockchain system, an account can
be created at will, receive / transfer funds, and send transactions. Recall that on Filecoin, and EVM `0x` prefixed
address is equivalent to a `t410...`/`f410...` address, which is a special namespace that enables for EVM-compatiablity
in the FVM.

The `account` command allows you to execute these actions within the ADM:

```
adm account
```

The following subcommands are available:

- `create`: Create a new account with a private key.
- `info`: Get account information (address, sequence, balance).
- `deposit`: Deposit funds into a subnet from its parent.
- `withdraw`: Withdraw funds from a subnet to its parent.
- `transfer`: Transfer funds to another account in a subnet.

#### Create an account

Create a new account from a random seed.

```
adm account create
```

This command logs a JSON object to stdout with three properties: the private key, public key, and its corresponding
FVM-converted address.

**Example:**

Create a new private key:

```
> adm account create

{
  "private_key": "d5020dd0b12d4d8d8793ff0edbaa29bd7197879ddf82d475b7e9a6a34de765b0",
  "address": "0xc37ab532c1409900520a92e04a6c0482394d3133",
  "fvm_address": "t410fyn5lkmwbicmqauqkslqeu3aeqi4u2mjturajlui"
}
```

- Optionally, pipe its output into a file to store the key and metadata:

```
> adm account create > account.json
```

#### Get account info

Get account information.

```
adm account info {--private-key <PRIVATE_KEY> | --address <ADDRESS>}
```

This commands logs a JSON object to stdout: its public key, FVM address, current sequence (nonce), current subnet
balance, and its balance on the parent subnet.

| Flag                   | Required?                | Description                                                           |
| ---------------------- | ------------------------ | --------------------------------------------------------------------- |
| `-p, --private-key`    | Yes, if no `address`     | Wallet private key (ECDSA, secp256k1) for signing transactions.       |
| `-a, --address`        | Yes, if no `private-key` | Account address; the signer's address is used if no address is given. |
| `--height`             | No                       | Query at a specific block height (default: `committed`).              |
| `--evm-rpc-api`        | No                       | The Ethereum API RPC HTTP endpoint.                                   |
| `--evm-rpc-timeout`    | No                       | Timeout for calls to the Ethereum API (default: `60 seconds`).        |
| `--evm-rpc-auth-token` | No                       | Bearer token for any Authorization header.                            |
| `--evm-gateway`        | No                       | The gateway contract address.                                         |
| `--evm-registry`       | No                       | The registry contract address.                                        |

**Example:**

Create a new private key, optionally, piping its output into a file:

```
> adm account info \
--address 0x4D5286d81317E284Cd377cB98b478552Bbe641ae

{
  "address": "0x4d5286d81317e284cd377cb98b478552bbe641ae",
  "fvm_address": "0x4d5286d81317e284cd377cb98b478552bbe641ae",
  "sequence": 5,
  "balance": "0.2",
  "parent_balance": "108.263573407968179933"
}
```

#### Get account sequence

Get an account sequence (i.e., nonce) in a subnet.

```
adm account sequence {--private-key <PRIVATE_KEY> | --address <ADDRESS>}
```

You must pass _either_ the `--private-key` or `--address` flag. An address must be in the delegated `t410` or `0x`
format.

- `adm account sequence --private-key <PRIVATE_KEY>`: Query with a private key (e.g., read from your `.env` file).
  (e.g., read from your `.env` file).
- `adm account sequence --address <ADDRESS>`: Query a `t410` or `0x` address.

| Flag                | Required?                | Description                                                           |
| ------------------- | ------------------------ | --------------------------------------------------------------------- |
| `-p, --private-key` | Yes, if no `address`     | Wallet private key (ECDSA, secp256k1) for signing transactions.       |
| `-a, --address`     | Yes, if no `private-key` | Account address; the signer's address is used if no address is given. |
| `--height`          | No                       | Query at a specific block height (default: `committed`).              |

**Examples:**

Get the sequence by:

- Hex address:

```
> adm objectstore list \
--address 0x4D5286d81317E284Cd377cB98b478552Bbe641ae

{
  "sequence": 2
}
```

- Its equivalent `t410` address:

```
> adm objectstore list \
--address t410fjvjinwatc7rijtjxps4ywr4fkk56mqnolzpcnrq
```

#### Get account balance

Get an account balance within a specific subnet.

```
adm account balance {--private-key <PRIVATE_KEY> | --address <ADDRESS>}
```

You must pass _either_ the `--private-key` or `--address` flag. An address must be in the delegated `t410` or `0x`
format.

- `adm account sequence --private-key <PRIVATE_KEY>`: Query with a private key (e.g., read from your `.env` file).
- `adm account sequence --address <ADDRESS>`: Query a `t410` or `0x` address.

The `--parent` flag allows you to get the balance of the parent.
If the `--network` flag is set, it will handle all the required `--evm-...` flag presets for you,
but you _can_ override them with your own values.

| Flag                   | Required?                | Description                                                           |
| ---------------------- | ------------------------ | --------------------------------------------------------------------- |
| `-p, --private-key`    | Yes, if no `address`     | Wallet private key (ECDSA, secp256k1) for signing transactions.       |
| `-a, --address`        | Yes, if no `private-key` | Account address; the signer's address is used if no address is given. |
| `--parent`             | No                       | Fetch the balance at the parent subnet (boolean flag).                |
| `--height`             | No                       | Query at a specific block height (default: `committed`).              |
| `--evm-rpc-api`        | No                       | The Ethereum API RPC HTTP endpoint.                                   |
| `--evm-rpc-timeout`    | No                       | Timeout for calls to the Ethereum API (default: `60 seconds`).        |
| `--evm-rpc-auth-token` | No                       | Bearer token for any Authorization header.                            |
| `--evm-gateway`        | No                       | The gateway contract address.                                         |
| `--evm-registry`       | No                       | The registry contract address.                                        |

**Examples:**

- Get the signer's balance on the subnet:

```
> adm account balance

{
  "balance": "0.2"
}
```

- Get its balance on the parent subnet:

```
> adm account balance --parent

{
  "balance": "100.5"
}
```

- Get the balance at a specific address on the subnet:

```
> adm account balance \
--address 0x4D5286d81317E284Cd377cB98b478552Bbe641ae
```

#### Deposit funds

Deposit funds into a subnet from its parent.

```
adm account deposit [--to <TO>] <AMOUNT>
```

Think of the `deposit` command as a typical transfer but _only_ from a parent to a child subnet. Both a transfer _out
of_ and _within_ a subnet are handled differently.

| Positionals | Description                      |
| ----------- | -------------------------------- |
| `<AMOUNT>`  | The amount to transfer (in FIL). |

Optionally, you can pass the `--to` flag to deposit funds from the parent to a specific address on the child, but if you
don't, the funds will be deposited to the address corresponding to the provided private key. If the `--network` flag is
set, it will handle all the required `--evm-...` flag presets for you, but you _can_ override them with your own
values.

| Flag                   | Required? | Description                                                                       |
| ---------------------- | --------- | --------------------------------------------------------------------------------- |
| `-p, --private-key`    | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.                   |
| `--to <TO>`            | No        | The recipient account address (if not present, defaults to the signer's address). |
| `--evm-rpc-api`        | No        | The Ethereum API RPC HTTP endpoint.                                               |
| `--evm-rpc-timeout`    | No        | Timeout for calls to the Ethereum API (default: `60 seconds`).                    |
| `--evm-rpc-auth-token` | No        | Bearer token for any Authorization header.                                        |
| `--evm-gateway`        | No        | The gateway contract address.                                                     |
| `--evm-registry`       | No        | The registry contract address.                                                    |

**Examples:**

- Deposit funds to the signer's address:

```
> adm account deposit 0.1

{
  "transactionHash": "0xcc7fdf8057dd9f024582b24fce2abe0f5e0c01f1e925fb52bd002c4456333bfc",
  "transactionIndex": "0x2",
  "blockHash": "0xdc623f489bb53aaa16186818858c63a5e4e694ed1b798fddae9f96b8d16b4e4b",
  "blockNumber": "0x18b456",
  "from": "0x181c2d11DbB674147Ba53F2cf26Cf6DF9d9cc0aC",
  "to": "0x728f3b71ebd1358973abce325fe45f7f701ea7e6",
  "cumulativeGasUsed": "0x0",
  "gasUsed": "0x41a8748",
  "contractAddress": null,
  "logs": [
    {
      "address": "0x728f3b71ebd1358973abce325fe45f7f701ea7e6",
      "topics": [
        "0xfdd39ce2560484814971f663392e78ae37dc62ba184b3370d830371dd271a8b7",
        "0x000000000000000000000000fdf8c3fb4af3b0c60f7128d2dce1281fdfa9ca6d"
      ],
      "data": "0x0000...",
      "blockHash": "0xdc623f489bb53aaa16186818858c63a5e4e694ed1b798fddae9f96b8d16b4e4b",
      "blockNumber": "0x18b456",
      "transactionHash": "0xcc7fdf8057dd9f024582b24fce2abe0f5e0c01f1e925fb52bd002c4456333bfc",
      "transactionIndex": "0x2",
      "logIndex": "0x0",
      "removed": false
    }
  ],
  "status": "0x1",
  "root": "0x0000000000000000000000000000000000000000000000000000000000000000",
  "logsBloom": "0xffaf...",
  "type": "0x2",
  "effectiveGasPrice": "0x6fbefce0"
}
```

- Deposit funds to some other, non-signer address:

```
> adm account deposit --to 0x181c2d11DbB674147Ba53F2cf26Cf6DF9d9cc0aC 0.1
```

#### Withdraw funds

Withdraw funds from a subnet to its parent.

```
adm account withdraw [--to <TO>] <AMOUNT>
```

The `withdraw` command is the opposite of a `deposit`. It's somewhat like a typical transfer but _only_ from a child
subnet to its parent.

| Positionals | Description                      |
| ----------- | -------------------------------- |
| `<AMOUNT>`  | The amount to transfer (in FIL). |

Optionally, you can pass the `--to` flag to withdraw subnet funds to a specific address on the parent, but if you don't,
the funds will be withdrawn to the address corresponding to the provided private key. If the `--network` flag is set, it
will handle all the required `--evm-...` flag presets for you, but you _can_ override them with your own values.

| Flag                   | Required? | Description                                                                       |
| ---------------------- | --------- | --------------------------------------------------------------------------------- |
| `-p, --private-key`    | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.                   |
| `--to <TO>`            | No        | The recipient account address (if not present, defaults to the signer's address). |
| `--evm-rpc-api`        | No        | The Ethereum API RPC HTTP endpoint.                                               |
| `--evm-rpc-timeout`    | No        | Timeout for calls to the Ethereum API (default: `60 seconds`).                    |
| `--evm-rpc-auth-token` | No        | Bearer token for any Authorization header.                                        |
| `--evm-gateway`        | No        | The gateway contract address.                                                     |
| `--evm-registry`       | No        | The registry contract address.                                                    |

**Examples:**

- Withdraw funds to the signer's address:

```
> adm account withdraw 0.1

{
  "transactionHash": "0xb098e39c4b358e5f55cd6f2db941092ff50b46d99db53c34101cac3f0f65f20d",
  "transactionIndex": "0x0",
  "blockHash": "0x3ebcd0c3b94a5076fffbeef95fd23cdd764a222679450e451dac6ce28b601eb2",
  "blockNumber": "0x19532",
  "from": "0x181c2d11DbB674147Ba53F2cf26Cf6DF9d9cc0aC",
  "to": "0x77aa40b105843728088c0132e43fc44348881da8",
  "cumulativeGasUsed": "0x5e63fa3",
  "gasUsed": "0x5e63fa3",
  "contractAddress": null,
  "logs": [],
  "status": "0x1",
  "root": "0x341c4ad32b230e66cdc5bf75e522934defa276afb88d705ce52a34336655b3a1",
  "logsBloom": "0x0000...",
  "type": "0x2",
  "effectiveGasPrice": "0x0"
}
```

- Withdraw funds to some other, non-signer address:

```
> adm account withdraw --to 0x181c2d11DbB674147Ba53F2cf26Cf6DF9d9cc0aC 0.1
```

#### Transfer funds

Transfer funds to another account in a subnet.

```
adm account transfer --to <TO> <AMOUNT>
```

| Positionals | Description                      |
| ----------- | -------------------------------- |
| `<AMOUNT>`  | The amount to transfer (in FIL). |

The `--to` flag is the destination address within the subnet that you want to send funds to. If the `--network` flag is
set, it will handle all the required `--evm-...` flag presets for you, but you _can_ override them with your own
values.

| Flag                   | Required? | Description                                                     |
| ---------------------- | --------- | --------------------------------------------------------------- |
| `-p, --private-key`    | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions. |
| `--to <TO>`            | Yes       | The recipient account address.                                  |
| `--evm-rpc-api`        | No        | The Ethereum API RPC HTTP endpoint.                             |
| `--evm-rpc-timeout`    | No        | Timeout for calls to the Ethereum API (default: `60 seconds`).  |
| `--evm-rpc-auth-token` | No        | Bearer token for any Authorization header.                      |
| `--evm-gateway`        | No        | The gateway contract address.                                   |
| `--evm-registry`       | No        | The registry contract address.                                  |

**Example:**

```
> adm account transfer \
--to 0x4D5286d81317E284Cd377cB98b478552Bbe641ae \
0.1

{
  "transactionHash": "0x814759e167906ffc65dd20c6ceb4cdd42e5f64f9af7ca5bcd2ac1ea365ce715d",
  "transactionIndex": "0x0",
  "blockHash": "0xf496f8f9bdfb909696513411f01abd72184446a9c846f6016a85c9601294d4d0",
  "blockNumber": "0x1a7d2",
  "from": "0x181c2d11DbB674147Ba53F2cf26Cf6DF9d9cc0aC",
  "to": "0x4d5286d81317e284cd377cb98b478552bbe641ae",
  "cumulativeGasUsed": "0x18f28a",
  "gasUsed": "0x18f28a",
  "contractAddress": null,
  "logs": [],
  "status": "0x1",
  "root": "0x05b06003f5986d96409d53af89e0d1ad44b8f8487254beb6bef20cda0d7e0874",
  "logsBloom": "0x0000...",
  "type": "0x2",
  "effectiveGasPrice": "0x410"
}
```

### Machine

Machines are the core building blocks of the ADM. The `machine` command allows you to retrieve machine information
relative to a specific address. This helps track which `ObjectStore` or `Accumulator` machines are tied to your account,
which are later used in the `objectstore` and `accumulator` subcommands.

#### Get machine info

Get machine metadata at a specific address.

```
adm machine info <ADDRESS>
```

| Positionals | Description      |
| ----------- | ---------------- |
| `<ADDRESS>` | Machine address. |

| Flag       | Required? | Description                                              |
| ---------- | --------- | -------------------------------------------------------- |
| `--height` | No        | Query at a specific block height (default: `committed`). |

**Example:**

```
> adm machine info t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa

{
    "kind": "ObjectStore",
    "owner": "0x4d5286d81317e284cd377cb98b478552bbe641ae"
}
```

### Object store

Interact with an object store machine using either the `objectstore` or aliased `os` subcommand:

```
adm objectstore <SUBCOMMAND>
adm os <SUBCOMMAND>
```

The `objectstore` subcommand has the following subcommands:

- `create`: Create a new object store machine.
- `list`: List object stores by owner in a subnet.
- `add`: Add an object into the object store.
- `get`: Get an object from the object store.
- `delete`: Delete an object from the object store.
- `query`: Query objects in the object store.

When you create objects, the `key` is a custom identifier that, by default, uses the `/` delimiter to create a key-based
hierarchy. The value is the data you want to store, which can be a file path. A best practice is to
decide on a key naming convention that makes sense for your data, such as `<namespace>/<id>` or similar. The
hierarchical structure of the key allows for easy retrieval of data by prefixes, which is explained below (see
the `query` subcommand).

#### Create

Create a new object store machine.

```
adm objectstore create
```

| Flag                | Required? | Description                                                               |
| ------------------- | --------- | ------------------------------------------------------------------------- |
| `-p, --private-key` | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.           |
| `--public-write`    | No        | Allow **_public, open_** write access to the object store.                |
| `--gas-limit`       | No        | Gas limit for the transaction.                                            |
| `--gas-fee-cap`     | No        | Maximum gas fee for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL). |
| `--gas-premium`     | No        | Gas premium for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).     |
| `--sequence`        | No        | Sequence (i.e., nonce) for the transaction.                               |

**Example:**

```
> adm objectstore create

{
  "address": "t2pefhfyobx2tdgznhcf2anr6p34z2rgso2ix7x5y",
  "tx": {
    "gas_used": 15004808,
    "hash": "3999595D0F74F912323F0F545204BE9D0605CE741275120E553FA395E64DA48D",
    "height": "7964"
  }
}
```

#### List object stores

List object stores by owner in a subnet.

```
adm objectstore list {--private-key <PRIVATE_KEY> | --address <ADDRESS>}
```

You must pass _either_ the `--private-key` or `--address` flag. An address must be in the delegated `t410` or `0x`
format.

- `adm objectstore list --private-key <PRIVATE_KEY>`: Query with a private key (or read from your `.env` file).
- `adm objectstore list --address <ADDRESS>`: Query a `t410` or `0x` address.

| Flag                | Required?                | Description                                                           |
| ------------------- | ------------------------ | --------------------------------------------------------------------- |
| `-p, --private-key` | Yes, if no `address`     | Wallet private key (ECDSA, secp256k1) for signing transactions.       |
| `-a, --address`     | Yes, if no `private-key` | Account address; the signer's address is used if no address is given. |
| `--height`          | No                       | Query at a specific block height (default: `committed`).              |

**Examples:**

Query machines by:

- A hex address:

```
> adm objectstore list \
--address 0x4D5286d81317E284Cd377cB98b478552Bbe641ae

[
  {
    "address": "t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa",
    "kind": "ObjectStore"
  },
  {
    "address": "t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa",
    "kind": "ObjectStore"
  }
]
```

- Its equivalent `t410` address:

```
> adm objectstore list \
--address t410fjvjinwatc7rijtjxps4ywr4fkk56mqnolzpcnrq
```

- At a specific block height (note how at this older height, there were fewer machines created than the most
  recent `committed` height above):

```
> adm objectstore list --height 114345
[
  {
    "address": "t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa",
    "kind": "ObjectStore"
  }
]
```

#### Add an object

Add an object with a key prefix.

```
adm objectstore add \
--address <ADDRESS> \
--key <KEY> \
[INPUT]
```

The `INPUT` can be a file path.

| Flag                   | Required? | Description                                                                           |
| ---------------------- | --------- | ------------------------------------------------------------------------------------- |
| `-p, --private-key`    | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.                       |
| `-a, --address`        | Yes       | Object store machine address.                                                         |
| `-k, --key`            | Yes       | Key of the object to upload.                                                          |
| `-o, --overwrite`      | No        | Overwrite the object if it already exists.                                            |
| `-b, --broadcast-mode` | No        | Broadcast mode for the transaction: `commit`, `sync`, or `async` (default: `commit`). |
| `--gas-limit`          | No        | Gas limit for the transaction.                                                        |
| `--gas-fee-cap`        | No        | Maximum gas fee for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).             |
| `--gas-premium`        | No        | Gas premium for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).                 |
| `--sequence`           | No        | Sequence (i.e., nonce) for the transaction.                                           |

**Examples:**

- Push a file to the object store:

```
> adm objectstore add \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
--key "my/object" \
./hello.json

{
  "status": "committed",
  "hash": "48BD1767DC5739C1EABB25FBC8E6718E3E8DE95FB7EE74D13895F2E9D9F5E00A",
  "height": "358569",
  "gas_used": 4784697,
  "data": "bafy2bzacebnjpu5e3ushfu2weqvmtvk7vnndg4fkqsbr4zub52cyekcix7l4o"
}
```

#### Get an object

Get an object from the object store machine.

```
adm objectstore get --address <ADDRESS> <KEY>
```

| Positionals | Description               |
| ----------- | ------------------------- |
| `<KEY>`     | Key of the object to get. |

Note that when you retrieve the object, it will be written to stdout.

| Flag               | Required? | Description                                                                                                   |
| ------------------ | --------- | ------------------------------------------------------------------------------------------------------------- |
| `-a, --address`    | Yes       | Object store machine address.                                                                                 |
| `--object-api-url` | No        | Node Object API URL.                                                                                          |
| `--range`          | No        | Range of bytes to get from the object (format: `"start-end"`; inclusive). Example: "0-99" => first 100 bytes. |
| `--height`         | No        | Query at a specific block height (default: `committed`).                                                      |

**Examples:**

- Get an object and write to stdout (default behavior):

```
> adm objectstore get \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
"my/object"

{"hello":"world"}
```

- Download the output to a file by piping the output:

```
> adm objectstore get \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
"my/object" > downloaded.json
```

- Range request for a subset of bytes:

```
> adm objectstore get \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
--range "10-14" \
"my/object"

world
```

#### Delete an object

Delete an object from the object store.

```
adm objectstore delete \
--address <ADDRESS> \
<KEY>
```

| Positionals | Description               |
| ----------- | ------------------------- |
| `<KEY>`     | Key of the object to get. |

Similar to when you `add` an object, you can specify gas settings or alter the broadcast mode.

| Flag                   | Required? | Description                                                                           |
| ---------------------- | --------- | ------------------------------------------------------------------------------------- |
| `-p, --private-key`    | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.                       |
| `-a, --address`        | Yes       | Object store machine address.                                                         |
| `--object-api-url`     | No        | Node Object API URL.                                                                  |
| `-b, --broadcast-mode` | No        | Broadcast mode for the transaction: `commit`, `sync`, or `async` (default: `commit`). |
| `--gas-limit`          | No        | Gas limit for the transaction.                                                        |
| `--gas-fee-cap`        | No        | Maximum gas fee for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).             |
| `--gas-premium`        | No        | Gas premium for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).                 |
| `--sequence`           | No        | Sequence (i.e., nonce) for the transaction.                                           |

**Example:**

- Delete an existing object:

```
> adm objectstore delete \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
"my/object"

{
  "status": "committed",
  "hash": "C718E2E7A4BC4DC0E6AB705698E8950CE3EBEADEC3268206145E5A109E76FBB4",
  "height": "358561",
  "gas_used": 4739569,
  "data": "bafy2bzaceamp42wmmgr2g2ymg46euououzfyck7szknvfacqscohrvaikwfay"
}
```

#### Query objects

Query across all objects in the store.

```
adm objectstore query --address <ADDRESS>
```

Performing a `query` lists all keys that match a given prefix _up to and including the delimiter_.
If the key supplies a delimiter, then the results stop there—essentially, listing subfolders, but none lower.
Think of it as you would when listing files in a directory.
If you list the contents of a folder, you'll see all subfolders,
but you won't see the contents of one of those subfolders.

For example, if you have the keys `my/object`, `my/data`, and `my/object/child`, and you query for the prefix `my/`, you
will get the objects at `my/object` and `my/data` but not `my/object/child` since its "nested" under the
prefix `my/object/` (note: inclusive of the `/` at the end).

| Flag              | Required? | Description                                                                        |
| ----------------- | --------- | ---------------------------------------------------------------------------------- |
| `-a, --address`   | Yes       | Object store machine address.                                                      |
| `-p, --prefix`    | No        | The prefix to filter objects by (defaults to empty string).                        |
| `-d, --delimiter` | No        | The delimiter used to define object hierarchy (default: `/`).                      |
| `-o, --offset`    | No        | The offset from which to start listing objects (default: `0`)                      |
| `-l, --limit`     | No        | The maximum number of objects to list, where `0` indicates max (10k)(default: `0`) |
| `--height`        | No        | Query at a specific block height (default: `committed`).                           |

**Examples:**

- Get all objects but without any filtering. Since the object keys have a delimiter included, you'll see the common
  prefix `my/`, but no objects are listed since the "root" is the prefix:

```
> adm objectstore query \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa

{
  "objects": [],
  "common_prefixes": [
    "my/"
  ]
}
```

- Get all objects under a specific prefix. In this case, the response will include all objects under the `my/` prefix,
  and since there are no "child" objects that match `my/` (e.g., `my/object/child`), the `common_prefixes` array will be
  empty, so you know there are no more sub-objects to list:

```
> adm objectstore query \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
--prefix "my/"

{
  "objects": [
    {
      "key": "my/object",
      "value": {
        "kind": "internal",
        "content": "bafy2bzacecgbwqdlb2ujejlsjyjnqq77ky6yhrsj7fa2dpafsdrcxsamzntby",
        "size": 18
      }
    },
    {
      "key": "my/data",
      "value": {
        "kind": "external",
        "content": "bafybeigdp2yqaqdbfhltvxdt3m5xmsrbvzyvtjrz5klhee33vpr5hdnpou",
        "resolved": true
      }
    }
  ],
  "common_prefixes": []
}
```

> [!NOTE]
> You can see the `my/object` object's `kind` is `internal` (shown with its `size` in bytes), and `my/data`
> object's `kind` is `external`. The `external` kind means it's a "detached" object that isn't stored fully onchain but
> externally on IPFS. That is, only the CID is stored onchain, but differs from `internal`'s onchain object storage. Any
> objects over 1 KB (1024 bytes) are considered `external`. Also, the `resolved` flag indicates whether the reference
> has been resolved or not by nodes on the network.

- Get all objects and "ignore" the delimiter. Here, an arbitrary `"*"` symbol is used as the delimiter; it's been chosen
  since it doesn't exist in the example's keys that are used. Thus, this effectively lists all objects in the store
  because the delimiter isn't in the keys, so the `common_prefixes` array will be empty. The response will be the same
  as above.

```
> adm objectstore query \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
--delimiter "*"
```

- Get all objects and filter by a prefix with offset and limit. In the example above, the `"my/data"` object was created
  _after_ `"my/object"`, so it will be the first object listed after offsetting by `1`:

```
> adm objectstore query \
--address t2weumc7otsi3kniwjgy2xnemws5jpi3vmbnxg4fa \
--delimiter "/" \
--prefix "my/" \
--offset 1 \
--limit 1

{
  "objects": [
    {
      "key": "my/data",
      "value": {
        "kind": "external",
        "content": "bafybeigdp2yqaqdbfhltvxdt3m5xmsrbvzyvtjrz5klhee33vpr5hdnpou",
        "resolved": true
      }
    }
  ],
  "common_prefixes": []
}
```

### Accumulator

Interact with an accumulator machine type using either the `accumulator` or aliased `ac` subcommand:

```
adm machine accumulator <SUBCOMMAND>
adm machine ac <SUBCOMMAND>
```

The `accumulator` subcommand has the following subcommands:

- `create`: Create a new accumulator.
- `list`: List accumulators by owner in a subnet.
- `push`: Push a value to the accumulator.
- `leaf`: Get leaf at a given index and height.
- `count`: Get leaf count at a given height.
- `root`: Get the root of the accumulator.
- `peaks`: Get peaks at a given height.

#### Create

Create a new accumulator machine.

```
adm machine accumulator create
```

| Flag                | Required? | Description                                                               |
| ------------------- | --------- | ------------------------------------------------------------------------- |
| `-p, --private-key` | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.           |
| `--public-write`    | No        | Allow **_public, open_** write access to the object store.                |
| `--gas-limit`       | No        | Gas limit for the transaction.                                            |
| `--gas-fee-cap`     | No        | Maximum gas fee for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL). |
| `--gas-premium`     | No        | Gas premium for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).     |
| `--sequence`        | No        | Sequence (i.e., nonce) for the transaction.                               |

**Example:**

```
> adm machine accumulator create

{
  "address": "t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia",
  "tx": {
    "gas_used": 18240442,
    "hash": "65C5D751A96B115530C1DBE3CF94012C2DD083565BAD5B2A27F9C0D6400B5206",
    "height": "114345"
  }
}
```

#### List accumulators

List accumulators by owner in a subnet.

```
adm accumulator list {--private-key <PRIVATE_KEY> | --address <ADDRESS>}
```

You must pass _either_ the `--private-key` or `--address` flag. An address must be in the delegated `t410` or `0x`
format.

- `adm accumulator list --private-key <PRIVATE_KEY>`: Query with a private key (or read from your `.env` file).
- `adm accumulator list --address <ADDRESS>`: Query a `t410` or `0x` address.

| Flag                | Required?                | Description                                                           |
| ------------------- | ------------------------ | --------------------------------------------------------------------- |
| `-p, --private-key` | Yes, if no `address`     | Wallet private key (ECDSA, secp256k1) for signing transactions.       |
| `-a, --address`     | Yes, if no `private-key` | Account address; the signer's address is used if no address is given. |
| `--height`          | No                       | Query at a specific block height (default: `committed`).              |

**Examples:**

Query machines by:

- A hex address:

```
> adm accumulator list \
--address 0x4D5286d81317E284Cd377cB98b478552Bbe641ae

[
  {
    "address": "t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia",
    "kind": "Accumulator"
  }
]
```

- Its equivalent `t410` address:

```
> adm accumulator list \
--address t410fjvjinwatc7rijtjxps4ywr4fkk56mqnolzpcnrq
```

- At a specific block height:

```
> adm accumulator list --height 339004
```

#### Push

Push a value to the accumulator.

```
adm machine accumulator push --address <ADDRESS> [INPUT]
```

The `INPUT` can be a file path.

| Flag                   | Required? | Description                                                                           |
| ---------------------- | --------- | ------------------------------------------------------------------------------------- |
| `-p, --private-key`    | Yes       | Wallet private key (ECDSA, secp256k1) for signing transactions.                       |
| `-a, --address`        | Yes       | Accumulator machine address.                                                          |
| `-b, --broadcast-mode` | No        | Broadcast mode for the transaction: `commit`, `sync`, or `async` (default: `commit`). |
| `--gas-limit`          | No        | Gas limit for the transaction.                                                        |
| `--gas-fee-cap`        | No        | Maximum gas fee for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).             |
| `--gas-premium`        | No        | Gas premium for the transaction in attoFIL (1FIL = 10\*\*18 attoFIL).                 |
| `--sequence`           | No        | Sequence (i.e., nonce) for the transaction.                                           |

**Examples:**

- Push a file to the accumulator:

```
> adm machine accumulator push \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia \
./hello.json

{
  "status": "committed",
  "hash": "142520D57D5DEB8C45F99F010950E76828CFA6EEDBA564A2A8BD7EF8FC79B34F",
  "height": "25249",
  "gas_used": 5419928,
  "data": {
    "root": "bafy2bzaceb7dtmkm77d7osdrpczjo3ytziofzzlavwuri744hijyxhzuuvsgk",
    "index": 0
  }
}
```

#### Get leaf

Get leaf at a given index and height.

```
adm machine accumulator leaf --address <ADDRESS> <INDEX>
```

| Positionals | Description |
| ----------- | ----------- |
| `<INDEX>`   | Leaf index. |

| Flag            | Required? | Description                                              |
| --------------- | --------- | -------------------------------------------------------- |
| `-a, --address` | Yes       | Accumulator machine address.                             |
| `--height`      | No        | Query at a specific block height (default: `committed`). |

**Example:**

- Get leaf at index `0` (the "hello world" object pushed above):

```
> adm machine accumulator leaf \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia \
0

{"hello":"world"}
```

#### Get count

Get the leaf counts at a given height.

```
adm machine accumulator count --address <ADDRESS>
```

| Flag            | Required? | Description                                                                                                  |
| --------------- | --------- | ------------------------------------------------------------------------------------------------------------ |
| `-a, --address` | Yes       | Accumulator machine address.                                                                                 |
| `--height`      | No        | Query block height: `committed`, `pending`, or a specific block height (e.g., `123`) (default: `committed`). |

**Examples:**

- Get the leaf count, which is just a single leaf at this point:

```
> adm machine accumulator root \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

{
  "count": 1
}
```

- If you push another piece of data, the count will increase:

```
> echo '{"hello":"again"}' | adm machine accumulator push \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

> adm machine accumulator root \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

{
  "count": 2
}
```

#### Get peaks

Get the peaks at a given height.

```
adm machine accumulator peaks --address <ADDRESS>
```

| Flag            | Required? | Description                                              |
| --------------- | --------- | -------------------------------------------------------- |
| `-a, --address` | Yes       | Accumulator machine address.                             |
| `--height`      | No        | Query at a specific block height (default: `committed`). |

**Examples:**

- Since there are only two leaves, there is only one peak since it's a balanced tree:

```
> adm machine accumulator peaks \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

{
  "peaks": [
    "bafy2bzaceaongqda6ddrhjf6o5x4lc7fejiichb7drqe6qmjqb5wrab6h3ayu"
  ]
}
```

- Pushing another piece of data (i.e., three total) leads to another peak:

```
> echo '{"hello":"basin"}' | adm machine accumulator push \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

> adm machine accumulator peaks \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

{
  "peaks": [
    "bafy2bzaceaongqda6ddrhjf6o5x4lc7fejiichb7drqe6qmjqb5wrab6h3ayu",
    "bafy2bzacedr2nvhvsiq2qyq5uoxczq4o2jinatofi2ba5tmffta7ir4psiwem"
  ]
}
```

#### Get root

Get the root at a given height.

```
adm machine accumulator root --address <ADDRESS>
```

| Flag            | Required? | Description                                              |
| --------------- | --------- | -------------------------------------------------------- |
| `-a, --address` | Yes       | Accumulator machine address.                             |
| `--height`      | No        | Query at a specific block height (default: `committed`). |

**Example:**

```
> adm machine accumulator root \
--address t2ous5hrcemefjn76ks2oiylz3ae2qkpkuydyu4ia

{
  "root": "bafy2bzacea4moduioz6jwq3kthmpgq7q7mgxruujh2aqbuhp6agwfwercmbie"
}
```

## Contributing

PRs accepted.

Small note: If editing the README, please conform to
the [standard-readme](https://github.com/RichardLitt/standard-readme) specification.

## License

MIT OR Apache-2.0, © 2024 ADM Contributors
