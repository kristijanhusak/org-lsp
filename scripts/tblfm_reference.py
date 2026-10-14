"""Run Emacs's table formulas over test cases and record the tables after.

A case is a JSON object:

    {
      "text": "| a | b |\\n...\\n#+TBLFM: $2=$1*2\\n",   a file; ¦ marks the
                                                       cursor, in the table
                                                       or on its #+TBLFM line
      "command": "all"                                 optional: "all"
                                                       (`C-u C-c *`, the
                                                       default), "line"
                                                       (`C-c *`) or "iterate"
                                                       (`C-u C-u C-c *`)
      "emacs": "(org-table-formula-create-columns t)"  optional: options,
                                                       as `let` bindings
    }

`reference(cases)` adds `expected`: `{"text": ...}`, the file after, or
`{"error": ...}` when Emacs refuses. Set ORG_LISP_DIR to the `lisp`
directory of an Org checkout to use that version instead of the one bundled
with Emacs.
"""

import json
import os
import subprocess
import tempfile

from todo_reference import CURSOR, lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'org-table)
(require 'calc)
(defun org-lsp-tblfm-case (text offset command out bindings)
  (let ((result
         (condition-case err
             (with-temp-buffer
               (insert text)
               (let ((org-mode-hook nil)) (org-mode))
               (goto-char (1+ offset))
               (dolist (binding bindings)
                 (set (make-local-variable (car binding)) (eval (cadr binding) t)))
               (let ((inhibit-message t))
                 (pcase command
                   ("line" (org-table-recalculate nil))
                   ("iterate" (org-table-recalculate 'iterate))
                   (_ (if (save-excursion (beginning-of-line) (looking-at-p "[ \t]*#\\+TBLFM:"))
                          (org-table-calc-current-TBLFM)
                        (org-table-recalculate t)))))
               (concat "text\0" (buffer-string)))
           (error (format "error\0%s" (error-message-string err))))))
    (with-temp-file out (insert result))))
"""


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            text = case["text"]
            offset = text.index(CURSOR) if CURSOR in text else text.index("|")
            out = os.path.join(directory, f"out-{index}")
            outputs.append((case, out))
            forms.append(
                f"(org-lsp-tblfm-case {lisp_string(text.replace(CURSOR, '', 1))} {offset}"
                f" {lisp_string(case.get('command', 'all'))} {lisp_string(out)} '({case.get('emacs', '')}))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, HOME=directory, TZ="UTC"),
            timeout=1200,
        )
        for case, out in outputs:
            with open(out, encoding="utf-8") as handle:
                kind, value = handle.read().split("\0", 1)
            case["expected"] = {kind: value}
    return cases
