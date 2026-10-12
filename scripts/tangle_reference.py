"""Run Emacs's `org-babel-tangle` over test cases and record what it writes.

A case is a JSON object:

    {
      "files": {"notes.org": "..."},   the files of a directory; the first
                                        `.org` file in name order is tangled
      "emacs": "(setq ...)"            optional: settings for Emacs, which
                                        the case's `settings` give org-lsp
    }

`reference(cases)` adds `expected` to each case:

    {
      "files": {"notes.py": "...", "sub/x.sh": "..."},   what was written,
                                                         relative to the
                                                         directory
      "modes": {"sub/x.sh": "755"},                     modes other than 644
      "message": "Tangled 2 code blocks from notes.org"
    }

or `{"error": "..."}` when Emacs refuses. Files that were there before and
did not change are left out. Set ORG_LISP_DIR to the `lisp` directory of
an Org checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

from todo_reference import lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'ob-tangle)
(setq org-babel-tangle-default-file-mode #o644)
(defun org-lsp-tangle-case (file settings out)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  ;; What a case sets is for that case only.
  (let* ((message-log-max t)
         (org-src-preserve-indentation nil)
         (org-babel-tangle-lang-exts (default-value 'org-babel-tangle-lang-exts))
         (org-babel-default-header-args (default-value 'org-babel-default-header-args))
         (result
          (condition-case err
              (with-current-buffer (find-file-noselect file)
                (eval (car (read-from-string (concat "(progn " settings ")"))) t)
                (let ((inhibit-message t))
                  (org-babel-tangle))
                (with-current-buffer "*Messages*"
                  (goto-char (point-max))
                  (forward-line -1)
                  (concat "message\0" (buffer-substring (line-beginning-position) (line-end-position)))))
            (error (format "error\0%s" (error-message-string err))))))
    (with-temp-file out (insert result))))
"""


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        runs = []
        for index, case in enumerate(cases):
            folder = os.path.join(directory, f"case-{index}")
            os.makedirs(folder)
            for name, text in case["files"].items():
                path = os.path.join(folder, name)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(text)
            before = snapshot(folder)
            org = sorted(name for name in case["files"] if name.endswith(".org"))[0]
            out = os.path.join(directory, f"out-{index}")
            runs.append((case, folder, before, out))
            forms.append(
                f"(org-lsp-tangle-case {lisp_string(os.path.join(folder, org))}"
                f" {lisp_string(case.get('emacs', ''))} {lisp_string(out)})"
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
            env=dict(os.environ, HOME=directory, TZ="UTC"),
            timeout=1200,
        )
        for case, folder, before, out in runs:
            with open(out, encoding="utf-8") as handle:
                kind, value = handle.read().split("\0", 1)
            if kind == "error":
                case["expected"] = {"error": value}
                continue
            after = snapshot(folder)
            written = {name: entry for name, entry in after.items() if before.get(name) != entry}
            case["expected"] = {
                "files": {name: text for name, (text, _) in sorted(written.items())},
                "modes": {name: mode for name, (_, mode) in sorted(written.items()) if mode != "644"},
                "message": value.replace(folder + "/", ""),
            }
    return cases


def snapshot(folder):
    """Every file under `folder`: its text and mode, by relative path."""
    found = {}
    for root, _, names in os.walk(folder):
        for name in names:
            path = os.path.join(root, name)
            with open(path, encoding="utf-8", errors="replace") as handle:
                text = handle.read()
            found[os.path.relpath(path, folder)] = (text, format(os.stat(path).st_mode & 0o777, "o"))
    return found
