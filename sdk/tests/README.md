# Integration Tests

This directory contains integration tests for the sdk.  These run against the Hoku network defined by the following env vars:
  - `HOKU_PRIVATE_KEY`, a private key for a wallet that has funds on the parent chain, HOKU, and credits
  - `TEST_TARGET_NETWORK`, one of `localnet` or `testnet`. (eventually `mainnet` may be supported)
  - `HOKU_AUTH_TOKEN`, Optional evm subnet auth token, if needed for the given target network.

An example of running these tests against localnet Anvil default account 8 follows:
`TEST_TARGET_NETWORK=localnet HOKU_PRIVATE_KEY=0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97 cargo test --test '*' --nocapture --ignored`
