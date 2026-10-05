"""Run Emacs's commands that ask for a date over test cases.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",     the pretended current time
      "text": "* TODO a¦\\n",          an Org document; ¦ marks the cursor
      "command": "schedule",           see COMMANDS below
      "answer": "fri 10:00",           what the user types at the date prompt
      "repeated": true,                optional: the same command ran just before
      "options": {"logDone": "time"}   optional: server settings
    }

`reference(cases)` adds two fields to each case. `expected` is the
document after the command with ¦ where the cursor is, or `ERROR: …` when
Emacs refuses. `prompt` is what the prompt offered before the user typed:
the text it was filled with (`default-input`), or "" for none. The answer
replaces that text, as when the user edits the field. Emacs runs under
faketime, so both programs see the same clock. Set ORG_LISP_DIR to the
`lisp` directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import os
import subprocess
import tempfile

import shift_reference
import todo_reference
from todo_reference import CURSOR, lisp_string, lisp_value, load, save  # noqa: F401

COMMANDS = {
    "schedule": "org-schedule",
    "deadline": "org-deadline",
    "timestamp": "org-timestamp",
    "timestamp-inactive": "org-timestamp-inactive",
}

DRIVER = r"""
(defvar org-lsp-prompt nil)
(defun org-lsp-planning-case (in out offset command answer repeated)
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil)
  (setq org-lsp-prompt "")
  (let ((result
         (condition-case err
             (with-temp-buffer
               (insert-file-contents in)
               (org-mode)
               (goto-char (1+ offset))
               (let ((this-command command)
                     (last-command (if repeated command 'ignore))
                     (org-read-date-popup-calendar nil))
                 (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t))
                           ((symbol-function 'read-string)
                            (lambda (_prompt &optional initial &rest _)
                              (setq org-lsp-prompt (or initial ""))
                              answer)))
                   (call-interactively command)
                   (org-lsp-finish-notes)))
               (insert "¦")
               (buffer-string))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))
    (with-temp-file (concat out ".prompt") (insert org-lsp-prompt))))
"""


def reference(cases):
    """Fill in `expected` and `prompt` for every case."""
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
                forms.append(
                    f"(let ({shift_reference.bindings(case.get('options'))})\n"
                    f"  (org-lsp-planning-case {lisp_string(source)} {lisp_string(target)} {offset}"
                    f" '{COMMANDS[case['command']]} {lisp_string(case['answer'])}"
                    f" {lisp_value(bool(case.get('repeated')))}))"
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
                with open(target + ".prompt", encoding="utf-8") as handle:
                    case["prompt"] = handle.read()
    return cases
