"""Sort entries in Emacs (`org-sort-entries`) and record the result.

A case is a JSON object:

    {
      "text": "* a\\n",   an Org document
      "point": 3,        where the cursor is, in characters from the start
      "type": "a",       the sorting type: a, n, p, r, o, t, s, d, c, k, or
                         the capital for the reverse order
      "property": "P"    for r and R, the property
    }

`reference(cases)` adds `expected`: `{"text", "point"}` after sorting, or
`{"error": "..."}` when Org stops with an error. Emacs runs in the C
locale, where `string-collate-lessp` compares characters by their code,
with the time fixed at NOW (what an entry without the date sorts by).
"""

import json
import os
import subprocess
import tempfile

# Monday 18 May 2026, 10:30, in the local time of the run.
NOW = "2026-05-18 10:30"

DRIVER = r"""
(require 'org)
(require 'org-clock)
(require 'json)
(setq-default indent-tabs-mode nil)
(defvar org-lsp-now (org-time-string-to-time "<%s>"))
(advice-add 'current-time :override (lambda () org-lsp-now))
(defun org-lsp-sort-case (in out point type property)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (switch-to-buffer (current-buffer))
    (goto-char (1+ point))
    (org-sort-entries nil (string-to-char type) nil nil (and (> (length property) 0) property))
    (let ((text (buffer-substring-no-properties (point-min) (point-max)))
          (at (1- (point))))
      (with-temp-file out
        (insert (json-encode (list (cons 'text text) (cons 'point at))))))))
""" % NOW


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases, timeout=1200):
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
                f"(condition-case err (org-lsp-sort-case {lisp_string(source)} {lisp_string(target)}"
                f" {case['point']} {lisp_string(case['type'])} {lisp_string(case.get('property', ''))})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err))))))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC")
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True, env=env,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
