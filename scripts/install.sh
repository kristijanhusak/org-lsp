#!/usr/bin/env bash
# Install org-lsp for an editor: the server, and the editor's extension or
# package, downloaded from a GitHub release. Neovim and Vim need none of
# this: their plugins download the server themselves.
#
#   scripts/install.sh vscode      VS Code, the server inside the extension
#   scripts/install.sh sublime     Sublime Text: LSP-org, LSP, the server
#   scripts/install.sh zed         Zed: the extension, the server, its tasks and keys
#   scripts/install.sh helix       Helix: the server, Org in languages.toml, its colors
#   scripts/install.sh server      the server alone
#
# Options, before the editor:
#   --release v0.2.0   a release other than the nightly build
#   --source           the server built from this checkout (cargo), and the
#                      VS Code extension too (npm); the Sublime Text package
#                      linked from it
#
# Without a checkout:
#   curl -fsSL https://github.com/kristijanhusak/org-lsp/releases/download/nightly/install.sh | bash -s -- zed
#
# Run it again to update. The server goes in ~/.local/bin (ORG_LSP_BIN_DIR
# for another directory), Zed's in the extension's own directory. VS Code's
# command is `code`; CODE names another (codium, code-insiders, cursor).
# Downloads that need a GitHub login, as from a private repository, go
# through the GitHub CLI (`gh`) when it is installed.
#
# Only functions are defined until the last line, which runs `main`: a
# download cut short (`curl | bash`) does nothing.
set -euo pipefail

repo=kristijanhusak/org-lsp
releases=${ORG_LSP_RELEASES:-https://github.com/$repo/releases/download}
# The comment at the top; through a pipe (`curl | bash`) there is no file
# to read it from.
usage() {
  if [ -f "${BASH_SOURCE[0]:-}" ]; then
    sed -n '2,25s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"
  else
    say "Usage: install.sh [--release TAG] [--source] vscode|sublime|zed|helix|server"
    say "See https://github.com/$repo/blob/main/scripts/install.sh for more."
  fi
}
say() { printf '%s\n' "$*"; }
fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

# Download the release's file `name` into `dest`, checked against the
# release's SHA256SUMS when it has them.
fetch() {
  local name=$1 dest=$2
  say "Downloading $name ($release)"
  if ! curl -fsSL --retry 2 -o "$dest" "$releases/$release/$name" 2> "$work/curl.log"; then
    # A private repository's files need GitHub's login, which `gh` has.
    if [ "$releases" = "https://github.com/$repo/releases/download" ] && command -v gh > /dev/null &&
      gh release download "$release" --repo "$repo" --pattern "$name" --dir "$work/gh" --clobber 2> "$work/gh.log"; then
      mv "$work/gh/$name" "$dest"
    else
      cat "$work/curl.log" "$work/gh.log" 2> /dev/null >&2 || true
      fail "Could not download $name from the $release release ($releases/$release). Releases: https://github.com/$repo/releases"
    fi
  fi
  # A release without SHA256SUMS is not checked; `SHA256SUMS.none` says
  # it has none, so that it is asked once.
  if [ ! -f "$work/SHA256SUMS" ] && [ ! -f "$work/SHA256SUMS.none" ]; then
    curl -fsSL -o "$work/SHA256SUMS" "$releases/$release/SHA256SUMS" 2> /dev/null ||
      { command -v gh > /dev/null && gh release download "$release" --repo "$repo" --pattern SHA256SUMS --dir "$work" --clobber 2> /dev/null; } ||
      { rm -f "$work/SHA256SUMS" && : > "$work/SHA256SUMS.none"; }
  fi
  [ -f "$work/SHA256SUMS" ] || return 0
  local sum
  sum=$(awk -v name="$name" '$2 == name || $2 == "*" name { print $1 }' "$work/SHA256SUMS")
  # The files built together are all in SHA256SUMS: one that is not is
  # left from an older build (a platform whose build failed).
  [ -n "$sum" ] ||
    fail "$name is not in the SHA256SUMS of the $release release: it is not from the same build, so it is not installed. Releases: https://github.com/$repo/releases"
  local actual
  if command -v sha256sum > /dev/null; then
    actual=$(sha256sum "$dest" | awk '{ print $1 }')
  elif command -v shasum > /dev/null; then
    actual=$(shasum -a 256 "$dest" | awk '{ print $1 }')
  else
    return 0
  fi
  [ "$actual" = "$sum" ] || fail "$name does not match its checksum in SHA256SUMS: the download is broken."
}

# Unpack the archive `file` into the directory `dest`.
unpack() {
  local file=$1 dest=$2
  mkdir -p "$dest"
  case "$file" in
    *.zip)
      if command -v unzip > /dev/null; then
        unzip -qo "$file" -d "$dest"
      else
        powershell.exe -NoProfile -Command "Expand-Archive -Force -LiteralPath '$file' -DestinationPath '$dest'"
      fi
      ;;
    *) tar -xzf "$file" -C "$dest" ;;
  esac
}

