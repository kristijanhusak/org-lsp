#!/usr/bin/env bash
# vx.sh CASES.cjs OUT.json SETTINGS.json: the typed explorer, in an Xvfb of
# its own (display $DISP, default 72).
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here" || exit 1
export ORG_LSP_BIN="${ORG_LSP_BIN:-/home/user/org-lsp/target/debug/org-lsp}" VSCODE_EXECUTABLE="${VSCODE_EXECUTABLE:-/root/vscodium/codium}"
exec timeout -k 10 "${TMO:-1500}" xvfb-run -n "${DISP:-72}" -s "-screen 0 1600x1000x24" node run.mjs "$@"
