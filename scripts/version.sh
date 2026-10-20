#!/usr/bin/env bash
# The version of org-lsp, in every place it is written: the server's
# workspace (Cargo.toml and Cargo.lock), the VS Code extension
# (package.json and package-lock.json), the Zed extension (extension.toml,
# Cargo.toml and Cargo.lock) and the Sublime Text package (`VERSION` in
# plugin.py). The Zed extension and the Sublime Text package download the
# server of the release named after it (`v0.2.0`).
#
#   scripts/version.sh 0.2.0    set it everywhere
#   scripts/version.sh          print it; fails when the places disagree
#
# A release is then the tag `v0.2.0` pushed to GitHub (docs/development.org,
# Publishing).
set -euo pipefail
cd "$(dirname "$0")/.."

# The version each place has, one `place version` a line.
versions() {
  printf 'Cargo.toml %s\n' "$(awk '/^\[/ { section = $0 } section == "[workspace.package]" && /^version = / { gsub(/"/, "", $3); print $3; exit }' Cargo.toml)"
  local crate
  for crate in org-syntax org-core org-lsp org-site; do
    printf 'Cargo.lock(%s) %s\n' "$crate" "$(lock_version Cargo.lock "$crate")"
  done
  node -e '
    const fs = require("fs");
    const pkg = JSON.parse(fs.readFileSync("editors/vscode/package.json", "utf8"));
    const lock = JSON.parse(fs.readFileSync("editors/vscode/package-lock.json", "utf8"));
    console.log("editors/vscode/package.json " + pkg.version);
    console.log("editors/vscode/package-lock.json " + lock.version);
    console.log("editors/vscode/package-lock.json(packages) " + lock.packages[""].version);
  '
  printf 'editors/zed/extension.toml %s\n' "$(awk '/^\[/ { exit } /^version = / { gsub(/"/, "", $3); print $3; exit }' editors/zed/extension.toml)"
  printf 'editors/zed/Cargo.toml %s\n' "$(awk '/^\[/ { section = $0 } section == "[package]" && /^version = / { gsub(/"/, "", $3); print $3; exit }' editors/zed/Cargo.toml)"
  printf 'editors/zed/Cargo.lock %s\n' "$(lock_version editors/zed/Cargo.lock zed_org_lsp)"
  printf 'editors/sublime/LSP-org/plugin.py %s\n' "$(sed -n "s/^VERSION = '\(.*\)'$/\1/p" editors/sublime/LSP-org/plugin.py)"
}

# The version of the package `name` in the Cargo.lock `file`.
lock_version() {
  awk -v name="$2" '$0 == "name = \"" name "\"" { found = 1; next } found && /^version = / { gsub(/"/, "", $3); print $3; exit }' "$1"
}

if [ $# -eq 0 ]; then
  all=$(versions)
  first=$(head -n 1 <<< "$all" | awk '{ print $2 }')
  if awk -v want="$first" '$2 != want { bad = 1 } END { exit !bad }' <<< "$all"; then
    echo "The versions disagree; set them with scripts/version.sh X.Y.Z:" >&2
    printf '%s\n' "$all" >&2
    exit 1
  fi
  echo "$first"
  exit 0
fi

version=$1
# Only X.Y.Z: the VS Code Marketplace takes no other.
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
  { echo "Not a version (X.Y.Z): $version" >&2; exit 1; }

# The first `version = "…"` of a TOML file's `section`.
set_toml() {
  local file=$1 section=$2
  awk -v section="$section" -v version="$version" '
    /^\[/ { current = $0 }
    !done && current == section && /^version = / { print "version = \"" version "\""; done = 1; next }
    { print }
  ' "$file" > "$file.tmp"
  mv "$file.tmp" "$file"
}
set_toml Cargo.toml '[workspace.package]'
set_toml editors/zed/Cargo.toml '[package]'
# extension.toml's version comes before any section.
set_toml editors/zed/extension.toml ''
sed -i.bak "s/^VERSION = '.*'$/VERSION = '$version'/" editors/sublime/LSP-org/plugin.py
rm editors/sublime/LSP-org/plugin.py.bak
# The JSON files' own top-level lines, so that nothing else in them moves.
node -e '
  const fs = require("fs");
  const version = process.argv[1];
  const set = (file, pattern) => {
    const text = fs.readFileSync(file, "utf8");
    if (!pattern.test(text)) throw new Error("no version in " + file);
    fs.writeFileSync(file, text.replace(pattern, (_, before) => before + JSON.stringify(version)));
  };
  set("editors/vscode/package.json", /^(  "version": )"[^"]*"/m);
  set("editors/vscode/package-lock.json", /^(  "version": )"[^"]*"/m);
  set("editors/vscode/package-lock.json", /^(    "": \{\n(?:      .*\n)*?      "version": )"[^"]*"/m);
' "$version"
# The lock files follow, without fetching anything.
cargo update -q -w --offline
cargo update -q -w --offline --manifest-path editors/zed/Cargo.toml
"$0"