# Put the server in the directory `dest`; its path is printed.
install_server() {
  local dest=$1
  mkdir -p "$dest"
  if $source; then
    say "Building the server" >&2
    (cd "$root" && cargo build --release -p org-lsp >&2)
    cp "$root/target/release/$exe" "$work/$exe"
  else
    [ "$os" != unknown ] && [ "$arch" != unknown ] && [ "$platform" != windows-arm64 ] ||
      fail "There is no build for $(uname -sm); build it in a checkout with --source."
    local archive=org-lsp-$platform.tar.gz
    [ "$os" = windows ] && archive=org-lsp-$platform.zip
    fetch "$archive" "$work/$archive" >&2
    unpack "$work/$archive" "$work/server"
    mv "$work/server/$exe" "$work/$exe"
  fi
  chmod 755 "$work/$exe"
  # A running server cannot be replaced on Windows, but it can be renamed.
  rm -f "$dest/$exe.old"
  [ -e "$dest/$exe" ] && mv "$dest/$exe" "$dest/$exe.old"
  mv "$work/$exe" "$dest/$exe"
  rm -f "$dest/$exe.old" 2> /dev/null || true
  printf '%s\n' "$dest/$exe"
}

# The server where every editor looks: ~/.local/bin.
install_server_on_path() {
  local dir=${ORG_LSP_BIN_DIR:-$HOME/.local/bin} path
  path=$(install_server "$dir")
  say "Installed the server: $path ($("$path" --version 2> /dev/null || echo "$release"))"
  # Sublime Text's package looks in ~/.local/bin itself, and Helix is
  # given the whole path.
  if [ "$editor" = server ]; then
    case ":$PATH:" in
      *":$dir:"*) ;;
      *) say "$dir is not on your PATH: add it, so that editors find the server." ;;
    esac
  fi
}

