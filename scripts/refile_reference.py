"""Run Emacs's `org-refile` over test cases and record what it does.

A case is a JSON object:

    {
      "files": {"a.org": "* A¦\\n* B\\n"},   Org files; ¦ marks the cursor,
                                            in exactly one of them
      "answer": "B",                        the target the user picks, or
      "answerIndex": 2,                     the one at this place among those
                                            offered (each name counted once)
      "note": "Moved",                      optional: the note `org-log-refile`
                                            asks for
      "copy": true,                         optional: `org-refile-copy`, the
                                            entry kept where it was
      "options": {                          optional: server settings
        "org_refile_targets": [{"files": "agenda", "maxlevel": 2}],
        "org_refile_use_outline_path": "file",
        "org_reverse_note_order": true,
        "org_tags_column": -40
      }
    }

`reference(cases)` adds `expected` to each case: `{"offered": [...],
"files": {...}}`, the targets offered in the order Emacs lists them and
the files after the command, or `ERROR: …` when Emacs refuses. All the
case's files are the agenda files. Emacs runs under faketime, at
2026-05-18 10:30, for the time of a log note. Set ORG_LISP_DIR to the `lisp`
directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

import todo_reference
from todo_reference import CURSOR, lisp_string, load, save  # noqa: F401

CLOCK = "2026-05-18 10:30"

DRIVER = r"""
(require 'org)
(require 'org-refile)
(require 'cl-lib)
;; Under a faked clock files seem changed on disk; nobody is there to ask,
;; and the buffers are kept as they are.
(load "userlock" nil t)
(defun ask-user-about-supersession-threat (&rest _) nil)
(defun userlock--ask-user-about-supersession-threat (&rest _) nil)
(advice-add 'yes-or-no-p :override (lambda (&rest _) nil))
(defun org-lsp-refile-case (files cursor-file offset answer index out &optional note copy)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let* ((offered nil)
         (result
          (condition-case err
              (let ((org-agenda-files files))
                (dolist (file files) (find-file-noselect file))
                (with-current-buffer (find-file-noselect cursor-file)
                  (goto-char (1+ offset))
                  (cl-letf (((symbol-function 'completing-read)
                             (lambda (_prompt collection &rest _)
                               (setq offered (mapcar (lambda (x) (if (consp x) (car x) x)) collection))
                               (or answer
                                   (let ((names (delete-dups (copy-sequence offered))))
                                     (nth (mod index (max 1 (length names))) names))))))
                    (org-refile (and copy 3))
                    (let ((org-lsp-note-text (or note "")))
                      (org-lsp-finish-notes))))
                (mapconcat
                 (lambda (file)
                   (with-current-buffer (find-file-noselect file)
                     (concat (file-name-nondirectory file) "\0" (buffer-string))))
                 files "\0"))
            (error (format "ERROR: %S" err)))))
    (with-temp-file out
      (insert (mapconcat #'identity offered "\1") "\2" result))))
"""


def targets_value(targets, paths):
    """The `org-refile-targets` for the server's `refileTargets`."""
    entries = []
    for target in targets:
        files = target.get("files", "current")
        if files == "current":
            files_form = "nil"
        elif files == "agenda":
            # The symbol itself: Org reads `org-agenda-files' then.
            files_form = "'org-agenda-files"
        else:
            names = files if isinstance(files, list) else [files]
            files_form = "(list " + " ".join(lisp_string(paths[name]) for name in names) + ")"
        if target.get("all"):
            spec = "t"
        elif "level" in target:
            spec = f"'(:level . {target['level']})"
        elif "maxlevel" in target:
            spec = f"'(:maxlevel . {target['maxlevel']})"
        elif "tag" in target:
            spec = f"(cons :tag {lisp_string(target['tag'])})"
        elif "todo" in target:
            spec = f"(cons :todo {lisp_string(target['todo'])})"
        else:
            raise ValueError(f"no Emacs form for target {target}")
        entries.append(f"(cons {files_form} {spec})")
    return "(list " + " ".join(entries) + ")"


def bindings(options, paths):
    out = {
        "org-refile-targets": "nil",
        "org-refile-use-outline-path": "nil",
        "org-outline-path-complete-in-steps": "nil",
        "org-reverse-note-order": "nil",
        "org-tags-column": "-77",
        "org-log-refile": "nil",
    }
    symbol = lambda value: "'" + value if isinstance(value, str) else ("t" if value else "nil")
    for key, value in (options or {}).items():
        if key == "org_refile_targets":
            out["org-refile-targets"] = targets_value(value, paths)
        elif key == "org_refile_use_outline_path":
            out["org-refile-use-outline-path"] = (
                "t" if value is True else "nil" if value is False else "'" + value
            )
        elif key == "org_reverse_note_order":
            out["org-reverse-note-order"] = "t" if value else "nil"
        elif key == "org_log_refile":
            out["org-log-refile"] = symbol(value)
        elif key == "org_log_into_drawer":
            out["org-log-into-drawer"] = "t" if value is True else lisp_string(value) if value else "nil"
        elif key == "org_tags_column":
            out["org-tags-column"] = str(value)
        elif key == "org_adapt_indentation":
            out["org-adapt-indentation"] = (
                "t" if value is True else "nil" if value is False else "'" + value
            )
        else:
            raise ValueError(f"no Emacs setting for option {key}")
    return " ".join(f"({name} {value})" for name, value in out.items())


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [todo_reference.DRIVER, DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            folder = os.path.join(directory, f"case-{index}")
            os.makedirs(folder)
            paths = {}
            cursor = None
            for name, text in sorted(case["files"].items()):
                paths[name] = os.path.join(folder, name)
                if CURSOR in text:
                    cursor = (paths[name], text.index(CURSOR))
                with open(paths[name], "w", encoding="utf-8") as handle:
                    handle.write(text.replace(CURSOR, "", 1))
            target = os.path.join(folder, "out")
            outputs.append((case, target))
            files = " ".join(lisp_string(paths[name]) for name in sorted(paths))
            forms.append(
                f"(let ({bindings(case.get('options'), paths)})\n"
                f"  (org-lsp-refile-case (list {files}) {lisp_string(cursor[0])} {cursor[1]}"
                f" {lisp_string(case['answer']) if 'answer' in case else 'nil'} {case.get('answerIndex', 0)}"
                f" {lisp_string(target)} {lisp_string(case['note']) if 'note' in case else 'nil'}"
                f" {'t' if case.get('copy') else 'nil'}))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["faketime", f"{CLOCK}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                offered, result = handle.read().split("\2", 1)
            offered = offered.split("\1") if offered else []
            if result.startswith("ERROR:"):
                case["expected"] = result
            else:
                parts = result.split("\0")
                case["expected"] = {"offered": offered, "files": dict(zip(parts[0::2], parts[1::2]))}
    return cases
