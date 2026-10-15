#!/usr/bin/env python3
"""Record how Emacs writes and reads durations (`org-duration`).

`tests/duration/cases.json` holds formats and units, written as Lisp, with
numbers of minutes to write (`org-duration-from-minutes`) and texts to read
(`org-duration-to-minutes`). Each case gets what Emacs answers: the text,
or the minutes (null for an error). Set ORG_LISP_DIR to the `lisp`
directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "duration", "cases.json")

DRIVER = r"""
(require 'org)
(require 'org-duration)
(require 'json)
(defun org-lsp-durations (units format minutes texts out)
  (let ((org-duration-units units) (org-duration-format format))
    (org-duration-set-regexps)
    (with-temp-file out
      (insert (json-encode
               (list (cons 'written (vconcat (mapcar (lambda (m) (org-duration-from-minutes m)) minutes)))
                     (cons 'read (vconcat (mapcar (lambda (text)
                                                    (condition-case nil (org-duration-to-minutes text)
                                                      (error nil)))
                                                  texts)))))))))
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
            target = os.path.join(directory, f"out-{index}.json")
            minutes = " ".join(repr(m) for m in case["minutes"])
            texts = " ".join(lisp_string(t) for t in case["texts"])
            forms.append(
                f"(org-lsp-durations '{case['units']} '{case['format']} '({minutes}) (list {texts}) {lisp_string(target)})"
            )
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
                found = json.load(handle)
            case["written"] = found["written"]
            case["read"] = found["read"]
    with open(PATH, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"updated {len(cases)} cases")


if __name__ == "__main__":
    main()
