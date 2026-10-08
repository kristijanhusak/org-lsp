#!/usr/bin/env bash
# Regenerate server/org-syntax/src/entities.rs from Org: the version bundled
# with Emacs, or the checkout whose `lisp` directory ORG_LISP_DIR names.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(emacs --batch -Q ${ORG_LISP_DIR:+-L "$ORG_LISP_DIR"} --eval '(progn (require (quote org)) (princ (org-version)))' 2>/dev/null)
emacs --batch -Q ${ORG_LISP_DIR:+-L "$ORG_LISP_DIR"} --eval '
(progn
  (require (quote org))
  (require (quote org-entities))
  (require (quote json))
  (dolist (entity (append org-entities-user org-entities))
    (when (consp entity) (princ (concat (json-encode (list (car entity) (nth 6 entity))) "\n")))))' 2>/dev/null |
  python3 -c '
import json, sys
pairs = [json.loads(line) for line in sys.stdin if line.strip()]
names = [name for name, _ in pairs]
utf8_of = dict(pairs)
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
out += [line, "];", "", "/// What each of [`ENTITIES`] is in UTF-8, in the same order.", "pub(crate) const UTF8: &[&str] = &["]
line = "   "
for name in plain:
    piece = " %s," % json.dumps(utf8_of[name], ensure_ascii=False)
    if len(line) + len(piece) > 100:
        out.append(line)
        line = "   "
    line += piece
out += [line, "];", "", "/// `\\_` followed by one to this many spaces is an entity too.",
        "pub(crate) const MAX_SPACE_ENTITY: usize = %d;" % spaces, ""]
sys.stdout.write("\n".join(out))
' "$version" > server/org-syntax/src/entities.rs
echo "wrote server/org-syntax/src/entities.rs (Org $version)"
