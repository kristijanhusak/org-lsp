#!/usr/bin/env python3
"""Record where Org's shifted cursor keys act with each value of
`org-support-shift-select`, and where they leave the key to
`shift-select-mode`, from every position of the texts in
tests/shift-select/cases.json.

A case is a JSON object:

    {"text": "* TODO a\\n- b\\n"}

and gets `expected`: for each value (`nil`, `t`, `always`), each command
(`org-shiftup`, `org-shiftdown`, `org-shiftleft`, `org-shiftright`,
`org-shiftcontrolright`) and each position of the text, in characters, a
letter: `o` when Org acts (or refuses in its own words), `s` when the key
selects, `e` when it says that it does not apply here
(`org-shiftselect-error`). Set ORG_LISP_DIR to the `lisp` directory of an
Org checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "shift-select", "cases.json")
VALUES = {"nil": "nil", "t": "t", "always": "always"}
COMMANDS = ["org-shiftup", "org-shiftdown", "org-shiftleft", "org-shiftright", "org-shiftcontrolright"]

DRIVER = r"""
(require 'org)
(require 'json)
(advice-add 'message :override #'ignore)
(advice-add 'org-call-for-shift-select :override (lambda (&rest _) (throw 'org-lsp 's)))
(advice-add 'org-shiftselect-error :override (lambda (&rest _) (throw 'org-lsp 'e)))
(defun org-lsp-shift-select (in out values commands)
  (let ((text (with-temp-buffer (insert-file-contents in) (buffer-string)))
        (result nil))
    (dolist (value values)
      (let (by-command)
        (dolist (command commands)
          (let ((letters ""))
            (dotimes (point (1+ (length text)))
              (setq letters
                    (concat letters
                            (symbol-name
                             (catch 'org-lsp
                               (with-temp-buffer
                                 (insert text)
                                 (org-mode)
                                 (goto-char (1+ point))
                                 (let ((org-support-shift-select (cdr value)))
                                   (condition-case nil
                                       (call-interactively command)
                                     (error nil)))
                                 'o))))))
            (push (cons command letters) by-command)))
        (push (cons (car value) (nreverse by-command)) result)))
    (with-temp-file out (insert (json-encode (nreverse result))))))
"""


def reference(cases):
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        values = " ".join(f'("{name}" . {lisp})' for name, lisp in VALUES.items())
        commands = " ".join(COMMANDS)
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            target = os.path.join(directory, f"out-{index}.json")
            forms.append(f"(org-lsp-shift-select {json.dumps(source)} {json.dumps(target)} '({values}) '({commands}))")
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            timeout=1200,
        )
        for index, case in enumerate(cases):
            with open(os.path.join(directory, f"out-{index}.json"), encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases


if __name__ == "__main__":
    path = sys.argv[1] if len(sys.argv) > 1 else PATH
    with open(path, encoding="utf-8") as handle:
        cases = json.load(handle)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(reference(cases), handle, indent=2, ensure_ascii=False)
        handle.write("\n")
    print(f"updated {len(cases)} cases")
