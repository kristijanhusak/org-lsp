#!/usr/bin/env python3
"""Record where RET follows a link in Emacs with `org-return-follows-link`.

For each Org text of `tests/open/return-follows.json`, every place of the
cursor is tried: `org-return` with `org-return-follows-link` set, and
`org-open-at-point` replaced by a stand-in that records it was called.
`follows` lists the places (in characters from the start) where it was.
Set ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "open", "return-follows.json")

DRIVER = r"""
(require 'org)
(require 'json)
(defun org-lsp-follows (in out)
  (let ((org-return-follows-link t) (found nil) (text nil))
    (with-temp-buffer
      (insert-file-contents in)
      (setq text (buffer-string))
      (org-mode)
      (dotimes (point (1+ (length text)))
        (erase-buffer)
        (insert text)
        (goto-char (1+ point))
        (when (catch 'followed
                (cl-letf (((symbol-function 'org-open-at-point) (lambda (&rest _) (interactive) (throw 'followed t)))
                          ((symbol-function 'org-table-next-row) (lambda (&rest _) (interactive) nil)))
                  (ignore-errors (org-return)))
                nil)
          (push point found))))
    (with-temp-file out (insert (json-encode (vconcat (nreverse found)))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main():
    with open(PATH, encoding="utf-8") as handle:
        cases = json.load(handle)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            forms.append(f"(org-lsp-follows {lisp_string(source)} {lisp_string(target)})")
            outputs.append(target)
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            env=dict(os.environ, HOME=directory),
            timeout=600,
        )
        for case, target in zip(cases, outputs):
            with open(target, encoding="utf-8") as handle:
                case["follows"] = json.load(handle)
    with open(PATH, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"updated {len(cases)} cases")


if __name__ == "__main__":
    main()
