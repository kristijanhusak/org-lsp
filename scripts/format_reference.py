"""Format documents as Emacs would and record the result: every table
aligned with `org-table-align`, and the tags of every headline with
`(org-align-tags t)`.

A case is a JSON object:

    {
      "text": "| a |bb|\\n",   an Org document
      "tagsColumn": -77      org-tags-column (optional)
    }

`reference(cases)` adds `expected`: the formatted document, or
`{"error": "..."}` when Org stops with an error. Set ORG_LISP_DIR to the
`lisp` directory of an Org checkout to use that version instead of the
one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(defun org-lsp-format-case (in out tags-column)
  (let ((org-tags-column tags-column))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (goto-char (point-min))
      (while (re-search-forward org-table-line-regexp nil t)
        (if (org-at-table-p)
            (progn (org-table-align) (goto-char (org-table-end)))
          (forward-line)))
      (org-align-tags t)
      (let ((text (buffer-substring-no-properties (point-min) (point-max))))
        (with-temp-file out
          (insert (json-encode (list (cons 'text text)))))))))
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
                f"(condition-case err (org-lsp-format-case {lisp_string(source)} {lisp_string(target)}"
                f" {case.get('tagsColumn', -77)})"
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
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                result = json.load(handle)
            case["expected"] = result["text"] if "text" in result else result
    return cases
