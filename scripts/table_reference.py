"""Run Org's table commands in Emacs, through their keys, and record the
result.

A case is `{"text": ..., "key": ...}`: an Org document with `¦` where the
cursor is, and the keys pressed (`TAB`, `<backtab>`, `M-<left>`,
`M-S-<up>`, `C-c -`, `C-u C-c -`, `C-c RET`, `C-c SPC`, `C-c ^ a`,
`C-c C-c`, …), or `fn:` and a command called interactively
(`fn:org-table-blank-field`, which Org 9.8 binds to no key in Org mode).
`reference(cases)` adds `expected`: `{"text", "point",
"killed"}` (the point in characters, `killed` what went to the kill ring),
or `{"error": "..."}` when Org stops with an error.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'org-table)
(require 'json)
(setq-default indent-tabs-mode nil)
;; Questions in the minibuffer (`read-number' for the column to sort by)
;; cannot be answered here.
(advice-add 'read-from-minibuffer :override
            (lambda (prompt &rest _) (error "Asked: %s" prompt)))
(defun org-lsp-table-case (in out key)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (switch-to-buffer (current-buffer))
    (goto-char (point-min))
    (search-forward "¦")
    (delete-char -1)
    ;; As Emacs shows the buffer: links show their descriptions.
    (font-lock-ensure)
    (setq org-table-may-need-update t
          org-table-last-alignment nil
          org-table-last-column-widths nil
          kill-ring nil
          last-command nil this-command nil real-this-command nil real-last-command nil)
    (let ((result
           (condition-case err
               (progn
                 (if (string-prefix-p "fn:" key)
                     (call-interactively (intern (substring key 3)))
                   (execute-kbd-macro (kbd key)))
                 (list (cons 'text (buffer-substring-no-properties (point-min) (point-max)))
                       (cons 'point (1- (point)))
                       (cons 'killed (and kill-ring (substring-no-properties (car kill-ring))))))
             (error (list (cons 'error (error-message-string err)))))))
      (with-temp-file out
        (insert (json-encode result))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases, timeout=300):
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
            forms.append(f"(org-lsp-table-case {lisp_string(source)} {lisp_string(target)} {lisp_string(case['key'])})")
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        try:
            subprocess.run(
                ["emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=False, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            pass
        for case, target in outputs:
            if os.path.exists(target):
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = json.load(handle)
            else:
                case["expected"] = {"error": "Emacs did not finish"}
    return cases
