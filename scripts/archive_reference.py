"""Run Emacs's archiving commands over test cases and record what they do.

A case is a JSON object:

    {
      "files": {"a.org": "* A¦\\n"},   Org files; ¦ marks the cursor, in
                                       exactly one of them
      "command": "subtree",            "subtree" (`org-archive-subtree`),
                                       "sibling" (`org-archive-to-archive-
                                       sibling`), "tag" (`org-toggle-archive-
                                       tag`), "done" (`org-archive-subtree`
                                       with C-u) or "tagDone"
                                       (`org-toggle-archive-tag` with C-u)
      "answers": [true, false],        optional: the answers to the
                                       questions "done" and "tagDone" ask;
                                       yes when they run out
      "clock": "2026-05-18 10:30",     optional: the pretended current time
      "options": {...}                 optional: server settings
    }

`reference(cases)` adds `expected` to each case: `{"files": {...},
"message": ..., "questions": [...]}`, every file of the case's folder after
the command (the archive file Emacs created among them) and what it said
and asked, or `ERROR: …` when Emacs refuses. The files are in
`/home/notes`, and the home directory is `/home`; a name may have
directories (`archive/a.org`). Set ORG_LISP_DIR to the
`lisp` directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import os
import subprocess
import tempfile

from todo_reference import CURSOR, lisp_string, lisp_value, load, save  # noqa: F401

HOME = "/home"
CLOCK = "2026-05-18 10:30"

DRIVER = r"""
(require 'org)
(require 'org-archive)
(require 'cl-lib)
(defvar org-lsp-messages nil)
(defvar org-lsp-questions nil)
(defvar org-lsp-answers nil)
(advice-add 'message :around
            (lambda (fn &rest args)
              (when (car args)
                (push (apply #'format-message args) org-lsp-messages))
              (apply fn args)))
(defun org-lsp-archive-case (home folder cursor-file offset command answers out)
  (setenv "HOME" home)
  (setq abbreviated-home-dir nil)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (setq org-lsp-messages nil org-lsp-questions nil org-lsp-answers answers)
  (let* ((point nil)
         (result
          (condition-case err
              (progn
                (with-current-buffer (find-file-noselect cursor-file)
                  (goto-char (1+ offset))
                  (cl-letf (((symbol-function 'y-or-n-p)
                             (lambda (prompt)
                               (push prompt org-lsp-questions)
                               (if org-lsp-answers
                                   (eq (pop org-lsp-answers) t)
                                 t))))
                    (pcase command
                      ("subtree" (org-archive-subtree))
                      ("done" (org-archive-subtree '(4)))
                      ("sibling" (org-archive-to-archive-sibling))
                      ("tag" (org-toggle-archive-tag))
                      ("tagDone" (org-toggle-archive-tag '(4)))))
                  (setq point (1- (point))))
                ;; Every file of the folder: as its buffer has it, or as it
                ;; is on disk.
                (mapconcat
                 (lambda (file)
                   (let ((buffer (get-file-buffer file)))
                     (concat (file-relative-name file folder) "\0"
                             (if buffer
                                 (with-current-buffer buffer (buffer-string))
                               (with-temp-buffer
                                 (insert-file-contents file)
                                 (buffer-string))))))
                 (cl-remove-if
                  (lambda (file) (or (string-suffix-p "~" file)
                                     (equal (file-name-nondirectory file) "out")))
                  (directory-files-recursively folder "\\`[^.]"))
                 "\0"))
            (error (format "ERROR: %S" err)))))
    (with-temp-file out
      (insert (format "%s" point) "\3"
              (mapconcat #'identity (reverse org-lsp-messages) "\1") "\2"
              (mapconcat #'identity (reverse org-lsp-questions) "\1") "\2"
              result))))
"""


def bindings(options):
    """The Emacs variables that correspond to the server options."""
    out = {
        "org-tags-column": "-77",
        "org-todo-keywords": "'((sequence \"TODO\" \"DONE\"))",
        "org-archive-location": lisp_string("%s_archive::"),
        "org-log-done": "nil",
    }
    for key, value in (options or {}).items():
        if key == "tagsColumn":
            out["org-tags-column"] = str(value)
        elif key == "adaptIndentation":
            out["org-adapt-indentation"] = "'headline-data" if value == "headline-data" else lisp_value(value)
        elif key == "todoKeywords":
            out["org-todo-keywords"] = "'((sequence " + " ".join(lisp_string(word) for word in value) + "))"
        elif key == "archiveLocation":
            out["org-archive-location"] = lisp_string(value)
        elif key == "archiveReversedOrder":
            out["org-archive-reversed-order"] = lisp_value(value)
        elif key == "archiveMarkDone":
            out["org-archive-mark-done"] = lisp_value(value)
        elif key == "archiveSubtreeAddInheritedTags":
            out["org-archive-subtree-add-inherited-tags"] = "'infile" if value == "infile" else lisp_value(value)
        elif key == "archiveSaveContextInfo":
            out["org-archive-save-context-info"] = "'(" + " ".join(value) + ")"
        elif key == "archiveSiblingHeading":
            out["org-archive-sibling-heading"] = lisp_string(value)
        elif key == "archiveFileHeaderFormat":
            out["org-archive-file-header-format"] = lisp_value(value)
        elif key == "hierarchicalTodoStatistics":
            out["org-hierarchical-todo-statistics"] = lisp_value(value)
        else:
            raise ValueError(f"no Emacs setting for option {key}")
    return " ".join(f"({name} {value})" for name, value in out.items())


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    by_clock = {}
    for index, case in enumerate(cases):
        by_clock.setdefault(case.get("clock", CLOCK), []).append((index, case))
    for clock, group in by_clock.items():
        with tempfile.TemporaryDirectory() as directory:
            directory = os.path.realpath(directory)
            forms = [DRIVER]
            outputs = []
            for index, case in group:
                home = os.path.join(directory, f"case-{index}")
                folder = os.path.join(home, "notes")
                os.makedirs(folder)
                cursor = None
                for name, text in sorted(case["files"].items()):
                    path = os.path.join(folder, name)
                    os.makedirs(os.path.dirname(path), exist_ok=True)
                    if CURSOR in text:
                        cursor = (path, text.index(CURSOR))
                    with open(path, "w", encoding="utf-8") as handle:
                        handle.write(text.replace(CURSOR, "", 1))
                target = os.path.join(folder, "out")
                outputs.append((index, case, target))
                answers = "'(" + " ".join("t" if answer else "nil" for answer in case.get("answers", [])) + ")"
                forms.append(
                    f"(let ({bindings(case.get('options'))})\n"
                    f"  (org-lsp-archive-case {lisp_string(home)} {lisp_string(folder)} {lisp_string(cursor[0])} {cursor[1]}"
                    f" {lisp_string(case['command'])} {answers} {lisp_string(target)}))"
                )
            driver = os.path.join(directory, "driver.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write("\n".join(forms))
            subprocess.run(
                ["faketime", f"{clock}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            for index, case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    raw = handle.read()
                raw = raw.replace(os.path.join(directory, f"case-{index}"), HOME)
                point, rest = raw.split("\3", 1)
                messages, questions, result = rest.split("\2", 2)
                if result.startswith("ERROR:"):
                    case["expected"] = result
                    continue
                parts = result.split("\0")
                messages = messages.split("\1") if messages else []
                case["expected"] = {
                    "files": dict(zip(parts[0::2], parts[1::2])),
                    "message": messages[-1] if messages else None,
                    "questions": questions.split("\1") if questions else [],
                    "point": int(point) if point != "nil" else None,
                }
    return cases
