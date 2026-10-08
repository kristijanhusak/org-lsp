"""Press RET or M-RET in Emacs and record the result.

A case is a JSON object:

    {
      "text": "* a\\n",     an Org document
      "point": 3,         where the cursor is, in characters from the start
      "key": "RET",       the key pressed: "RET" or "M-RET"; "below:M-RET"
                          or "below:M-S-<return>" is the key of modal
                          editors, below the line: the command from the
                          end of the line with `org-M-RET-may-split-line`
                          nil, in a table `org-table-insert-row` with its
                          argument
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
(defun org-lsp-below (key)
  (let ((org-M-RET-may-split-line nil))
    (end-of-line)
    (cond ((and (equal key "M-RET") (org-at-table-p)) (org-table-insert-row '(4)))
          ((equal key "M-RET") (org-meta-return))
          (t (org-insert-todo-heading nil)))))
(defun org-lsp-return-case (in out point key adapt)
  (let ((org-adapt-indentation adapt))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (switch-to-buffer (current-buffer))
      (goto-char (1+ point))
      (if (string-prefix-p "below:" key)
          (org-lsp-below (substring key 6))
        (setq last-command nil this-command nil real-this-command nil real-last-command nil)
        (execute-kbd-macro (kbd key)))
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


HANG = "Emacs did not finish"


def reference(cases, timeout=None, preamble=""):
    """Fill in `expected` for every case. With `timeout` (seconds for the
    whole run), a case Emacs never finishes gets `{"error": HANG}` and the
    cases after it run again. `preamble` is Lisp to run first."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER, preamble]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            adapt = case.get("adapt", "nil")
            adapt = adapt if adapt in ("nil", "t") else "'" + adapt
            circular = "t" if case.get("circular") else "nil"
            forms.append(
                f"(let ((org-list-use-circular-motion {circular}))"
                f" (condition-case err (org-lsp-return-case {lisp_string(source)} {lisp_string(target)}"
                f" {case['point']} {lisp_string(case.get('key', 'RET'))} {adapt})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err)))))))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        try:
            subprocess.run(
                ["emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            done = 0
            while done < len(outputs) and os.path.exists(outputs[done][1]):
                done += 1
            for case, target in outputs[:done]:
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = json.load(handle)
            outputs[done][0]["expected"] = {"error": HANG}
            reference(cases[done + 1:], timeout, preamble)
            return cases
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
