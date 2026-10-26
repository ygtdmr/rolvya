#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
cargo test -p rolvya-persistence-sqlite --test migration_replay --locked
