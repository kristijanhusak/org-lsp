#!/usr/bin/env bash
# Install the VS Code extension, with the server inside it.
#
#   scripts/install-vscode.sh               # the nightly build, for this machine
#   scripts/install-vscode.sh v0.2.0        # a release
#   scripts/install-vscode.sh --source      # built from this checkout
#
# The editor's command is `code`; set CODE for another one
# (CODE=codium, CODE=code-insiders, CODE=cursor). Run it again to update:
# the extension is replaced. Reload the VS Code window afterwards.
set -euo pipefail

repo=kristijanhusak/org-lsp
code=${CODE:-code}
release=nightly
source=false
for arg in "$@"; do
  case "$arg" in
    --source) source=true ;;
    -h | --help)
      sed -n '2,10s/^# \{0,1\}//p' "$0"
      exit 0
      ;;
    -*)
      echo "Unknown option: $arg" >&2
      exit 2
      ;;
    *) release=$arg ;;
  esac
done

if ! command -v "$code" > /dev/null; then
  echo "No \`$code\` command. In VS Code, run \"Shell Command: Install 'code' command in PATH\", or set CODE." >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if $source; then
  root=$(cd "$(dirname "$0")/.." && pwd)
  echo "Building the server"
  (cd "$root" && cargo build --release -p org-lsp)
  echo "Packaging the extension"
  cd "$root/editors/vscode"
  npm ci --silent
  npm run --silent build
  npm run --silent bundle-server
  npx --no-install vsce package --no-dependencies --out "$work/org-lsp.vsix"
  vsix=$work/org-lsp.vsix
else
  # VS Code's name for this platform, as the release names its packages.
  case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=darwin ;;
    MINGW* | MSYS* | CYGWIN*) os=win32 ;;
    *)
      echo "No package for $(uname -s); use --source." >&2
      exit 1
      ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) arch=x64 ;;
    arm64 | aarch64) arch=arm64 ;;
    *)
      echo "No package for $(uname -m); use --source." >&2
      exit 1
      ;;
  esac
  url=https://github.com/$repo/releases/download/$release/org-lsp-$os-$arch.vsix
  echo "Downloading $url"
  vsix=$work/org-lsp-$os-$arch.vsix
  if ! curl -fSL --progress-bar -o "$vsix" "$url"; then
    echo "Could not download it. Releases: https://github.com/$repo/releases; or use --source." >&2
    exit 1
  fi
fi

"$code" --install-extension "$vsix" --force
echo "Installed. Reload VS Code (\"Developer: Reload Window\") to use it."
