"""Run Emacs's priority and timestamp commands over test cases.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",     the pretended current time
      "text": "* TODO a¦\\n",          an Org document; ¦ marks the cursor
      "command": "shift-up",           see COMMANDS below
      "count": 3,                      optional: the prefix argument
      "repeated": true,                optional: the same key was pressed before
      "answer": "b",                   for `priority`: what is typed at its prompt
      "options": {"priorityLowest": "E"}   optional: server settings
    }

`reference(cases)` adds `expected` to each case: the document after the
command with ¦ where the cursor is, or `ERROR: …` when Emacs refuses.
Emacs runs under faketime, so both programs see the same clock. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that version
instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

import todo_reference
from todo_reference import CURSOR, lisp_string, lisp_value, load, save  # noqa: F401

# The server's command names and what Emacs calls them.
COMMANDS = {
    "priority-up": "org-priority-up",
    "priority-down": "org-priority-down",
    "timestamp-up": "org-timestamp-up",
    "timestamp-down": "org-timestamp-down",
    "date-later": "org-timestamp-up-day",
    "date-earlier": "org-timestamp-down-day",
    "shift-up": "org-shiftup",
    "shift-down": "org-shiftdown",
    "shift-left": "org-shiftleft",
    "shift-right": "org-shiftright",
    "priority": "org-priority",
}

DRIVER = r"""
(defun org-lsp-shift-case (in out offset command count repeated answer)
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil)
  (let ((result
         (condition-case err
             (with-temp-buffer
               (insert-file-contents in)
               (org-mode)
               (goto-char (1+ offset))
               (let ((current-prefix-arg count)
                     (this-command command)
                     (last-command (if repeated command 'ignore)))
                 (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t))
                           ;; The key typed at a one-key prompt, and the
                           ;; text typed at any other.
                           ((symbol-function 'read-char-exclusive)
                            (lambda (&rest _) (string-to-char answer)))
                           ((symbol-function 'read-string) (lambda (&rest _) answer)))
                   (call-interactively command)
                   (org-lsp-finish-notes)))
               (insert "¦")
               (buffer-string))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))))
"""


def priority_value(value):
    """`"A"` or `3` as Emacs writes a priority: `?A` or `3`."""
    return "?" + value if isinstance(value, str) else str(value)


def bindings(options):
    """The Emacs variables that correspond to the server options."""
    out = {
        "org-priority-highest": "?A",
        "org-priority-lowest": "?C",
        "org-priority-default": "?B",
        "org-priority-start-cycle-with-default": "t",
        "org-time-stamp-rounding-minutes": "'(0 5)",
    }
    rest = {}
    for key, value in (options or {}).items():
        if key == "priorityHighest":
            out["org-priority-highest"] = priority_value(value)
        elif key == "priorityLowest":
            out["org-priority-lowest"] = priority_value(value)
        elif key == "priorityDefault":
            out["org-priority-default"] = priority_value(value)
        elif key == "priorityStartCycleWithDefault":
            out["org-priority-start-cycle-with-default"] = lisp_value(value)
        elif key == "timestampRoundingMinutes":
            out["org-time-stamp-rounding-minutes"] = f"'(0 {value})"
        else:
            rest[key] = value
    own = " ".join(f"({name} {value})" for name, value in out.items())
    return own + " " + todo_reference.bindings(rest)


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for clock, group in by_clock.items():
            forms = [todo_reference.DRIVER, DRIVER]
            outputs = []
            for index, case in enumerate(group):
                offset = case["text"].index(CURSOR)
                source = os.path.join(directory, f"in-{index}.org")
                target = os.path.join(directory, f"out-{index}.org")
                with open(source, "w", encoding="utf-8") as handle:
                    handle.write(case["text"].replace(CURSOR, "", 1))
                outputs.append((case, target))
                count = case.get("count")
                forms.append(
                    f"(let ({bindings(case.get('options'))})\n"
                    f"  (org-lsp-shift-case {lisp_string(source)} {lisp_string(target)} {offset}"
                    f" '{COMMANDS[case['command']]} {'nil' if count is None else count}"
                    f" {lisp_value(bool(case.get('repeated')))} {lisp_string(case.get('answer', ''))}))"
                )
            driver = os.path.join(directory, "driver.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write("\n".join(forms))
            subprocess.run(
                ["faketime", f"{clock}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=dict(os.environ, TZ="UTC"),
            )
            for case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = handle.read()
    return cases