# The entries of the JSON array in `entries` (a file of the release or the
# checkout), put at the start of the JSON array in `file`, between two
# comments, in place of what an earlier run put there. Zed reads JSON with
# comments, which a JSON tool would drop: the file is changed as text.
merge_json_array() {
  local file=$1 entries=$2
  local begin='// BEGIN org-lsp: written by scripts/install.sh, which replaces it when run again'
  local end='// END org-lsp'
  local body
  body=$(sed '1d;$d' "$entries")
  mkdir -p "$(dirname "$file")"
  if [ ! -s "$file" ]; then
    printf '[\n%s\n%s\n%s\n]\n' "$begin" "$body" "$end" > "$file"
    return
  fi
  # What an earlier run put there, out.
  awk -v begin="$begin" -v end="$end" '
    $0 == begin { skip = 1; next }
    skip && $0 == end { skip = 0; next }
    !skip
  ' "$file" > "$work/merged"
  # The first `[` outside a comment opens the array; whether anything but
  # blanks and comments follows it decides the comma after the entries.
  local line n=0 at=0 rest=
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    [[ "$line" =~ ^[[:space:]]*// ]] && continue
    if [ "$at" -gt 0 ]; then
      rest+=$line
    elif [[ "$line" == *\[* ]]; then
      at=$n
      rest=${line#*\[}
    fi
  done < "$work/merged"
  [ "$at" -gt 0 ] || fail "$file has no JSON array; add the entries of $entries to it by hand."
  rest=${rest#"${rest%%[![:space:]]*}"}
  local comma=,
  [ "${rest:0:1}" = "]" ] && comma=
  # The entries through the environment: `awk -v` would read their
  # backslashes as escapes.
  ORG_LSP_ENTRIES=$body awk -v at="$at" -v begin="$begin" -v end="$end" -v comma="$comma" '
    NR == at {
      i = index($0, "[")
      print substr($0, 1, i)
      print begin
      print ENVIRON["ORG_LSP_ENTRIES"] comma
      print end
      rest = substr($0, i + 1)
      if (rest ~ /[^[:space:]]/) print rest
      next
    }
    { print }
  ' "$work/merged" > "$file"
}

# Install the VS Code extension.
install_vscode() {
  local code=${CODE:-code} vsix
  command -v "$code" > /dev/null ||
    fail "No \`$code\` command. In VS Code, run \"Shell Command: Install 'code' command in PATH\", or set CODE."
  if $source; then
    say "Building the server"
    (cd "$root" && cargo build --release -p org-lsp)
    say "Packaging the extension"
    (
      cd "$root/editors/vscode"
      npm ci --silent
      npm run --silent build
      npm run --silent bundle-server
      npx --no-install vsce package --no-dependencies --baseContentUrl "https://github.com/$repo/blob/HEAD/editors/vscode" --baseImagesUrl "https://github.com/$repo/raw/HEAD/editors/vscode" --out "$work/org-lsp.vsix"
    )
    vsix=$work/org-lsp.vsix
  else
    # VS Code's name for this platform, as the release names its packages.
    local target
    case "$platform" in
      linux-x86_64) target=linux-x64 ;;
      linux-arm64) target=linux-arm64 ;;
      macos-x86_64) target=darwin-x64 ;;
      macos-arm64) target=darwin-arm64 ;;
      windows-x86_64) target=win32-x64 ;;
      *) fail "There is no package for $(uname -sm); build it in a checkout with --source." ;;
    esac
    vsix=$work/org-lsp-$target.vsix
    fetch "org-lsp-$target.vsix" "$vsix"
  fi
  "$code" --install-extension "$vsix" --force
  say "Installed. Reload VS Code (\"Developer: Reload Window\") to use it."
}

# Sublime Text's Packages folder (4, then 3).
sublime_packages() {
  local candidate candidates
  if [ -n "${SUBLIME_PACKAGES:-}" ]; then
    printf '%s\n' "$SUBLIME_PACKAGES"
    return
  fi
  case "$os" in
    macos) candidates=("$HOME/Library/Application Support/Sublime Text/Packages" "$HOME/Library/Application Support/Sublime Text 3/Packages") ;;
    windows) candidates=("$APPDATA/Sublime Text/Packages" "$APPDATA/Sublime Text 3/Packages") ;;
    *) candidates=("${XDG_CONFIG_HOME:-$HOME/.config}/sublime-text/Packages" "${XDG_CONFIG_HOME:-$HOME/.config}/sublime-text-3/Packages" "$HOME/.var/app/com.sublimetext.three/config/sublime-text/Packages") ;;
  esac
  for candidate in "${candidates[@]}"; do
    if [ -d "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
}

