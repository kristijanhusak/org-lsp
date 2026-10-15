#!/usr/bin/env python3
"""Turn on `org-num-mode` in Emacs and record the numbers it shows.

A case is a JSON object:

    {
      "text": "* A\\n** B\\n",   an Org document
      "options": {               Org's options, if any
        "org-num-max-level": 2,
        "org-num-skip-commented": true,
        "org-num-skip-footnotes": true,
        "org-num-skip-tags": ["noexport"],
        "org-num-skip-unnumbered": true
      }
    }

`reference(cases)` adds `expected`: for each numbering overlay, its line
(from 0), where it ends on the line (the column after the stars and the
space) and what it shows after it, or null for a headline left out.
Run as a script (`--new` to start from the cases written here), it fills
in tests/num/cases.json.
"""

import json
import os
import subprocess
import sys
import tempfile

DRIVER = r"""
(require 'org)
(require 'org-num)
(require 'json)
(defun org-lsp-num-case (in out options)
  (cl-progv (mapcar #'car options) (mapcar #'cdr options)
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (org-num-mode 1)
      (let ((found nil))
        (dolist (o (sort (copy-sequence org-num--overlays)
                         (lambda (a b) (< (overlay-start a) (overlay-start b)))))
          (let ((shown (overlay-get o 'after-string)))
            (push (vector (1- (line-number-at-pos (overlay-start o)))
                          (save-excursion (goto-char (overlay-end o)) (current-column))
                          (and shown (substring-no-properties shown)))
                  found)))
        (with-temp-file out
          (insert (json-encode (vconcat (nreverse found)))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_value(value):
    if value is True:
        return "t"
    if value is False or value is None:
        return "nil"
    if isinstance(value, list):
        return "(" + " ".join(lisp_string(item) for item in value) + ")"
    return str(value)


def lisp_options(options):
    pairs = [f"({name} . {lisp_value(value)})" for name, value in (options or {}).items()]
    return "'(" + " ".join(pairs) + ")"


TEXTS = [
    "* A\n** B\n** C\n*** D\n* E\n",
    "** A\n* B\n*** C\n** D\n* E\n",
    "* A\n** COMMENT B\n*** C\n** D\n* COMMENT\n* Footnotes\n** F\n* G\n",
    "* A\n** B :noexport:\n*** C\n** D :other:noexport:\n* E\n",
    "* A\n** B\n:PROPERTIES:\n:UNNUMBERED: t\n:END:\n*** C\n** D\n:PROPERTIES:\n:UNNUMBERED: notoc\n:END:\n* E\n",
    "#+STARTUP: odd\n* A\n*** B\n***** C\n*** D\n* E\n",
    "* TODO [#A] A :tag:\n** DONE B\n",
]
OPTIONS = [
    {},
    {"org-num-max-level": 2},
    {"org-num-skip-commented": True},
    {"org-num-skip-footnotes": True},
    {"org-num-skip-tags": ["noexport"]},
    {"org-num-skip-unnumbered": True},
    {"org-num-max-level": 1, "org-num-skip-commented": True, "org-num-skip-tags": ["noexport"]},
]


def default_cases():
    cases = []
    for text in TEXTS:
        for options in OPTIONS:
            case = {"text": text}
            if options:
                case["options"] = options
            cases.append(case)
    return cases


def reference(cases):
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
                f"(org-lsp-num-case {lisp_string(source)} {lisp_string(target)} {lisp_options(case.get('options'))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases


if __name__ == "__main__":
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "tests", "num", "cases.json")
    if "--new" in sys.argv or not os.path.exists(path):
        cases = default_cases()
    else:
        with open(path, encoding="utf-8") as handle:
            cases = json.load(handle)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(reference(cases), handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"recorded {len(cases)} cases")
