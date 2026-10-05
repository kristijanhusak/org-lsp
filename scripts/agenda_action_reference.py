"""Run Emacs's agenda commands that change entries, and record the files.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",        the pretended current time
      "files": {"work.org": "* TODO a\\n"},  the agenda files
      "view": "agenda",                   or "todo"
      "span": "week",                     for the agenda: day, week, …
      "line": "Sched. 2x:  TODO a",       text of the agenda line to act on
      "command": "date-later",            see COMMANDS below
      "count": 2                          optional: the prefix argument
    }

`reference(cases)` adds `expected`: the files after the command, or
`ERROR: …` when Emacs refuses. Emacs runs under faketime, so both programs
see the same clock. Set ORG_LISP_DIR to the `lisp` directory of an Org
checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

import todo_reference
from todo_reference import lisp_string, load, save  # noqa: F401

# The server's command names and the Emacs forms that do the same.
COMMANDS = {
    "todo-next": "(org-agenda-todo 'right)",
    "todo-previous": "(org-agenda-todo 'left)",
    "priority-up": "(call-interactively 'org-agenda-priority-up)",
    "priority-down": "(call-interactively 'org-agenda-priority-down)",
    "date-later": "(call-interactively 'org-agenda-do-date-later)",
    "date-earlier": "(call-interactively 'org-agenda-do-date-earlier)",
}

DRIVER = r"""
;; Loaded here, not on demand inside a case's `let': its options take
;; their defaults from variables the cases bind.
(require 'org-agenda)
(require 'org-clock)
(setq org-agenda-tags-column -80
      org-agenda-window-setup 'current-window
      org-agenda-sticky nil)
(defun org-lsp-agenda-case (files out view span needle count form)
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let ((result
         (condition-case err
             (let ((org-agenda-files files)
                   (org-agenda-span span))
               (if (equal view "todo") (org-todo-list) (org-agenda-list))
               (with-current-buffer org-agenda-buffer-name
                 (goto-char (point-min))
                 (unless (search-forward needle nil t) (error "No such agenda line"))
                 (beginning-of-line)
                 (let ((current-prefix-arg count)
                       (this-command 'org-lsp-command)
                       (last-command 'ignore))
                   (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t)))
                     (eval form)
                     (org-lsp-finish-notes))))
               (mapconcat
                (lambda (file)
                  ;; Not `find-file-noselect': under a faked clock it takes
                  ;; the file for changed on disk and asks what to do.
                  (with-current-buffer (get-file-buffer file)
                    (concat (file-name-nondirectory file) "\0" (buffer-string))))
                files "\0"))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))))
"""


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for clock, group in by_clock.items():
            forms = [todo_reference.DRIVER, DRIVER]
            outputs = []
            for index, case in enumerate(group):
                folder = os.path.join(directory, f"case-{index}")
                os.makedirs(folder)
                paths = []
                for name, text in sorted(case["files"].items()):
                    paths.append(os.path.join(folder, name))
                    with open(paths[-1], "w", encoding="utf-8") as handle:
                        handle.write(text)
                target = os.path.join(folder, "out")
                outputs.append((case, target))
                span = case.get("span", "week")
                count = case.get("count")
                forms.append(
                    f"(let ({todo_reference.bindings(case.get('options'))})\n"
                    f"  (org-lsp-agenda-case (list {' '.join(lisp_string(path) for path in paths)}) {lisp_string(target)}"
                    f" {lisp_string(case.get('view', 'agenda'))} {span if isinstance(span, int) else chr(39) + span}"
                    f" {lisp_string(case['line'])} {'nil' if count is None else count}"
                    f" '{COMMANDS[case['command']]}))"
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
                env=dict(os.environ, TZ="UTC"),
            )
            for case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    result = handle.read()
                if result.startswith("ERROR:"):
                    case["expected"] = result
                else:
                    parts = result.split("\0")
                    case["expected"] = dict(zip(parts[0::2], parts[1::2]))
    return cases