# Install the Sublime Text package, LSP and the server.
install_sublime() {
  local packages
  packages=$(sublime_packages)
  [ -n "$packages" ] || fail "No Sublime Text Packages folder found. Start Sublime Text once, or set SUBLIME_PACKAGES (Preferences > Browse Packages... shows it)."
  local target=$packages/LSP-org
  if $source; then
    rm -rf "$target"
    ln -s "$root/editors/sublime/LSP-org" "$target"
    say "Linked $target to $root/editors/sublime/LSP-org"
  else
    # The package that is there stays until the new one has come.
    fetch org-lsp-sublime.zip "$work/org-lsp-sublime.zip"
    rm -rf "$target"
    unpack "$work/org-lsp-sublime.zip" "$packages"
    say "Installed the package in $target"
  fi

  # LSP, through Package Control's list of installed packages.
  local user=$packages/User installed
  installed=$(dirname "$packages")/Installed\ Packages
  if [ -f "$installed/LSP.sublime-package" ] || [ -d "$packages/LSP" ]; then
    say "LSP is installed."
  elif [ -f "$installed/Package Control.sublime-package" ] || [ -d "$packages/Package Control" ]; then
    local control=$user/Package\ Control.sublime-settings
    command -v python3 > /dev/null ||
      fail "Install LSP with Package Control (Package Control: Install Package > LSP), then run this again."
    mkdir -p "$user"
    python3 - "$control" << 'PY'
import json, os, sys
path = sys.argv[1]
settings = {}
if os.path.exists(path):
    with open(path, encoding='utf-8') as handle:
        settings = json.load(handle)
packages = settings.setdefault('installed_packages', [])
if 'LSP' not in packages:
    packages.append('LSP')
    packages.sort(key=str.lower)
with open(path, 'w', encoding='utf-8') as handle:
    json.dump(settings, handle, indent='\t')
    handle.write('\n')
PY
    say "LSP is added to Package Control's packages; it is installed when Sublime Text starts."
  else
    say "Install LSP with Package Control (Package Control: Install Package > LSP)."
  fi

  install_server_on_path
  say "Restart Sublime Text, then open an .org file."
}

# Zed's data and configuration directories.
zed_dirs() {
  case "$os" in
    macos)
      zed_data=${ZED_DATA:-$HOME/Library/Application Support/Zed}
      zed_config=${ZED_CONFIG:-$HOME/.config/zed}
      ;;
    windows)
      zed_data=${ZED_DATA:-$LOCALAPPDATA/Zed}
      zed_config=${ZED_CONFIG:-$APPDATA/Zed}
      ;;
    *)
      zed_data=${ZED_DATA:-${FLATPAK_XDG_DATA_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}}/zed}
      zed_config=${ZED_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/zed}
      ;;
  esac
}

# Install the Zed extension, the server beside it, and its tasks and keys.
install_zed() {
  local zed_data zed_config
  zed_dirs
  # The extension as Zed installs one from its registry: unpacked in
  # `installed`, which Zed watches. A dev extension of the same name goes.
  fetch org-lsp-zed.tar.gz "$work/org-lsp-zed.tar.gz"
  local extension=$zed_data/extensions/installed/org-lsp
  rm -rf "$extension"
  unpack "$work/org-lsp-zed.tar.gz" "$extension"
  say "Installed the extension in $extension"

  local server
  server=$(install_server "$zed_data/extensions/work/org-lsp")
  say "Installed the server: $server"

  # Its tasks and keys, from the release's files or the checkout's.
  local file
  for file in tasks keymap; do
    if $source; then
      cp "$root/editors/zed/$file.json" "$work/zed-$file.json"
    else
      fetch "org-lsp-zed-$file.json" "$work/zed-$file.json"
    fi
    merge_json_array "$zed_config/$file.json" "$work/zed-$file.json"
    say "Added Org's ${file/keymap/keys} to $zed_config/$file.json"
  done
  say "Restart Zed, then open an .org file."
}

