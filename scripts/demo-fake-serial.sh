#!/bin/sh
cd "$(dirname "$0")/.." || exit 1
exec cargo run -p helm-cli --features dashboard,hardware -- --demo --plant fake-serial "$@"
