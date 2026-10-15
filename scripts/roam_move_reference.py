"""Run org-roam's commands that move a subtree between files over test
cases and record every file afterwards.

A case is a JSON object:

    {
      "files": {"a.org": "…¦…", "b.org": "…"},  the files of
                                          `org-roam-directory`; ¦ marks the
                                          cursor
      "command": "refile",                `refile` (`org-roam-refile`, to
                                          the node whose ID is `arg`) or
                                          `extract`
                                          (`org-roam-extract-subtree`,
                                          answering `arg` for the file)
      "arg": "b1"
    }

`reference(cases)` adds `expected`: `{"files": {name: text}, "error": …}`,
the files once Emacs has saved its buffers (a file deleted is left out),
and its error if it fails, with `org-roam-directory` cut out of it. `org-roam-extract-new-file-path` is the
default, so the answer's default name is not recorded; the time is
Monday 18 May 2026, 10:30. See roam_reference.py for
ORG_LSP_EMACS_PACKAGES.
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
(defun org-lsp-roam-move (dir out file cursor command arg)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let* ((org-roam-directory (file-name-as-directory dir))
         (org-roam-db-location (expand-file-name "../roam.db" dir))
         error)
    (when (file-exists-p org-roam-db-location) (delete-file org-roam-db-location))
    (org-roam-db--close-all)
    (org-roam-db-sync)
    (with-current-buffer (find-file-noselect file)
      (goto-char (1+ cursor))
      (condition-case err
          (cl-letf (((symbol-function 'read-file-name)
                     (lambda (&rest _) (expand-file-name arg org-roam-directory))))
            (pcase command
              ("refile" (org-roam-refile (org-roam-node-from-id arg)))
              ("extract" (org-roam-extract-subtree))))
        (error (setq error (replace-regexp-in-string
                            (regexp-quote org-roam-directory) "" (error-message-string err) t t)))))
    (save-some-buffers t)
    (let (files)
      (dolist (path (directory-files-recursively dir "\\.org\\'"))
        (push (cons (intern (file-relative-name path dir))
                    (with-temp-buffer (insert-file-contents path) (buffer-string)))
              files))
      (org-roam-db--close-all)
      (with-temp-file out
        (set-buffer-file-coding-system 'utf-8)
        (insert (json-serialize (list (cons 'files files) (cons 'error (or error :null)))))))))
"""


def run(case, load_path):
    """Run one case in an Emacs of its own: what a command leaves behind
    (the kill ring, buffers) changes the next one."""
    with tempfile.TemporaryDirectory() as directory:
        folder = os.path.join(directory, "roam")
        os.makedirs(folder)
        cursor = None
        for name, text in sorted(case["files"].items()):
            path = os.path.join(folder, name)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            if CURSOR in text:
                cursor = (path, len(text[: text.index(CURSOR)]))
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text.replace(CURSOR, "", 1))
        target = os.path.join(directory, "out.json")
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write(DRIVER)
            handle.write(
                f"(org-lsp-roam-move {lisp_string(folder)} {lisp_string(target)} {lisp_string(cursor[0])}"
                f" {cursor[1]} {lisp_string(case['command'])} {lisp_string(case.get('arg', ''))})"
            )
        subprocess.run(
            ["faketime", "2026-05-18 10:30:00", "emacs", "--batch", "-Q", *load_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
            timeout=120,
        )
        with open(target, encoding="utf-8") as handle:
            return json.load(handle)


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
    for case in cases:
        case["expected"] = run(case, load_path)
    return cases


if __name__ == "__main__":
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "tests", "roam", "move.json")
    cases = load(path)
    save(path, reference(cases))
    print(f"recorded {len(cases)} cases")
