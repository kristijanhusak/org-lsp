#!/usr/bin/env python3
"""Open Org files in Emacs with `org-startup-align-all-tables` and record
what they become.

A case is a JSON object:

    {
      "text": "| a |bb|\\n",  an Org document
      "align": true,          `org-startup-align-all-tables`
      "shrink": false         `org-startup-shrink-all-tables` (optional)
    }

`reference(cases)` adds `expected`, the text once `org-mode` has started
in the buffer (`#+STARTUP: align` and `noalign` in the text count too).
Run as a script, it fills in tests/startup-align/cases.json.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(defun org-lsp-startup-case (in out align shrink)
  (let ((org-startup-align-all-tables align)
        (org-startup-shrink-all-tables shrink))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (let ((text (buffer-substring-no-properties (point-min) (point-max))))
        (with-temp-file out (insert text))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def reference(cases):
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.org")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            flag = lambda value: "t" if value else "nil"
            forms.append(
                f"(org-lsp-startup-case {lisp_string(source)} {lisp_string(target)}"
                f" {flag(case.get('align'))} {flag(case.get('shrink'))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       timeout=1200)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = handle.read()
    return cases


if __name__ == "__main__":
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "tests", "startup-align", "cases.json")
    with open(path, encoding="utf-8") as handle:
        cases = json.load(handle)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(reference(cases), handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"recorded {len(cases)} cases")
