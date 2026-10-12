"""Render org-roam's buffer over test cases and record its text.

A case is a JSON object:

    {
      "files": {"a.org": "…¦…", "b.org": "…"},  the files of
                                          `org-roam-directory`; ¦ marks the
                                          cursor, whose node the buffer is
                                          for (`org-roam-node-at-point`)
      "sections": ["backlinks", "reflinks", "unlinked-references"]
                                          optional: `org-roam-mode-sections`
                                          (backlinks and reflinks otherwise)
    }

`reference(cases)` adds `expected`: the buffer's text
(`org-roam-buffer-render-contents`), its header line (the node's title)
apart as `title`; or null when there is no node at the cursor. Ripgrep
lists the unlinked references sorted by file (`--sort path`), where
otherwise their order would vary from run to run. See roam_reference.py
for ORG_LSP_EMACS_PACKAGES.
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
;; The unlinked references in one order.
(advice-add 'org-roam-unlinked-references--rg-command :filter-return
            (lambda (command) (replace-regexp-in-string "\\`rg " "rg --sort path " command)))
(defun org-lsp-roam-buffer (dir out file cursor sections)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let* ((org-roam-directory (file-name-as-directory dir))
         (org-roam-db-location (expand-file-name "../roam.db" dir))
         (org-roam-mode-sections sections)
         result)
    (when (file-exists-p org-roam-db-location) (delete-file org-roam-db-location))
    (org-roam-db--close-all)
    (org-roam-db-sync)
    (with-current-buffer (find-file-noselect file)
      (goto-char (1+ cursor))
      (let ((node (org-roam-node-at-point)))
        (setq result
              (if (not node)
                  :null
                ;; A buffer of its own: the last one's `org-roam-directory'
                ;; would stay.
                (when (get-buffer "*org-lsp-roam*") (kill-buffer "*org-lsp-roam*"))
                (with-current-buffer (get-buffer-create "*org-lsp-roam*")
                  (setq-local org-roam-buffer-current-node node)
                  (setq-local org-roam-buffer-current-directory org-roam-directory)
                  (org-roam-buffer-render-contents)
                  (list (cons 'title (or (org-roam-node-title node) :null))
                        (cons 'text (buffer-substring-no-properties (point-min) (point-max)))))))))
    (org-roam-db--close-all)
    (with-temp-file out
      (set-buffer-file-coding-system 'utf-8)
      (insert (json-serialize result)))))
"""

SECTIONS = {
    "backlinks": "org-roam-backlinks-section",
    "reflinks": "org-roam-reflinks-section",
    "unlinked-references": "org-roam-unlinked-references-section",
}


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
            sections = case.get("sections", ["backlinks", "reflinks"])
            forms.append(
                f"(org-lsp-roam-buffer {lisp_string(folder)} {lisp_string(target)} {lisp_string(cursor[0])}"
                f" {cursor[1]} (list {' '.join('#' + chr(39) + SECTIONS[s] for s in sections)}))"
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
            # The files by their names in the case, not the temporary ones.
            if isinstance(expected, dict):
                expected["text"] = expected["text"].replace(os.path.dirname(target) + "/roam/", "")
            case["expected"] = expected
    return cases
