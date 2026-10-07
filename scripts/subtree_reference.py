"""Cut, copy and paste subtrees in Emacs and record the result.

A case is a JSON object:

    {
      "text": "* a\\n",   an Org document
      "point": 3,        where the cursor is, in characters from the start
      "op": "cut",       "cut" (`org-cut-subtree`), "copy"
                         (`org-copy-subtree`) or "paste"
                         (`org-paste-subtree`)
      "count": 2,        the prefix argument of a cut or copy (optional)
      "kill": "* b\\n",  for a paste, what is pasted
      "level": 3,        for a paste, a numeric prefix argument (optional)
      "adapt": "nil"     org-adapt-indentation: nil, t or headline-data
    }

`reference(cases)` adds `expected`: `{"text", "point", "kill"}` after the
command (`kill` for a cut or copy), or `{"error": "..."}` when Org stops
with an error. The commands run as called interactively.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(setq-default indent-tabs-mode nil)
(defun org-lsp-subtree-case (in out point op count kill level adapt)
  (let ((org-adapt-indentation adapt))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (switch-to-buffer (current-buffer))
      (goto-char (1+ point))
      (setq kill-ring nil)
      (cond
       ((equal op "cut") (let ((current-prefix-arg count)) (call-interactively #'org-cut-subtree)))
       ((equal op "copy") (let ((current-prefix-arg count)) (call-interactively #'org-copy-subtree)))
       (t (kill-new kill)
          (let ((current-prefix-arg level)) (call-interactively #'org-paste-subtree))))
      (let ((text (buffer-substring-no-properties (point-min) (point-max)))
            (at (1- (point))))
        (with-temp-file out
          (insert (json-encode (list (cons 'text text) (cons 'point at)
                                     (cons 'kill (if (member op '("cut" "copy")) (car kill-ring) :json-null))))))))))
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


def reference(cases, timeout=None):
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
            adapt = case.get("adapt", "nil")
            adapt = adapt if adapt in ("nil", "t") else "'" + adapt
            count = str(case["count"]) if case.get("count") else "nil"
            level = str(case["level"]) if case.get("level") else "nil"
            forms.append(
                f"(condition-case err (org-lsp-subtree-case {lisp_string(source)} {lisp_string(target)}"
                f" {case['point']} {lisp_string(case['op'])} {count} {lisp_string(case.get('kill', ''))} {level} {adapt})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err))))))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
