# CI

This module contains the code for the Recall CI pipeline. It uses [Dagger](https://dagger.io/) to build the code and run
Recall CLI and SDK tests against a `localnet` Docker image. It can be run identically both locally and in CI.

## Prerequisites
- [Dagger](https://docs.dagger.io/install) installed
- [Docker](https://docs.docker.com/get-docker/) installed and running

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
dagger call test --progress plain  \
  --source ../ \
  2>&1 | grep -vi -E "resolve|containerd|libnetwork|client|daemon|checkpoint|task|^$"
```

The `grep` command is used to filter out some of the Dagger output that is not relevant to the pipeline. You can remove
it if you want to see all the output.

### Specifying Docker Credentials

Docker credentials can optionally be passed in to avoid throttling issues with Docker Hub:

```bash
DAGGER_NO_NAG=1 \
DO_NOT_TRACK=1 \
dagger call test --progress plain \
  --source ../ \
  --docker-username $DOCKER_USERNAME \
  --docker-password env://DOCKER_PASSWORD \
  2>&1 | grep -vi -E "resolve|containerd|libnetwork|client|daemon|checkpoint|task|^$"
```

### Specifying the Localnet Docker image

The pipeline uses the latest `textile/recall-localnet` Docker image by default. If you want to use a different image,
perhaps a locally built one, you can specify it using the `--localnet-image` flag:

```bash
DAGGER_NO_NAG=1 \
DO_NOT_TRACK=1 \
dagger call test --progress plain \
  --source ../ \
  --localnet-image "textile/recall-localnet:sha-dc4da8c-3e80bf0" \
  --docker-username $DOCKER_USERNAME \
  --docker-password env://DOCKER_PASSWORD \
  2>&1 | grep -vi -E "resolve|containerd|libnetwork|client|daemon|checkpoint|task|^$"
```
