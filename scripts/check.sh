#!/usr/bin/env bash
# The fast checks, as CI runs them, minus the editors' own end-to-end tests
# (scripts/test-all.sh): formatting, clippy, the Rust tests, the generated
# files against the server, VS Code's type check, the Sublime Text
# plugin's tests and the links of the docs (the website).
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
(cd editors/vscode && npm run -s generate -- "../../$server" --check && npm run -s check && node scripts/readme.mjs --check)
python3 editors/sublime/test/test_plugin.py
# The website: every link of the docs leads to a file or a heading there.
cargo run -q -p org-site -- --check --out target/site > /dev/null
echo "all checks passed"
