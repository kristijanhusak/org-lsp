#!/usr/bin/env bash
# Regenerate server/org-syntax/src/entities.rs from the Org shipped with Emacs.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(emacs --batch -Q --eval '(progn (require (quote org)) (princ (org-version)))' 2>/dev/null)
emacs --batch -Q --eval '
(progn
  (require (quote org))
  (require (quote org-entities))
  (dolist (entity (append org-entities-user org-entities))
    (when (consp entity) (princ (format "%S\n" (car entity))))))' 2>/dev/null |
  python3 -c '
import json, sys
names = [json.loads(line) for line in sys.stdin if line.strip()]
plain = sorted({name for name in names if not name.startswith("_")})
spaces = max(len(name) - 1 for name in names if name.startswith("_"))
out = [
    "//! Names of the entities Org knows (`\\alpha`, `\\to` …), from `org-entities`.",
    "//!",
    "//! Generated from Org %s; regenerate with `scripts/update-entities.sh`." % sys.argv[1],
    "",
    "/// Sorted for binary search.",
    "pub(crate) const ENTITIES: &[&str] = &[",
]
line = "   "
for name in plain:
    piece = " \"%s\"," % name
    if len(line) + len(piece) > 100:
        out.append(line)
        line = "   "
    line += piece
out += [line, "];", "", "/// `\\_` followed by one to this many spaces is an entity too.",
        "pub(crate) const MAX_SPACE_ENTITY: usize = %d;" % spaces, ""]
sys.stdout.write("\n".join(out))
' "$version" > server/org-syntax/src/entities.rs
echo "wrote server/org-syntax/src/entities.rs (Org $version)"
