#!/bin/bash

# Start the Dagger binary in the background and redirect its output to a file
go run ./cmd/ci/main.go > output.log 2>&1 &

PID=$!

START_TIME=$(date +%s)
TIMEOUT=300

echo "Process started with PID: $PID"
echo "Waiting for service to be ready (timeout: 5 minutes)..."
while ! grep -q "All containers started" output.log; do
  if ! ps -p $PID > /dev/null; then
    echo "Error: Process died before becoming ready"
    cat output.log
    exit 1
  fi
  
  CURRENT_TIME=$(date +%s)
  ELAPSED_TIME=$((CURRENT_TIME - START_TIME))
  if [ $ELAPSED_TIME -gt $TIMEOUT ]; then
    echo "Error: Timed out after 5 minutes waiting for service to be ready"
    kill $PID 2>/dev/null || true
    # Wait for process to actually terminate
    sleep 2
    if ps -p $PID > /dev/null; then
      echo "Process didn't terminate, trying SIGKILL..."
      kill -9 $PID 2>/dev/null || true
    fi
    cat output.log
    exit 1
  fi
  
  if [ $((ELAPSED_TIME % 30)) -eq 0 ] && [ $ELAPSED_TIME -ne 0 ]; then
    echo "Still waiting... ($ELAPSED_TIME seconds elapsed)"
  fi
  
  sleep 1
done

ELAPSED_TIME=$(($(date +%s) - START_TIME))
echo "Service is now ready! (took $ELAPSED_TIME seconds)"

# Provide instructions for clean shutdown
echo ""
echo "To stop the service cleanly, run: kill -s SIGTERM $PID"
echo "The service will be running until you terminate it."
