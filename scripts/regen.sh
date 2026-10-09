#!/usr/bin/env bash
# Write every generated file again from a fresh debug build of the server:
# the Sublime Text package's keys and settings, the Zed extension's rules
# and tasks, VS Code's package.json, and the generated parts of the docs.
# Run it after changing anything the server announces (commands, keys,
# settings, tokens) or scripts/features.mjs.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p org-lsp
server=target/debug/org-lsp
node editors/sublime/generate.mjs "$server"
node editors/zed/generate.mjs "$server"
(cd editors/vscode && npm run -s generate -- "../../$server" > /dev/null)
node scripts/generate-docs.mjs "$server"
