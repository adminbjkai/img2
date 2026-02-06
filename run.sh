#!/usr/bin/env bash
set -euo pipefail
cd /apps/img2
HOST="${HOST:-127.0.0.1}" PORT="${PORT:-8097}" ./target/release/img2
