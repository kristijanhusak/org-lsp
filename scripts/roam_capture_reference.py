"""Run org-roam's capture of a new node over test cases and record it.

A case is a JSON object:

    {
      "files": {"a.org": "…¦…"},         the files of `org-roam-directory`;
                                         ¦ marks the cursor of the buffer
                                         the capture starts from, if any
      "title": "My note",                the new node's title
      "templates": [ … ],                optional: `org-roam-capture-
                                         templates` as the server takes
                                         them (the default otherwise)
      "keys": "d",                       optional: the template's keys
      "answers": ["…"],                  what the prompts are answered with
      "body": "typed",                   what is typed at the cursor of the
                                         capture buffer
      "finalize": "insert-link"          optional: `org-roam-node-insert`'s
                                         way, a link inserted where the
                                         capture started; else `find-file`
      "node": ["a.org", "id"]            optional: capture to this existing
                                         node of that file, not a new one
    }

`reference(cases)` adds `expected`: `{"files": {...}, "prompts": [...]}`,
every file that is new or changed afterwards, by its name relative to the
directory, and the prompts asked; or `ERROR: …` when Emacs refuses. The
node's ID is 00000000-0000-4000-8000-000000000001 and Emacs runs under
faketime at 2026-05-18 10:30 (see roam_reference.py for
ORG_LSP_EMACS_PACKAGES and ORG_LISP_DIR).
"""

import json
import os
import subprocess
import tempfile

from roam_reference import PACKAGES
from todo_reference import CURSOR, lisp_string, load, save  # noqa: F401

CLOCK = "2026-05-18 10:30"
ID = "00000000-0000-4000-8000-000000000001"

DRIVER = r"""
(require 'org)
(require 'org-roam)
(require 'cl-lib)
(setq org-roam-list-files-commands nil)
(setq org-roam-verbose nil)
(setq create-lockfiles nil)
;; The time of every capture, to the second.
(setq org-overriding-default-time (encode-time 0 30 10 18 5 2026))
(load "userlock" nil t)
(defun ask-user-about-supersession-threat (&rest _) nil)
(advice-add 'yes-or-no-p :override (lambda (&rest _) t))
(defvar org-lsp-answers nil)
(defvar org-lsp-prompts nil)
(defun org-lsp-answer (prompt &rest _)
  (push (substring-no-properties prompt) org-lsp-prompts)
  (if org-lsp-answers (pop org-lsp-answers) (error "No answer for %s" prompt)))
(defun org-lsp-roam-capture (dir out title templates keys answers body finalize cursor-file cursor node)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let* ((org-roam-directory (file-name-as-directory dir))
         (org-roam-db-location (expand-file-name "../roam.db" dir))
         (org-lsp-answers answers)
         (org-lsp-prompts nil)
         (before (make-hash-table :test #'equal))
         result)
    (when (file-exists-p org-roam-db-location) (delete-file org-roam-db-location))
    (org-roam-db--close-all)
    (org-roam-db-sync)
    (dolist (file (directory-files-recursively dir "\\`[^.]"))
      (puthash file (with-temp-buffer (insert-file-contents file) (buffer-string)) before))
    (setq result
          (condition-case err
              (cl-letf (((symbol-function 'org-id-new) (lambda (&rest _) "00000000-0000-4000-8000-000000000001"))
                        ((symbol-function 'read-from-minibuffer) #'org-lsp-answer)
                        ((symbol-function 'read-string) #'org-lsp-answer)
                        ((symbol-function 'completing-read) #'org-lsp-answer))
                (with-current-buffer (if cursor-file (find-file-noselect cursor-file) (get-buffer-create "*scratch*"))
                  (when cursor (goto-char (1+ cursor)))
                  (org-roam-capture-
                   :node (if node (org-roam-node-from-id node) (org-roam-node-create :title title))
                   :keys keys
                   :templates templates
                   :props (if (equal finalize "insert-link")
                              (list :link-description title :finalize 'insert-link)
                            (list :finalize 'find-file))))
                (with-current-buffer
                    (or (cl-find-if (lambda (b) (string-prefix-p "CAPTURE-" (buffer-name b))) (buffer-list))
                        (error "No capture buffer"))
                  (insert body)
                  (org-capture-finalize))
                (let (files)
                  (dolist (buffer (buffer-list))
                    (let ((file (buffer-file-name buffer)))
                      (when (and file (string-prefix-p (file-name-as-directory dir) file))
                        (with-current-buffer buffer
                          (unless (equal (gethash file before) (buffer-string))
                            (push (cons (file-relative-name file dir) (buffer-string)) files))))))
                  (dolist (file (directory-files-recursively dir "\\`[^.]"))
                    (let ((name (file-relative-name file dir)))
                      (unless (or (assoc name files) (find-buffer-visiting file))
                        (let ((text (with-temp-buffer (insert-file-contents file) (buffer-string))))
                          (unless (equal (gethash file before) text)
                            (push (cons name text) files))))))
                  (json-serialize
                   (list (cons 'files (let ((table (make-hash-table :test #'equal)))
                                        (dolist (pair files) (puthash (car pair) (cdr pair) table))
                                        table))
                         (cons 'prompts (vconcat (reverse org-lsp-prompts)))))))
            (error (format "ERROR: %S" err))))
    (org-roam-db--close-all)
    (with-temp-file out
      (set-buffer-file-coding-system 'utf-8)
      (insert result))))
"""


