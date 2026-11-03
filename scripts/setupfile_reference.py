#!/usr/bin/env python3
"""Record what Org reads from setup files (`#+SETUPFILE:`): for each case
of tests/setupfile/cases.json, the values `org-collect-keywords` finds for
the keywords in-buffer settings use, and the TODO keywords the document
then has (`org-todo-keywords-1`).

A case is a JSON object:

    {"files": {"notes.org": "#+SETUPFILE: setup.org\\n* a\\n",
               "setup.org": "#+TODO: FOO | BAR\\n"},
     "document": "notes.org"}

and gets `expected`: `{"keywords": {KEY: [value, …]}, "todo": [keyword, …]}`,
the values in order, a `BIBLIOGRAPHY` value as a path from the case's
directory (`org-collect-keywords` with it in DIRECTORY). Set ORG_LISP_DIR
to the `lisp` directory of an Org checkout to use that version instead of
the one bundled with Emacs.
"""

import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "setupfile", "cases.json")
KEYS = ["TODO", "SEQ_TODO", "TYP_TODO", "TAGS", "FILETAGS", "STARTUP", "LINK", "PROPERTY", "CONSTANTS", "BIBLIOGRAPHY"]

DRIVER = r"""
(require 'org)
(require 'json)
(advice-add 'message :override #'ignore)
(defun org-lsp-setupfile (root document out keys)
  (with-current-buffer (find-file-noselect (expand-file-name document root))
    (let* ((found (org-collect-keywords keys nil '("BIBLIOGRAPHY")))
           (keywords
            (mapcar
             (lambda (entry)
               (cons (car entry)
                     (vconcat
                      (mapcar (lambda (value)
                                (if (consp value)
                                    (file-relative-name (expand-file-name (car value) (cdr value)) root)
                                  value))
                              (cdr entry)))))
             found)))
      ;; Read here: the file written to is another buffer.
      (let ((todo (vconcat org-todo-keywords-1)))
        (with-temp-file out
          (insert (json-encode
                   (list (cons "keywords" (or keywords (make-hash-table)))
                         (cons "todo" todo)))))))))
"""


def reference(cases):
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        keys = " ".join(json.dumps(key) for key in KEYS)
        for index, case in enumerate(cases):
            root = os.path.realpath(os.path.join(directory, f"case-{index}"))
            for name, text in case["files"].items():
                path = os.path.join(root, name)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(text)
            target = os.path.join(directory, f"out-{index}.json")
            forms.append(
                f"(org-lsp-setupfile {json.dumps(root + '/')} {json.dumps(case['document'])} {json.dumps(target)} '({keys}))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            timeout=600,
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
