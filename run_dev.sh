#!/usr/bin/env bash
set -e

# Source - https://stackoverflow.com/a/2173421
# Retrieved 2026-04-19, License - CC BY-SA 4.0
trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

(cd client && npm run start)&
RUST_LOG=debug cargo watch --why -x 'run 0.0.0.0:8000 sqlite://decide.sqlite'&
wait
