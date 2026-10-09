#!/usr/bin/env bash
# Run the Vim plugin's download test (download.vim), once per LSP client:
#
#   editors/vim/test/download.sh [lsp|vim_lsp|coc|ale ...]
#
# The plugin is copied without the checkout's build, and the server
# (ORG_LSP_BIN, default: the debug build) packed into a "release" that it
# downloads through a file:// URL. CLIENTS and VIM_BIN as for run.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
server="${ORG_LSP_BIN:-$root/target/debug/org-lsp}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

case "$(uname -sm)" in
  "Linux x86_64") platform=linux-x86_64 ;;
  "Linux aarch64" | "Linux arm64") platform=linux-arm64 ;;
  "Darwin arm64") platform=macos-arm64 ;;
  "Darwin x86_64") platform=macos-x86_64 ;;
  *) echo "no build for $(uname -sm)" >&2; exit 1 ;;
esac
mkdir -p "$work/releases/nightly" "$work/staged"
cp "$server" "$work/staged/org-lsp"
tar -czf "$work/releases/nightly/org-lsp-$platform.tar.gz" -C "$work/staged" org-lsp
cp -R "$root/editors/vim" "$work/plugin"

# No `org-lsp` on PATH.
path=""
IFS=: read -ra dirs <<< "$PATH"
for dir in "${dirs[@]}"; do
  [ -x "$dir/org-lsp" ] || path="${path:+$path:}$dir"
done

PATH="$path" ORG_LSP_TEST_FILE="$here/download.vim" ORG_LSP_TEST_PLUGIN="$work/plugin" \
  ORG_LSP_TEST_RELEASES="$work/releases" "$here/run.sh" "$@"
