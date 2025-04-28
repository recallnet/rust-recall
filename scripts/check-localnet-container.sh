#!/bin/bash

CONTAINER_NAME="recall-localnet"
SEARCH_STRING="All containers started. Waiting for termination signal..."
MAX_WAIT_TIME=300
INTERVAL=5

start_time=$(date +%s)

while true; do
    if docker logs $CONTAINER_NAME 2>&1 | grep -q "$SEARCH_STRING"; then
        echo -e "\nLocalnet container ready!"
        exit 0
    fi

    current_time=$(date +%s)
    elapsed_time=$((current_time - start_time))
    if [ $elapsed_time -ge $MAX_WAIT_TIME ]; then
        echo -e "\nLocalnet container startup failed!"
        exit 1
    fi

    remaining=$((MAX_WAIT_TIME - elapsed_time))
    echo -ne "Waiting for Localnet container... ${elapsed_time}s elapsed, ${remaining}s remaining\r"

    sleep $INTERVAL
done
