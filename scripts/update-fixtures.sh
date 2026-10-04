#!/usr/bin/env bash
# Regenerate the org-element reference dumps next to each fixture.
# Requires Emacs; the dumps are committed so the tests run without it.
set -euo pipefail
cd "$(dirname "$0")/.."
for file in tests/fixtures/*.org; do
  emacs --batch -Q -l scripts/org-element-dump.el "$file" > "${file%.org}.elements"
  echo "wrote ${file%.org}.elements"
done
