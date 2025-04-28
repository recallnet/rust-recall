#!/bin/bash

: "${RECALL_NETWORK:=localnet}"
: "${RECALL_NETWORK_CONFIG_FILE:=/tmp/networks.toml}"
: "${RECALL_CLI:=./target/release/recall}"
: "${RECALL_PRIVATE_KEY:=0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97}"

export RECALL_NETWORK
export RECALL_NETWORK_CONFIG_FILE
export RECALL_CLI
export RECALL_PRIVATE_KEY

for file in $(find tests/cli -type f | sort); do
    echo "Running test: $file"
    chmod +x "$file"

    if ! "$file"; then
        echo "Test failed: $file"
        exit 1
    fi
done

echo "All tests completed successfully!"
