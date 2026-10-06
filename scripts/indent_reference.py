"""Indent lines with Emacs's `org-indent-line` and record the result.

A case is a JSON object:

    {
      "text": "* a\\nbody\\n",      an Org document, or
      "file": "tests/fixtures/x.org",   one, relative to the repository
      "adapt": "t"                     org-adapt-indentation: nil, t or
                                       headline-data
    }

`reference(cases)` adds `expected` to each case:

    {
      "lines": [...],     every line once `org-indent-line` has run on it,
                          each on its own from the original document
      "newline": [...]    the indentation `org-indent-line` gives a new
                          line opened after each line (`RET` at its end)
      "region": "..."     the document once `org-indent-region` has run
                          over all of it
      "virtual": [...]    `[line-prefix, wrap-prefix]` of every line with
                          `org-indent-mode`, in columns
    }

or `{"error": "..."}` when Org stops with an error.

Tabs are not used for indentation (`indent-tabs-mode` is nil). Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DRIVER = r"""
(require 'org)
(require 'json)
(defun org-lsp-indent-buffer (text)
  (let ((buffer (generate-new-buffer "indent")))
    (with-current-buffer buffer
      (insert text)
      (org-mode)
      (setq-local indent-tabs-mode nil)
      (goto-char (point-min)))
    buffer))
(defun org-lsp-indent-case (in out adapt)
  (let* ((org-adapt-indentation adapt)
         (text (with-temp-buffer (insert-file-contents in) (buffer-string)))
         (count (with-temp-buffer (insert text) (count-lines (point-min) (point-max))))
         (lines '())
         (newlines '()))
    (dotimes (line count)
      (let ((buffer (org-lsp-indent-buffer text)))
        (with-current-buffer buffer
          (forward-line line)
          (org-indent-line)
          (push (buffer-substring-no-properties (line-beginning-position) (line-end-position)) lines))
        (kill-buffer buffer))
      (let ((buffer (org-lsp-indent-buffer text)))
        (with-current-buffer buffer
          (forward-line line)
          (end-of-line)
          (insert "\n")
          (org-indent-line)
          (push (current-indentation) newlines))
        (kill-buffer buffer)))
    (let* ((virtual
            (let ((buffer (org-lsp-indent-buffer text)) (out '()))
              (with-current-buffer buffer
                (org-indent-mode 1)
                (org-indent-add-properties (point-min) (point-max))
                (dotimes (line count)
                  (goto-char (point-min))
                  (forward-line line)
                  (push (vector (string-width (or (get-text-property (point) 'line-prefix) ""))
                                (string-width (or (get-text-property (point) 'wrap-prefix) "")))
                        out)))
              (kill-buffer buffer)
              (vconcat (nreverse out))))
           (buffer (org-lsp-indent-buffer text))
           (region (with-current-buffer buffer
                     (org-indent-region (point-min) (point-max))
                     (buffer-substring-no-properties (point-min) (point-max)))))
      (kill-buffer buffer)
      (with-temp-file out
        (insert (json-encode (list (cons 'lines (vconcat (nreverse lines)))
                                   (cons 'newline (vconcat (nreverse newlines)))
                                   (cons 'region region)
                                   (cons 'virtual virtual))))))))
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


def case_text(case):
    if "file" in case:
        with open(os.path.join(ROOT, case["file"]), encoding="utf-8") as handle:
            return handle.read()
    return case["text"]


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
                handle.write(case_text(case))
            outputs.append((case, target))
            adapt = case.get("adapt", "nil")
            adapt = adapt if adapt in ("nil", "t") else "'" + adapt
            # A document Org itself fails on is recorded as such.
            forms.append(
                f"(condition-case err (org-lsp-indent-case {lisp_string(source)} {lisp_string(target)} {adapt})"
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
