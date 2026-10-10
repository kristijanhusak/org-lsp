#!/usr/bin/env bash
# Write what Emacs's calendar shows around a few months, for the test of
# the server's date prompt (server/org-core/src/calendar.rs): weeks
# starting on Sunday in emacs.txt, on Monday (`calendar-week-start-day` 1)
# in emacs-monday.txt. Trailing blanks are dropped. Requires Emacs.
set -euo pipefail
cd "$(dirname "$0")/.."
for start in 0 1; do
  out=tests/calendar/emacs.txt
  [ "$start" = 1 ] && out=tests/calendar/emacs-monday.txt
  : > "$out"
  for month in "10 2026" "2 2026" "1 2027" "12 2026" "3 2024" "8 2026" "5 2026"; do
    set -- $month
    echo "== $1 $2" >> "$out"
    emacs --batch -Q --eval "(progn (require 'calendar) (setq calendar-week-start-day $start) (with-temp-buffer (calendar-mode) (let ((inhibit-read-only t)) (calendar-generate $1 $2)) (princ (buffer-string)) (princ \"\n\")))" 2>/dev/null |
      sed -e 's/[[:space:]]*$//' >> "$out"
  done
done
