# CI

This module contains the code for the Recall CI pipeline. It uses [Dagger](https://dagger.io/) to build the code and run
Recall CLI and SDK tests against a `localnet` Docker image. It can be run identically both locally and in CI.

## Prerequisites
- [Dagger](https://dagger.io/docs/install) installed
- [Docker](https://docs.docker.com/get-docker/) installed and running
- [Golang](https://golang.org/doc/install) installed (for `dagger` CLI)

## Initializing the pipeline

If running Dagger for the first time, initialize the pipeline by running the following command in the
`rust-recall/dagger` directory:

```bash
dagger update
```

## Running the pipeline

To run the pipeline, use the following command:

```bash
DAGGER_NO_NAG=1 \
DO_NOT_TRACK=1 \
RECALL_PRIVATE_KEY=0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97 \
DOCKER_PASSWORD=[password] \
dagger call test --progress plain \
  --docker-username [username] \
  --docker-password env://DOCKER_PASSWORD \
  --recall-private-key env://RECALL_PRIVATE_KEY \
  --source ../
```

Docker credentials are passed in to avoid throughput issues with Docker Hub. These can be made optional in a future PR.

The `RECALL_PRIVATE_KEY` environment variable is for one of the wallets created by `anvil`.
