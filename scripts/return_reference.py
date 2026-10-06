"""Press RET or M-RET in Emacs and record the result.

A case is a JSON object:

    {
      "text": "* a\\n",     an Org document
      "point": 3,         where the cursor is, in characters from the start
      "key": "RET",       the key pressed: "RET" or "M-RET"
      "adapt": "nil"      org-adapt-indentation: nil, t or headline-data
    }

`reference(cases)` adds `expected`: `{"text", "point"}` after the key, or
`{"error": "..."}` when Org stops with an error. The keys go through
Org's keymap as typed (`execute-kbd-macro`), with Emacs's defaults:
`electric-indent-mode` on, `indent-tabs-mode` off.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(setq-default indent-tabs-mode nil)
(electric-indent-mode 1)
(defun org-lsp-return-case (in out point key adapt)
  (let ((org-adapt-indentation adapt))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (switch-to-buffer (current-buffer))
      (goto-char (1+ point))
      (execute-kbd-macro (kbd key))
      (let ((text (buffer-substring-no-properties (point-min) (point-max)))
            (at (1- (point))))
        (with-temp-file out
          (insert (json-encode (list (cons 'text text) (cons 'point at)))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            adapt = case.get("adapt", "nil")
            adapt = adapt if adapt in ("nil", "t") else "'" + adapt
            forms.append(
                f"(condition-case err (org-lsp-return-case {lisp_string(source)} {lisp_string(target)}"
                f" {case['point']} {lisp_string(case.get('key', 'RET'))} {adapt})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err))))))))"
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
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
