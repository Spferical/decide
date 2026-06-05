#!/usr/bin/env bash
set -e

# Configuration
DB_FILE="/tmp/decide_e2e_$(date +%s).sqlite"
ADDR="127.0.0.1:8001"
# Ensure we build the frontend first to have assets
pushd client
npm run build
popd

echo "Starting test server..."
cargo run --manifest-path ../decide/Cargo.toml -- $ADDR "sqlite://$DB_FILE" &
SERVER_PID=$!

echo "Waiting for server to start..."
until curl -s http://$ADDR > /dev/null; do sleep 1; done

echo "Running Playwright tests..."
cd client
npx playwright test

echo "Cleaning up..."
kill $SERVER_PID
rm $DB_FILE
