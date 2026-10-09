#!/usr/bin/env bash
# The fast checks, as CI runs them, minus the editors' own end-to-end tests
# (scripts/test-all.sh): formatting, clippy, the Rust tests, the generated
# files against the server, VS Code's type check and the Sublime Text
# plugin's tests.
#
# With Emacs installed, the Rust tests that need it run against the Org of
# ORG_LISP_DIR (the `lisp` directory of an Org 9.8.10 checkout); a few of
# them fail against an older Org that Emacs ships with.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -n "${ORG_LISP_DIR:-}" ] || echo "note: ORG_LISP_DIR is not set; tests that run Emacs use the Org it ships with" >&2
cargo fmt --all -- --check
cargo clippy -q --workspace --all-targets -- -D warnings
cargo test -q --workspace
server=target/debug/org-lsp
node scripts/generate-docs.mjs "$server" --check
node editors/sublime/generate.mjs "$server" --check
node editors/zed/generate.mjs "$server" --check
(cd editors/vscode && npm run -s generate -- "../../$server" --check && npm run -s check)
python3 editors/sublime/test/test_plugin.py
echo "all checks passed"
