#!/usr/bin/env bash
# Install the Sublime Text package (LSP-org) from this checkout.
#
#   scripts/install-sublime.sh            # link the package into Packages
#   scripts/install-sublime.sh --copy     # copy it instead (run again to update)
#
# The package needs sublimelsp/LSP: when Package Control is installed, LSP
# is added to its list and installed the next time Sublime Text starts.
# The server is found on PATH (and in ~/.cargo/bin, ~/.local/bin), in this
# checkout's target/ when the package is linked, or else downloaded once
# (the nightly build). Set SUBLIME_PACKAGES for an unusual Packages folder.
set -euo pipefail

copy=false
for arg in "$@"; do
  case "$arg" in
    --copy) copy=true ;;
    -h | --help)
      sed -n '2,11s/^# \{0,1\}//p' "$0"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

root=$(cd "$(dirname "$0")/.." && pwd)
package=$root/editors/sublime/LSP-org

# Sublime Text's Packages folder (4, then 3).
packages=${SUBLIME_PACKAGES:-}
if [ -z "$packages" ]; then
  case "$(uname -s)" in
    Darwin) candidates=("$HOME/Library/Application Support/Sublime Text/Packages" "$HOME/Library/Application Support/Sublime Text 3/Packages") ;;
    MINGW* | MSYS* | CYGWIN*) candidates=("$APPDATA/Sublime Text/Packages" "$APPDATA/Sublime Text 3/Packages") ;;
    *) candidates=("${XDG_CONFIG_HOME:-$HOME/.config}/sublime-text/Packages" "${XDG_CONFIG_HOME:-$HOME/.config}/sublime-text-3/Packages" "$HOME/.var/app/com.sublimetext.three/config/sublime-text/Packages") ;;
  esac
  for candidate in "${candidates[@]}"; do
    if [ -d "$candidate" ]; then
      packages=$candidate
      break
    fi
  done
fi
if [ -z "$packages" ]; then
  echo "No Sublime Text Packages folder found. Start Sublime Text once, or set SUBLIME_PACKAGES" >&2
  echo "(Preferences > Browse Packages... shows it)." >&2
  exit 1
fi

target=$packages/LSP-org
if [ -e "$target" ] || [ -L "$target" ]; then
  rm -rf "$target"
fi
if $copy; then
  cp -R "$package" "$target"
  rm -rf "$target/__pycache__"
  echo "Copied the package to $target"
else
  ln -s "$package" "$target"
  echo "Linked $target to $package"
fi

# LSP, through Package Control's list of installed packages.
user=$packages/User
control=$user/Package\ Control.sublime-settings
installed=$(dirname "$packages")/Installed\ Packages
if [ -f "$installed/LSP.sublime-package" ] || [ -d "$packages/LSP" ]; then
  echo "LSP is installed."
elif [ -f "$installed/Package Control.sublime-package" ] || [ -d "$packages/Package Control" ]; then
  mkdir -p "$user"
  python3 - "$control" <<'PY'
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
  echo "LSP is added to Package Control's packages; it is installed when Sublime Text starts."
else
  echo "Install LSP with Package Control (Package Control: Install Package > LSP)."
fi

echo "Restart Sublime Text, then open an .org file."
