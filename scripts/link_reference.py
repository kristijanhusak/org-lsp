"""Run Emacs's link commands over test cases and record what they did.

A case is a JSON object:

    {
      "text": "* a¦\\n",                    `main.org`, `¦` where the cursor
                                            is (the start without one)
      "files": [["other.org", "* b\\n"]],    optional: more files beside it
                                            (in subdirectories too)
      "steps": [["store"]],                 what is done, in order:
          ["store"], ["store", "C-u"]       `org-store-link` (`C-c l`)
          ["annotation"]                    `(org-store-link nil)`, as
                                            capture's `%a` calls it: what it
                                            returns goes to `returned`
          ["insert", prefix, answers…]      `org-insert-link` (`C-c C-l`),
                                            prefix "" or "C-u" (up to three);
                                            the answers are given to its
                                            prompts in turn, null for the
                                            default or the text as it is
          ["insert-last"]                   `org-insert-last-stored-link`
          ["insert-all"], ["insert-all", "C-u"]   `org-insert-all-links`
          ["id"]                            `org-id-get-create`
          ["visit", "other.org"]            that file becomes the current
                                            buffer, at its start
          ["goto", "text"]                  the cursor to the start of that
                                            text
      "options": {"org_id_link_to_org_use_id": "t"}   optional
    }

`reference(cases)` adds `expected`: the files after the steps (the current
one with `¦` at the cursor), the stored links and what `annotation`
returned, as a JSON object; or `ERROR: …`. The files are in `~/org`, and
`~` is `/home/u`. New IDs are `id-1`, `id-2` and on.
Set ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import shutil
import subprocess

from todo_reference import lisp_string

CURSOR = "¦"
# The home directory Emacs is given; each case is in a directory of it.
BASE = "/tmp/org-lsp-link"

DRIVER = r"""
(require 'org)
(require 'ol)
(require 'org-id)
(require 'cl-lib)
(require 'json)
(setq org-id-track-globally nil)
(defvar org-lsp-answers nil "The answers to the prompts, in turn.")
(defvar org-lsp-ids 0)
(defun org-lsp-answer (default)
  (let ((answer (pop org-lsp-answers)))
    (if (eq answer :default) (or default "") answer)))
(defun org-lsp-link-case (dir main out offset steps)
  (setq org-stored-links nil org-link--insert-history nil org-lsp-ids 0)
  (let (returned buffer)
    (let ((result
           (condition-case err
               (cl-letf (((symbol-function 'org-id-new)
                          (lambda (&optional _) (format "id-%d" (cl-incf org-lsp-ids))))
                         ((symbol-function 'completing-read)
                          (lambda (_prompt _collection &optional _pred _req initial _hist default &rest _)
                            (let* ((default (if (consp default) (car default) default))
                                   (answer (org-lsp-answer (or default initial))))
                              (if (and (equal answer "") default) default answer))))
                         ((symbol-function 'read-string)
                          (lambda (_prompt &optional initial _hist default &rest _)
                            (let ((answer (org-lsp-answer initial)))
                              (if (and (equal answer "") default) default answer))))
                         ((symbol-function 'read-file-name)
                          (lambda (&rest _) (org-lsp-answer "")))
                         ((symbol-function 'sit-for) (lambda (&rest _) t)))
                 (setq buffer (find-file-noselect main))
                 (set-buffer buffer)
                 (goto-char (1+ offset))
                 (dolist (step steps)
                   (pcase (car step)
                     ("visit" (setq buffer (find-file-noselect (expand-file-name (cadr step) dir)))
                      (set-buffer buffer)
                      (goto-char (point-min)))
                     ("goto" (goto-char (point-min))
                      (search-forward (cadr step))
                      (goto-char (match-beginning 0)))
                     ("store" (let ((current-prefix-arg (and (cadr step) '(4))))
                                (call-interactively #'org-store-link)))
                     ("annotation" (push (or (ignore-errors (org-store-link nil)) "") returned))
                     ("insert"
                      (let ((current-prefix-arg
                             (pcase (cadr step)
                               ("C-u" '(4)) ("C-u C-u" '(16)) ("C-u C-u C-u" '(64))))
                            (org-lsp-answers (mapcar (lambda (a) (or a :default)) (cddr step))))
                        (call-interactively #'org-insert-link)))
                     ("insert-last" (call-interactively #'org-insert-last-stored-link))
                     ("insert-all" (let ((current-prefix-arg (and (cadr step) '(4))))
                                     (call-interactively #'org-insert-all-links)))
                     ("id" (org-id-get-create))))
                 (let ((files nil))
                   (dolist (b (buffer-list))
                     (when (buffer-file-name b)
                       (with-current-buffer b
                         (push (cons (file-relative-name (buffer-file-name b) dir)
                                     (if (eq b buffer)
                                         (concat (buffer-substring-no-properties (point-min) (point)) "¦"
                                                 (buffer-substring-no-properties (point) (point-max)))
                                       (buffer-substring-no-properties (point-min) (point-max))))
                               files))))
                   (json-encode
                    (list (cons "files" (sort files (lambda (a b) (string< (car a) (car b)))))
                          (cons "stored" (vconcat (mapcar (lambda (l) (vector (car l) (cadr l)))
                                                          org-stored-links)))
                          (cons "returned" (vconcat (reverse returned)))))))
             (error (format "ERROR: %s" (error-message-string err))))))
      (dolist (b (buffer-list))
        (when (buffer-file-name b)
          (with-current-buffer b (set-buffer-modified-p nil))
          (kill-buffer b)))
      (with-temp-file out (insert result)))))
"""

# The link options and their Emacs variables, as symbols or values.
OPTIONS = {
    "org_id_link_to_org_use_id": ("org-id-link-to-org-use-id", "symbol"),
    "org_link_context_for_files": ("org-link-context-for-files", "bool"),
    "org_id_link_use_context": ("org-id-link-use-context", "bool"),
    "org_id_link_consider_parent_id": ("org-id-link-consider-parent-id", "bool"),
    "org_link_file_path_type": ("org-link-file-path-type", "symbol"),
    "org_link_keep_stored_after_insertion": ("org-link-keep-stored-after-insertion", "bool"),
}


def bindings(options):
    out = []
    for key, value in (options or {}).items():
        name, kind = OPTIONS[key]
        if kind == "bool":
            out.append(f"({name} {'t' if value else 'nil'})")
        elif value in (None, False, "nil"):
            out.append(f"({name} nil)")
        else:
            out.append(f"({name} '{value})")
    return " ".join(out)


def lisp_steps(steps):
    def part(value):
        return "nil" if value is None else lisp_string(value)

    return "'(" + " ".join("(" + " ".join(part(value) for value in step) + ")" for step in steps) + ")"


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    shutil.rmtree(BASE, ignore_errors=True)
    os.makedirs(BASE)
    try:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            directory = os.path.join(BASE, str(index), "org")
            os.makedirs(directory)
            text = case["text"]
            offset = text.find(CURSOR)
            text = text.replace(CURSOR, "", 1)
            path = os.path.join(directory, "main.org")
            for name, other in case.get("files", []):
                target = os.path.join(directory, name)
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with open(target, "w", encoding="utf-8") as handle:
                    handle.write(other)
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text)
            out = os.path.join(BASE, str(index), "out")
            outputs.append((case, out, os.path.join(BASE, str(index))))
            forms.append(
                f"(let ((default-directory {lisp_string(directory + '/')}) (process-environment"
                f" (cons {lisp_string('HOME=' + os.path.join(BASE, str(index)))} process-environment))"
                f" (abbreviated-home-dir nil) {bindings(case.get('options'))})\n"
                f"  (setenv \"HOME\" {lisp_string(os.path.join(BASE, str(index)))})\n"
                f"  (org-lsp-link-case {lisp_string(directory)} {lisp_string(path)} {lisp_string(out)}"
                f" {max(offset, 0)} {lisp_steps(case['steps'])}))"
            )
        driver = os.path.join(BASE, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
        )
        for case, out, home in outputs:
            with open(out, encoding="utf-8") as handle:
                result = handle.read()
            # `~` is the case's directory: /home/u for both programs.
            case["expected"] = result.replace(home, "/home/u")
    finally:
        shutil.rmtree(BASE, ignore_errors=True)
    return cases


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


if __name__ == "__main__":
    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "link", "cases.json")
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
