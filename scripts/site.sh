#!/usr/bin/env bash
# The documentation website (site/), written from the Org docs into
# target/site; with --serve, shown at http://localhost:8000 as well.
# Fails on a link to a file or a heading that is not there.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -q -p org-site -- --check --out target/site
if [ "${1:-}" = "--serve" ]; then
  echo "http://localhost:${PORT:-8000}"
  python3 -m http.server -d target/site "${PORT:-8000}"
fi
