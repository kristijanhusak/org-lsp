"""Fold every subtree in Emacs and record which lines it hides.

A case is a JSON object:

    {
      "text": "* a\\nbody\\n\\n* b\\n",   an Org document
      "separatorLines": 2              optional: org-cycle-separator-lines
    }

`reference(cases)` adds `expected` to each case: for every headline whose
subtree hides something when folded, `[headline line, last hidden line]`,
lines counted from 0. Each subtree is folded on its own, from a document
that shows everything, the way the first `TAB` on it does: the subtree is
hidden and `org-cycle-show-empty-lines` shows the blank lines Org keeps
between folded subtrees. Set ORG_LISP_DIR to the `lisp` directory of an
Org checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(defun org-lsp-fold-case (in out)
  (let ((folds '()))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (goto-char (point-min))
      (while (re-search-forward org-outline-regexp-bol nil t)
        (let ((start (line-beginning-position))
              (line (1- (line-number-at-pos))))
          (save-excursion
            (org-fold-show-all)
            (goto-char start)
            (org-fold-subtree t)
            (org-cycle-show-empty-lines 'folded)
            ;; The lines after the headline that are hidden: a line shows
            ;; on screen only if the newline before it does.
            (let ((last line))
              (forward-line 1)
              (while (and (not (eobp)) (invisible-p (1- (point))))
                (setq last (1- (line-number-at-pos)))
                (forward-line 1))
              (when (> last line)
                (push (vector line last) folds))))
          (goto-char start)
          (end-of-line))))
    (with-temp-file out
      (insert (json-encode (vconcat (nreverse folds)))))))
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
            forms.append(
                f"(let ((org-cycle-separator-lines {case.get('separatorLines', 2)}))\n"
                f"  (org-lsp-fold-case {lisp_string(source)} {lisp_string(target)}))"
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
