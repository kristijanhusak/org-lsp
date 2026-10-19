"""Press keys on inline tasks in Emacs, with `org-inlinetask` loaded, and
record the result.

A case is a JSON object:

    {
      "text": "* a\\n",     an Org document
      "point": 3,         where the cursor is, in characters from the start
      "mark": 1,          optional: the other end of an active region
      "key": "C-c C-x t", the keys pressed, as `kbd` reads them
      "emacs": "(org-inlinetask-default-state \\"TODO\\")"
                          optional: more bindings for the case, as Lisp
    }

`reference(cases)` adds `expected`: `{"text", "point", "hidden"}` after
the keys, `hidden` the lines the keys left hidden as `[first, last]`
ranges counted from 0, or `{"error": "..."}` when Org stops with an
error. The keys go through Org's keymap as typed (`execute-kbd-macro`),
with Emacs's defaults and `transient-mark-mode` on.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'org-inlinetask)
(require 'json)
(setq-default indent-tabs-mode nil)
(transient-mark-mode 1)
(defun org-lsp-hidden-lines ()
  "The ranges of lines whose start is hidden, as [first last]."
  (let ((ranges '()) (first nil) (line 0))
    (save-excursion
      (goto-char (point-min))
      (while (not (eobp))
        (forward-line 1)
        (setq line (1+ line))
        (if (and (not (eobp)) (invisible-p (1- (point))))
            (setq first (or first line))
          (when first (push (vector first (1- line)) ranges) (setq first nil)))))
    (vconcat (nreverse ranges))))
(defun org-lsp-inlinetask-case (in out point mark key)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (switch-to-buffer (current-buffer))
    (goto-char (1+ point))
    (when mark
      (push-mark (1+ mark) t t))
    (setq last-command nil this-command nil real-this-command nil real-last-command nil)
    (execute-kbd-macro (kbd key))
    (let ((text (buffer-substring-no-properties (point-min) (point-max)))
          (at (1- (point)))
          (hidden (org-lsp-hidden-lines)))
      (with-temp-file out
        (insert (json-encode (list (cons 'text text) (cons 'point at) (cons 'hidden hidden))))))))
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
            mark = case.get("mark")
            mark = "nil" if mark is None else str(mark)
            forms.append(
                f"(let ({case.get('emacs', '')})"
                f" (condition-case err (org-lsp-inlinetask-case {lisp_string(source)} {lisp_string(target)}"
                f" {case['point']} {mark} {lisp_string(case['key'])})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err)))))))))"
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
