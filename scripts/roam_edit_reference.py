"""Run org-roam's editing commands over test cases and record the result.

A case is a JSON object:

    {
      "files": {"a.org": "…¦…", "b.org": "…"},  the files of
                                          `org-roam-directory`; ¦ marks the
                                          cursor in the file edited
      "command": "alias-add",             `alias-add`, `alias-remove`,
                                          `tag-add`, `tag-remove`,
                                          `ref-add`, `ref-remove` (each
                                          `org-roam-…`), `replace-links`
                                          (`org-roam-link-replace-all`),
                                          `promote` or `demote`
                                          (`org-roam-promote-entire-buffer`,
                                          `org-roam-demote-entire-buffer`)
      "arg": "x"                          its argument: a text, or a list
                                          of tags
    }

`reference(cases)` adds `expected`: `{"text": …, "message": …}`, the
file's text afterwards and the last message, or `ERROR: …` with the text
when Emacs refuses (`{"text": …, "error": …}`). The database is synced
over the files first (see roam_reference.py for ORG_LSP_EMACS_PACKAGES).
"""

import json
import os
import subprocess
import tempfile

from roam_reference import PACKAGES
from todo_reference import CURSOR, lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'org-roam)
(setq org-roam-list-files-commands nil)
(setq org-roam-verbose nil)
(setq create-lockfiles nil)
(defvar org-lsp-said nil)
(advice-add 'message :around
            (lambda (fn format &rest args)
              (when format (setq org-lsp-said (apply #'format-message format args)))
              (apply fn format args)))
(defun org-lsp-roam-edit (dir out file cursor command arg)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let* ((org-roam-directory (file-name-as-directory dir))
         (org-roam-db-location (expand-file-name "../roam.db" dir))
         result)
    (when (file-exists-p org-roam-db-location) (delete-file org-roam-db-location))
    (org-roam-db--close-all)
    (org-roam-db-sync)
    (setq org-lsp-said nil)
    (with-current-buffer (find-file-noselect file)
      (goto-char (1+ cursor))
      (setq result
            (condition-case err
                (progn
                  (pcase command
                    ("alias-add" (org-roam-alias-add arg))
                    ("alias-remove" (org-roam-alias-remove arg))
                    ("tag-add" (org-roam-tag-add arg))
                    ("tag-remove" (org-roam-tag-remove arg))
                    ("ref-add" (org-roam-ref-add arg))
                    ("ref-remove" (org-roam-ref-remove arg))
                    ("replace-links" (org-roam-link-replace-all))
                    ("promote" (org-roam-promote-entire-buffer))
                    ("demote" (org-roam-demote-entire-buffer)))
                  (list (cons 'text (buffer-substring-no-properties (point-min) (point-max)))
                        (cons 'message (or org-lsp-said :null))))
              (error (list (cons 'text (buffer-substring-no-properties (point-min) (point-max)))
                           (cons 'error (error-message-string err))))))
      (set-buffer-modified-p nil))
    (org-roam-db--close-all)
    (with-temp-file out
      (set-buffer-file-coding-system 'utf-8)
      (insert (json-serialize result)))))
"""


def lisp_arg(arg):
    if isinstance(arg, list):
        return "(list " + " ".join(lisp_string(x) for x in arg) + ")"
    if arg is None:
        return "nil"
    return lisp_string(arg)


def reference(cases):
    """Fill in `expected` for every case."""
    packages = os.environ.get("ORG_LSP_EMACS_PACKAGES")
    if not packages:
        raise SystemExit("set ORG_LSP_EMACS_PACKAGES to the directory of org-roam and what it needs")
    load_path = []
    if os.environ.get("ORG_LISP_DIR"):
        load_path += ["-L", os.environ["ORG_LISP_DIR"]]
    for package in PACKAGES:
        load_path += ["-L", os.path.join(packages, package)]
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            folder = os.path.join(directory, f"case-{index}", "roam")
            os.makedirs(folder)
            cursor = None
            for name, text in sorted(case["files"].items()):
                path = os.path.join(folder, name)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                if CURSOR in text:
                    cursor = (path, len(text[: text.index(CURSOR)]))
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(text.replace(CURSOR, "", 1))
            target = os.path.join(directory, f"case-{index}", "out.json")
            outputs.append((case, target))
            forms.append(
                f"(org-lsp-roam-edit {lisp_string(folder)} {lisp_string(target)} {lisp_string(cursor[0])}"
                f" {cursor[1]} {lisp_string(case['command'])} {lisp_arg(case.get('arg'))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *load_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                expected = json.load(handle)
            # A read error names the file being loaded: the driver's.
            if "error" in expected:
                expected["error"] = expected["error"].replace(": " + driver, "")
            case["expected"] = expected
    return cases