def lisp_template(template):
    """A template as `org-roam-capture-templates` writes it."""
    parts = [lisp_string(template["key"]), lisp_string(template["description"])]
    if "type" not in template and "target" not in template:
        return "(" + " ".join(parts) + ")"
    parts.append(template.get("type", "entry"))
    text = template.get("template", "")
    if isinstance(text, list):
        text = "\n".join(text)
    parts.append(lisp_string(text))
    target = template["target"]
    kind = target[0]
    if kind in ("file+olp",):
        form = f"(file+olp {lisp_string(target[1])} (" + " ".join(lisp_string(x) for x in target[2:]) + "))"
    elif kind == "file+head+olp":
        form = (f"(file+head+olp {lisp_string(target[1])} {lisp_string(target[2])} ("
                + " ".join(lisp_string(x) for x in target[3:]) + "))")
    elif kind == "file+datetree":
        form = f"(file+datetree {lisp_string(target[1])} {target[2] if len(target) > 2 else 'day'})"
    else:
        form = "(" + kind + " " + " ".join(lisp_string(x) for x in target[1:]) + ")"
    parts += [":target", form]
    for name in ("unnarrowed", "prepend", "immediate_finish", "empty_lines", "empty_lines_before", "empty_lines_after"):
        if name in template:
            value = template[name]
            text = "t" if value is True else "nil" if value is False else str(value)
            parts += [":" + name.replace("_", "-"), text]
    return "(" + " ".join(parts) + ")"


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
            for name, text in sorted(case.get("files", {}).items()):
                path = os.path.join(folder, name)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                if CURSOR in text:
                    cursor = (path, text.index(CURSOR))
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(text.replace(CURSOR, "", 1))
            target = os.path.join(directory, f"case-{index}", "out.json")
            outputs.append((case, target))
            templates = case.get("templates")
            templates_form = ("(list " + " ".join("'" + lisp_template(t) for t in templates) + ")") if templates else "nil"
            answers = "(list " + " ".join(lisp_string(a) for a in case.get("answers", [])) + ")"
            forms.append(
                f"(org-lsp-roam-capture {lisp_string(folder)} {lisp_string(target)} {lisp_string(case['title'])}"
                f" {templates_form} {lisp_string(case['keys']) if 'keys' in case else 'nil'} {answers}"
                f" {lisp_string(case.get('body', ''))} {lisp_string(case.get('finalize', 'find-file'))}"
                f" {lisp_string(cursor[0]) if cursor else 'nil'} {cursor[1] if cursor else 'nil'}"
                f" {lisp_string(case['node'][1]) if 'node' in case else 'nil'})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["faketime", f"{CLOCK}:00", "emacs", "--batch", "-Q", *load_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                text = handle.read()
            case["expected"] = text if text.startswith("ERROR:") else json.loads(text)
    return cases