# Install the server, Org in Helix's languages.toml, and Org's colors in
# Helix's runtime.
install_helix() {
  install_server_on_path
  local dir=${ORG_LSP_BIN_DIR:-$HOME/.local/bin}
  local config=${HELIX_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/helix}
  [ "$os" = windows ] && config=${HELIX_CONFIG:-$APPDATA/helix}
  local file=$config/languages.toml
  local begin='# BEGIN org-lsp: written by scripts/install.sh, which replaces it when run again'
  local end='# END org-lsp'
  mkdir -p "$config"
  touch "$file"
  awk -v begin="$begin" -v end="$end" '
    $0 == begin { skip = 1; next }
    skip && $0 == end { skip = 0; next }
    !skip
  ' "$file" > "$work/languages.toml"
  local block
  # What Helix does without telling the server: it shows documents, and
  # no lists (docs/helix.org). Options of one's own go in the shared
  # configuration file, as this block is written again. The agenda is a
  # language of its own, `org-agenda`, for its colors.
  block=$(printf '%s\n[language-server.org-lsp]\ncommand = "%s"\n\n[language-server.org-lsp.config]\nshow_document = true\nmessage_requests = false\n\n[[language]]\nname = "org"\nfile-types = ["org", "org_archive"]\nlanguage-servers = ["org-lsp"]\n\n[[language]]\nname = "org-agenda"\nscope = "source.org-agenda"\ngrammar = "org"\nlanguage-id = "org"\nfile-types = [{ glob = "org-lsp-view-*/Agenda.org" }]\nlanguage-servers = ["org-lsp"]\n%s' "$begin" "$dir/$exe" "$end")
  if grep -Eq '^\[language-server\.org-lsp\]|^name *= *"org"' "$work/languages.toml"; then
    say "$file sets up Org already; leaving it as it is. What it needs:"
    say "$block"
  else
    { cat "$work/languages.toml"; [ -s "$work/languages.toml" ] && echo; printf '%s\n' "$block"; } > "$file"
    say "Added Org to $file"
  fi

  # Org's colors: the queries of editors/helix/runtime, which Helix reads
  # in its configuration's runtime directory before its own. They replace
  # those of an earlier install.
  if $source; then
    cp -R "$root/editors/helix/runtime" "$work/helix"
  elif (fetch org-lsp-helix.tar.gz "$work/org-lsp-helix.tar.gz"); then
    unpack "$work/org-lsp-helix.tar.gz" "$work/helix"
  else
    say "The $release release has no colors for Helix; none were installed."
    return
  fi
  local language
  mkdir -p "$config/runtime/queries"
  for language in org org-agenda; do
    rm -rf "${config:?}/runtime/queries/$language"
    cp -R "$work/helix/queries/$language" "$config/runtime/queries/$language"
  done
  say "Added Org's colors to $config/runtime/queries"
}

main() {
  release=nightly
  source=false
  editor=
  while [ $# -gt 0 ]; do
    case "$1" in
      --release)
        release=${2:?--release needs a release, such as v0.2.0}
        shift
        ;;
      --release=*) release=${1#*=} ;;
      --source) source=true ;;
      -h | --help)
        usage
        exit 0
        ;;
      vscode | sublime | zed | helix | server) editor=$1 ;;
      *)
        echo "Unknown argument: $1 (see --help)" >&2
        exit 2
        ;;
    esac
    shift
  done
  if [ -z "$editor" ]; then
    usage >&2
    exit 2
  fi

  # The checkout this script is in, if it is in one.
  root=
  if [ -f "${BASH_SOURCE[0]:-}" ]; then
    local dir
    dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
    [ -f "$dir/server/org-lsp/Cargo.toml" ] && root=$dir
  fi
  if $source && [ -z "$root" ]; then
    fail "--source needs a checkout of the repository: run the script from it."
  fi

  # This platform, as the release names its files.
  case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=macos ;;
    MINGW* | MSYS* | CYGWIN*) os=windows ;;
    *) os=unknown ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=arm64 ;;
    *) arch=unknown ;;
  esac
  platform=$os-$arch
  exe=org-lsp
  [ "$os" = windows ] && exe=org-lsp.exe

  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT

  case "$editor" in
    vscode) install_vscode ;;
    sublime) install_sublime ;;
    zed) install_zed ;;
    helix) install_helix ;;
    server) install_server_on_path ;;
  esac
}

main "$@"
