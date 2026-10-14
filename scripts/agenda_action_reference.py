"""Run Emacs's agenda commands that change entries, and record the files.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",        the pretended current time
      "files": {"work.org": "* TODO a\\n"},  the agenda files
      "view": "agenda",                   or "todo"
      "span": "week",                     for the agenda: day, week, …
      "line": "Sched. 2x:  TODO a",       text of the agenda line to act on
      "command": "date-later",            see COMMANDS below
      "count": 2,                         optional: the prefix argument
      "answer": "fri"                     for commands that ask for a date
    }

A bulk action (`"command": "bulk"`) marks the lines of `"marks"` (texts, as
`line`) with `m`, then runs `B` on `line` with the key `"action"`, and
`"answer"` for what that asks (a state, tag, date, refile target or
number of days).

`reference(cases)` adds `expected`: the files after the command, or
`ERROR: …` when Emacs refuses. A case with an answer also gets `prompt`,
the text the date prompt offered before the answer replaced it. A command
that changes the view (`agenda-goto-date`), or a case with `"view_after":
true`, also gets `agenda`, the text of the agenda after it as it shows
(text a `display` property covers shows as that: the new date of
`org-agenda-show-new-time`), and `cursor`, the line the cursor is left on
(from 0). Emacs runs under faketime, so both programs
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
    "date-later-hours": "(call-interactively 'org-agenda-date-later-hours)",
    "date-earlier-hours": "(call-interactively 'org-agenda-date-earlier-hours)",
    "date-later-minutes": "(call-interactively 'org-agenda-date-later-minutes)",
    "date-earlier-minutes": "(call-interactively 'org-agenda-date-earlier-minutes)",
    "schedule": "(call-interactively 'org-agenda-schedule)",
    "deadline": "(call-interactively 'org-agenda-deadline)",
    "agenda-goto-date": "(call-interactively 'org-agenda-goto-date)",
    "priority": "(call-interactively 'org-agenda-priority)",
}

# Commands that change the view rather than the files.
VIEW_COMMANDS = {"agenda-goto-date"}

DRIVER = r"""
;; Loaded here, not on demand inside a case's `let': its options take
;; their defaults from variables the cases bind.
(require 'org-agenda)
(require 'org-clock)
(setq org-agenda-tags-column -80
      org-agenda-window-setup 'current-window
      org-agenda-sticky nil)
(defvar org-lsp-prompt nil)
(defun org-lsp-shown (text)
  "TEXT as it shows: what a `display' string covers is that string."
  (let ((at 0) (out ""))
    (while (< at (length text))
      (let* ((next (next-single-property-change at 'display text (length text)))
             (display (get-text-property at 'display text)))
        (setq out (concat out (if (stringp display) display (substring-no-properties text at next))))
        (setq at next)))
    out))
(defun org-lsp-agenda-case (files out view span needle count form answer)
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil)
  (setq org-lsp-prompt "")
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
                       (last-command 'ignore)
                       (org-read-date-popup-calendar nil))
                   (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t))
                             ((symbol-function 'read-string)
                              (lambda (_prompt &optional initial &rest _)
                                (setq org-lsp-prompt (or initial ""))
                                answer))
                             ((symbol-function 'read-char-exclusive)
                              (lambda (&rest _) (string-to-char answer))))
                     (eval form)
                     (org-lsp-finish-notes)))
                 (let ((view (with-current-buffer org-agenda-buffer-name
                               (format "%d\n%s" (1- (line-number-at-pos)) (org-lsp-shown (buffer-string))))))
                   (with-temp-file (concat out ".view") (insert view))))
               (mapconcat
                (lambda (file)
                  ;; Not `find-file-noselect': under a faked clock it takes
                  ;; the file for changed on disk and asks what to do.
                  (with-current-buffer (get-file-buffer file)
                    (concat (file-name-nondirectory file) "\0" (buffer-string))))
                files "\0"))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))
    (with-temp-file (concat out ".prompt") (insert org-lsp-prompt))))
"""


def bulk_form(case):
    """`m` on each line of the case's marks, then `B` and its answers on
    the case's line."""
    marks = " ".join(lisp_string(mark) for mark in case["marks"])
    answer = lisp_string(case.get("answer", ""))
    return (
        f"(let ((here (point)))"
        f" (set-window-buffer (selected-window) (current-buffer))"
        f" (dolist (needle (list {marks}))"
        f"   (goto-char (point-min)) (search-forward needle) (beginning-of-line) (org-agenda-bulk-mark))"
        f" (goto-char here)"
        f" (cl-letf (((symbol-function 'read-char-exclusive) (lambda (&rest _) ?{case['action']}))"
        f"           ((symbol-function 'completing-read) (lambda (&rest _) {answer}))"
        f"           ((symbol-function 'read-number) (lambda (&rest _) (string-to-number {answer}))))"
        f"   (org-agenda-bulk-action)))"
    )


def without_hints(cursor, agenda):
    """The agenda without Emacs's hints about its key bindings, which the
    server leaves out (see `agenda_reference.clean`), and the cursor's
    line in it."""
    kept = []
    skipping = False
    for number, line in enumerate(agenda.split("\n")):
        if line.startswith("Press ") and ("to search again" in line or "to add/sub word" in line):
            skipping = True
        elif not (skipping and line.startswith("                     ")):
            skipping = False
            kept.append(line)
            continue
        if number < cursor:
            cursor -= 1
    return cursor, "\n".join(kept)


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for index, case in enumerate(cases):
        # A bulk action leaves windows that the next case's commands trip
        # over: each runs in an Emacs of its own.
        key = (case["clock"], index if case["command"] == "bulk" else None)
        by_clock.setdefault(key, []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for number, ((clock, _), group) in enumerate(by_clock.items()):
            forms = [todo_reference.DRIVER, DRIVER]
            outputs = []
            for index, case in enumerate(group):
                folder = os.path.join(directory, f"group-{number}", f"case-{index}")
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
                form = COMMANDS.get(case["command"]) or bulk_form(case)
                forms.append(
                    f"(let ({todo_reference.bindings(case.get('options'))})\n"
                    f"  (org-lsp-agenda-case (list {' '.join(lisp_string(path) for path in paths)}) {lisp_string(target)}"
                    f" {lisp_string(case.get('view', 'agenda'))} {span if isinstance(span, int) else chr(39) + span}"
                    f" {lisp_string(case['line'])} {'nil' if count is None else count}"
                    f" '{form} {lisp_string(case.get('answer', ''))}))"
                )
            driver = os.path.join(directory, f"driver-{number}.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write("\n".join(forms))
            subprocess.run(
                ["faketime", f"{clock}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=dict(os.environ, TZ="UTC"),
                timeout=1200,
            )
            for case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    result = handle.read()
                if "answer" in case:
                    with open(target + ".prompt", encoding="utf-8") as handle:
                        case["prompt"] = handle.read()
                wants_view = case["command"] in VIEW_COMMANDS or case.get("view_after")
                if wants_view and os.path.exists(target + ".view"):
                    with open(target + ".view", encoding="utf-8") as handle:
                        cursor, agenda = handle.read().split("\n", 1)
                    case["cursor"], case["agenda"] = without_hints(int(cursor), agenda)
                if result.startswith("ERROR:"):
                    case["expected"] = result
                else:
                    parts = result.split("\0")
                    case["expected"] = dict(zip(parts[0::2], parts[1::2]))
    return cases
