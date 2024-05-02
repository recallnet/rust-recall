# adm

[![License](https://img.shields.io/github/license/amazingdatamachine/adm.svg)](./LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg)](https://github.com/RichardLitt/standard-readme)

> The Amazing Data Machine (ADM) CLI

## Table of Contents

- [Table of Contents](#table-of-contents)
- [Background](#background)
- [Usage](#usage)
  - [Global options](#global-options)
  - [Get machine info](#get-machine-info)
  - [List machines](#list-machines)
  - [Object store](#object-store)
    - [Create](#create)
    - [Put an object](#put-an-object)
    - [Get an object](#get-an-object)
    - [List objects](#list-objects)
  - [Accumulator](#accumulator)
    - [Create](#create-1)
    - [Push](#push)
    - [Root](#root)
- [Contributing](#contributing)
- [License](#license)

## Background

_todo_

## Usage

TODO:

- we should add command to generate private key (& view its public key) like the `vaults` CLI
- each command should have docstring examples (aka @dtbuchholz can handle any comments about docstrings)
- include main docstring around being able to use `export WALLET_PK`
- we should add a `config` or `init` command to set the flags like the wallet pk and machine address and read those from env vars, config, or CLI commands

Before getting started, you want to make sure you have a private key (ECDSA, secp256k1) available. You can generate one using the following command:

```sh
adm wallet create <PATH_TO_FILE>
```

### Global options

TODO:

- passing global flags like `--wallet-pk` doesn't work; only env vars do.
- maybe use `--private-key, -pk` instead of `--wallet-pk`? although, maybe the `key` flag is why we're using `wallet-pk` due to potential confusion. indifferent.
- setting `quiet` doesn't seem to silence logging because `⠹ [00:00:00]` is still displayed
- also, idk if the pk is a global flag since it's not used for network reads. more so, it's an issue in certain commands below where it's not listed as a required flag, but i suppose docstrings can add that manually for each CLI command.

| Flag               | Description                                                                     |
| ------------------ | ------------------------------------------------------------------------------- |
| `--rpc-url`        | Node CometBFT RPC URL (default: `http://127.0.0.1:26657`).                      |
| `--object-api-url` | Node Object API URL (default: `http://127.0.0.1:8001`).                         |
| `--wallet-pk`      | Wallet private key (ECDSA, secp256k1) for signing transactions.                 |
| `--chain-name`     | IPC chain name (default: `test`).                                               |
| `--testnet`        | Use testnet addresses (default: `true`).                                        |
| `-v, --verbosity`  | Logging verbosity (`0`: error; `1`: warn; `2`: info; `3`: debug; `4` -> trace). |
| `-q, --quiet`      | Silence logging (default: `false`).                                             |
| `-h, --help`       | Print help.                                                                     |

### Get machine info

TODO:

- maybe: docstrings/docs to explain t1 vs t2 vs t4 addresses, what they mean, and which commands should use it. for a non FVM user, it's confusing, so the docstrings should be explicit.

Get machine metadata at a specific address.

```
adm machine get --address <ADDRESS>
```

| Flag            | Required? | Description                      |
| --------------- | --------- | -------------------------------- |
| `-a, --address` | Yes       | The address of the object store. |

Example:

```
> machine get --address t2xplsbor65en7jome74tk73e5gcgqazuwm5qqamy

{
    "kind": "objectstore",
    "owner": "t410fybp6nnr77jftyumon7y6lfzvr3udtwybx2pcvxi"
}
```

### List machines

TODO:

- the response includes a array of arrays, which is confusing. it should be a list of objects.
- the values for the machine is an array of numbers; is it just an unstructured FIL address as a uint array?

List machine metadata for a specific owner

```
adm machine list --owner <OWNER>
```

| Flag            | Required? | Description                      |
| --------------- | --------- | -------------------------------- |
| `-a, --address` | Yes       | The address of the object store. |

Example:

```
> machine list --owner t410fybp6nnr77jftyumon7y6lfzvr3udtwybx2pcvxi

[
  [
    "ObjectStore",
    [
      ...
    ]
  ],
  [
    "Accumulator",
    [
      ...
    ]
  ]
]
```

### Object store

TODO: we should `#[clap(alias = "os")]` to make it easier to type

Interact with an object store machine type.

```
adm machine objectstore <SUBCOMMAND>
```

#### Create

TODO:

- `--wallet-pk` is required, but it's not listed in the help message.
- idk if this is needed or "correct," but should the `hash` be `0x` prefixed? or is that not a thing on IPC/FVM?

Create a new object store machine.

```
adm machine objectstore create [OPTIONS] --wallet-pk <WALLET_PK>
```

| Flag             | Required? | Description                                                |
| ---------------- | --------- | ---------------------------------------------------------- |
| `--wallet-pk`    | Yes       | Private key to sign transaction.                           |
| `--public-write` | No        | Allow **_public, open_** write access to the object store. |

Example:

```
> adm machine objectstore \
--wallet-pk 1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812 \
create

{
  "address": "t2pefhfyobx2tdgznhcf2anr6p34z2rgso2ix7x5y",
  "tx": {
    "gas_used": 15004808,
    "hash": "3999595D0F74F912323F0F545204BE9D0605CE741275120E553FA395E64DA48D",
    "height": "7964"
  }
}
```

#### Put an object

TODO:

- `--wallet-pk` is required, but it's not listed in the help message.
- passing `--wallet-pk` doesn't work, even though it's logged upon failure.
- the response includes `data: [ ... ]` that has a bunch of numbers...what are they?
- CLI strings can't be passed; only files are acceptable. ideally, it's both.
- the size limit is 1023 bytes...i'm guessing this is temporary. i.e., a file 1024 kb or large fils with an ambiguous `Error: failed to upload object: {"code":404,"message":"Not Found"}`

Puts an object in the object store, signed by a private key.

```
adm machine objectstore put [OPTIONS] \
--wallet-pk <WALLET_PK> \
--address <ADDRESS> \
--key <KEY> \
[PATH_TO_FILE]
```

| Flag              | Required? | Description                                       |
| ----------------- | --------- | ------------------------------------------------- |
| `--wallet-pk`     | Yes       | Private key to sign transaction.                  |
| `-a, --address`   | Yes       | The address of the object store.                  |
| `-k, --key`       | Yes       | The key of the object to create.                  |
| `-p, --prefix`    | No        | Filter objects by an object prefix.               |
| `-d, --delimiter` | No        | Filter objects by a delimiter.                    |
| `-o, --offset`    | No        | Offset to start listing objects (default: `0`)    |
| `-l, --limit`     | No        | Limit the number of objects listed (default: `0`) |

Example:

```
> adm machine objectstore put \
--wallet-pk 1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812 \
--address t2xplsbor65en7jome74tk73e5gcgqazuwm5qqamy \
--key hello \
hello.json

{
  "status": "Committed",
  "hash": "C18DD27F1AB70F6BC15956D7B3A7C7708785A4BDBAE55D23B0A36D62FEF5E03C",
  "height": "6751",
  "gas_used": 9524562,
  "data": [
    1,
    113,
    ...
  ]
}
```

#### Get an object

TODO:

- passing a path to `--output` just logs the data e.g., `{"hello":"world"}` is logged and not downloaded

Gets an object from the object store.

```
adm machine objectstore get [OPTIONS] \
--address <ADDRESS> \
--key <KEY> \
--output <OUTPUT>
```

| Flag            | Required? | Description                                |
| --------------- | --------- | ------------------------------------------ |
| `-a, --address` | Yes       | The address of the object store.           |
| `-k, --key`     | Yes       | The key of the object to get.              |
| `-o, --output`  | Yes       | Output filepath to download the object to. |

Example:

```
> adm machine objectstore put \
--wallet-pk 1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812 \
--address t2xplsbor65en7jome74tk73e5gcgqazuwm5qqamy \
--key hello \
hello.json

{
  "status": "Committed",
  "hash": "C18DD27F1AB70F6BC15956D7B3A7C7708785A4BDBAE55D23B0A36D62FEF5E03C",
  "height": "6751",
  "gas_used": 9524562,
  "data": [
    1,
    113,
    ...
  ]
}
```

#### List objects

TODO:

- this doesn't look to be parsing the key and values...it's just a list of numbers, probs a uint array

```
adm machine objectstore list [OPTIONS] --address <ADDRESS>
```

| Flag              | Required? | Description                                       |
| ----------------- | --------- | ------------------------------------------------- |
| `-a, --address`   | Yes       | The address of the object store.                  |
| `-p, --prefix`    | No        | Filter objects by an object prefix.               |
| `-d, --delimiter` | No        | Filter objects by a delimiter.                    |
| `-o, --offset`    | No        | Offset to start listing objects (default: `0`)    |
| `-l, --limit`     | No        | Limit the number of objects listed (default: `0`) |

Example:

```
> adm machine objectstore list --address t2xplsbor65en7jome74tk73e5gcgqazuwm5qqamy

[
  [
    [
      [
        104,
        101,
        108,
        108,
        111,
        51
      ],
      {
        "Internal": [
          [
            1,
            113,
            160,
            228,
            2,
            32,
            16,
            212,
            38,
            151,
            124,
            120,
            176,
            84,
            33,
            221,
            47,
            26,
            131,
            111,
            55,
            150,
            206,
            103,
            158,
            132,
            5,
            109,
            185,
            107,
            243,
            81,
            205,
            35,
            139,
            237,
            175,
            194
          ],
          1024
        ]
      }
    ]
  ],
  []
]
```

### Accumulator

TODO: we should `#[clap(alias = "acc")]` to make it easier to type

Interact with an accumulator machine type.

```
adm machine accumulator <SUBCOMMAND>
```

#### Create

TODO:

- `--wallet-pk` is required, but it's not listed in the help message.

Create a new accumulator machine.

```
adm machine accumulator create [OPTIONS] --wallet-pk <WALLET_PK>
```

| Flag             | Required? | Description                                                |
| ---------------- | --------- | ---------------------------------------------------------- |
| `--wallet-pk`    | Yes       | Private key to sign transaction.                           |
| `--public-write` | No        | Allow **_public, open_** write access to the object store. |

Example:

```
> adm machine accumulator \
--wallet-pk 1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812 \
create

{
  "address": "t2jhx2rem5tqli3gftzndbfajbbt5rinnmyp7wgyy",
  "tx": {
    "gas_used": 15265338,
    "hash": "F90242BFC63D4538D73AD3A94E3C3C1686FE030889459F2110F81E7DB1A98DAA",
    "height": "895"
  }
}
```

#### Push

TODO:

- `--wallet-pk` is required, but it's not listed in the help message.
- the `data` array is a bunch of numbers, followed by a `0` after the array

Push a new value to the accumulator.

```
adm machine accumulator push --wallet-pk <WALLET_PK> --address <ADDRESS> [INPUT]
```

| Flag            | Required? | Description                      |
| --------------- | --------- | -------------------------------- |
| `--wallet-pk`   | Yes       | Private key to sign transaction. |
| `-a, --address` | Yes       | The address of the accumulator.  |

Example:

```
> adm machine accumulator push \
--wallet-pk 1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812 \
--address t2jhx2rem5tqli3gftzndbfajbbt5rinnmyp7wgyy \
hello.json

{
  "status": "Committed",
  "hash": "DA82111F844BBDD287FC06826A49298C6B7EB2B8CB8CEEBB26D5B83C4879134A",
  "height": "1147",
  "gas_used": 5419928,
  "data": [
    [
      1,
      113,
      ...
    ],
    0
  ]
}
```

#### Root

Get the current accumulator root hash.

```
adm machine accumulator root --address <ADDRESS>
```

| Flag            | Required? | Description                     |
| --------------- | --------- | ------------------------------- |
| `-a, --address` | Yes       | The address of the accumulator. |

Example:

```
> adm machine accumulator root \
--address t2jhx2rem5tqli3gftzndbfajbbt5rinnmyp7wgyy

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
