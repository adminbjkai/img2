#!/usr/bin/env bash
set -euo pipefail
cd /apps/img2
HOST=127.0.0.1 PORT=8127 ./target/release/img2
