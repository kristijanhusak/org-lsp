"""Run Emacs's `org-todo` over test cases and record what it does.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",     the pretended current time
      "text": "* TODO a¦\\n",          an Org document; ¦ marks the cursor
      "command": "next",               "next", "previous", {"state": "DONE"},
                                       "yesterday": `org-todo-yesterday` to
                                       the next state,
                                       or {"key": "d"}: `org-todo` with its
                                       fast selection answered with that key;
                                       "checkbox": `org-toggle-checkbox`;
                                       "note": `org-add-note`
      "note": "Waiting",               optional: the text of the note, if the
                                       change asks for one
      "options": {"org_log_done": "time"}   optional: server settings
    }

A change Org blocks (`org-enforce-todo-dependencies`) also gets
`blocked`, what Org says about it.

`reference(cases)` adds `expected`, the document after the command, to each
case, or `QUIT` when the key quits the fast selection. Emacs runs under faketime, so both programs see the same clock. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that version
instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

CURSOR = "¦"

DRIVER = r"""
(require 'org)
(require 'org-clock)
(require 'cl-lib)
;; The blockers of `org-enforce-todo-dependencies' and its checkbox
;; sibling, in the order Org installs them when it loads with both set (the
;; checkboxes' runs first); Org mode takes out those whose option is nil.
(defun org-lsp-blockers ()
  (remove-hook 'org-blocker-hook #'org-block-todo-from-children-or-siblings-or-parent)
  (remove-hook 'org-blocker-hook #'org-block-todo-from-checkboxes)
  (add-hook 'org-blocker-hook #'org-block-todo-from-children-or-siblings-or-parent)
  (add-hook 'org-blocker-hook #'org-block-todo-from-checkboxes))
(defvar org-lsp-blocked nil)
(advice-add 'message :around
            (lambda (f &rest args)
              (let ((said (and (car args) (apply #'format-message args))))
                (when (and said (string-prefix-p "TODO state change from" said))
                  (setq org-lsp-blocked said)))
              (apply f args)))
(defvar org-lsp-note-text ""
  "What the user writes in a note buffer.")
(defun org-lsp-finish-notes ()
  "Store a pending log note with the text `org-lsp-note-text'."
  (when (memq 'org-add-log-note post-command-hook)
    (org-add-log-note)
    (when (get-buffer "*Org Note*")
      (with-current-buffer "*Org Note*"
        (goto-char (point-max))
        (insert org-lsp-note-text)
        (org-ctrl-c-ctrl-c)))))
(defun org-lsp-todo-case (in out offset arg &optional key note)
  ;; A case that stopped with an error may have left a note pending.
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil org-lsp-blocked nil)
  (org-lsp-blockers)
  (let ((result
         (condition-case err
             (with-temp-buffer
               (insert-file-contents in)
               (org-mode)
               (goto-char (1+ offset))
               ;; A long catch-up asks for confirmation; say yes.
               (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t))
                         ((symbol-function 'read-char-exclusive)
                          (lambda (&rest _) (string-to-char key))))
                 (let ((org-lsp-note-text (or note "")))
                   (cond ((eq arg 'checkbox) (org-toggle-checkbox))
                         ((eq arg 'note) (org-add-note))
                         ((eq arg 'yesterday) (org-todo-yesterday 'right))
                         ;; Cycling, pressed once: a `type' keyword goes
                         ;; to done (`this-command' is not `last-command').
                         ((eq arg 'cycle)
                          (let ((this-command 'org-todo) (last-command 'ignore))
                            (org-todo nil)))
                         (t (org-todo arg)))
                   (org-lsp-finish-notes)))
               (buffer-string))
           (quit "QUIT")
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))
    (with-temp-file (concat out ".blocked") (insert (or org-lsp-blocked "")))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_value(value):
    if value is True:
        return "t"
    if value is False or value is None:
        return "nil"
    if isinstance(value, int):
        return str(value)
    return lisp_string(value)


# `org-log-note-headings` as Org has it.
NOTE_HEADINGS = {
    "done": "CLOSING NOTE %t",
    "state": "State %-12s from %-12S %t",
    "note": "Note taken on %t",
    "reschedule": "Rescheduled from %S on %t",
    "delschedule": "Not scheduled, was %S on %t",
    "redeadline": "New deadline from %S on %t",
    "deldeadline": "Removed deadline, was %S on %t",
    "refile": "Refiled on %t",
    "clock_out": "",
}


def bindings(options):
    """The Emacs variables that correspond to the server options."""
    symbol = lambda value: "'" + value if isinstance(value, str) else lisp_value(value)
    out = {
        "org-log-done": "nil",
        "org-log-repeat": "'time",
        "org-log-reschedule": "nil",
        "org-log-redeadline": "nil",
        "org-log-into-drawer": "nil",
        "org-tags-column": "-77",
        "org-todo-repeat-to-state": "nil",
        "org-todo-keywords": "'((sequence \"TODO\" \"DONE\"))",
    }
    for key, value in (options or {}).items():
        if key == "org_log_done":
            out["org-log-done"] = symbol(value)
        elif key == "org_log_repeat":
            out["org-log-repeat"] = symbol(value)
        elif key == "org_log_reschedule":
            out["org-log-reschedule"] = symbol(value)
        elif key == "org_log_redeadline":
            out["org-log-redeadline"] = symbol(value)
        elif key == "org_enforce_todo_dependencies":
            out["org-enforce-todo-dependencies"] = lisp_value(value)
        elif key == "org_enforce_todo_checkbox_dependencies":
            out["org-enforce-todo-checkbox-dependencies"] = lisp_value(value)
        elif key == "org_log_into_drawer":
            out["org-log-into-drawer"] = lisp_value(value)
        elif key == "org_tags_column":
            out["org-tags-column"] = str(value)
        elif key in ("org_log_states_order_reversed", "org_log_state_notes_insert_after_drawers", "org_log_done_with_time"):
            out[key.replace("_", "-")] = lisp_value(value)
        elif key in ("org_hierarchical_todo_statistics", "org_closed_keep_when_no_todo"):
            out[key.replace("_", "-")] = lisp_value(value)
        elif key == "org_checkbox_hierarchical_statistics":
            out["org-checkbox-hierarchical-statistics"] = lisp_value(value)
        elif key == "org_list_automatic_rules":
            rules = " ".join(f"({rule} . {lisp_value(on)})" for rule, on in value.items())
            out["org-list-automatic-rules"] = f"'({rules})"
        elif key == "org_list_two_spaces_after_bullet_regexp":
            out["org-list-two-spaces-after-bullet-regexp"] = lisp_string(value) if value else "nil"
        elif key == "org_provide_todo_statistics":
            if value == "all-headlines":
                out["org-provide-todo-statistics"] = "'all-headlines"
            elif isinstance(value, list) and value and isinstance(value[0], list):
                lists = " ".join("(" + " ".join(lisp_string(word) for word in part) + ")" for part in value)
                out["org-provide-todo-statistics"] = "'(" + lists + ")"
            elif isinstance(value, list):
                out["org-provide-todo-statistics"] = "'(" + " ".join(lisp_string(word) for word in value) + ")"
            else:
                out["org-provide-todo-statistics"] = lisp_value(value)
        elif key == "org_todo_state_tags_triggers":
            entries = []
            for state, changes in value.items():
                head = state if state in ("todo", "done") else lisp_string(state)
                tags = " ".join(
                    f"({lisp_string(change[1:])} . t)" if change.startswith("+") else f"({lisp_string(change[1:])})"
                    for change in changes
                )
                entries.append(f"({head} {tags})")
            out["org-todo-state-tags-triggers"] = "'(" + " ".join(entries) + ")"
        elif key == "org_log_note_headings":
            # Org's headings, with those the option gives in their place.
            headings = dict(NOTE_HEADINGS, **value)
            out["org-log-note-headings"] = "'(" + " ".join(
                f"({name.replace('_', '-')} . {lisp_string(text)})" for name, text in headings.items()
            ) + ")"
        elif key == "org_agenda_move_date_from_past_immediately_to_today":
            out["org-agenda-move-date-from-past-immediately-to-today"] = lisp_value(value)
        elif key in ("org_use_effective_time", "org_use_last_clock_out_time_as_effective_time"):
            out[key.replace("_", "-")] = lisp_value(value)
        elif key == "org_extend_today_until":
            out["org-extend-today-until"] = str(value)
        elif key == "org_todo_repeat_to_state":
            out["org-todo-repeat-to-state"] = lisp_value(value)
        elif key == "org_adapt_indentation":
            out["org-adapt-indentation"] = "'headline-data" if value == "headline-data" else lisp_value(value)
        elif key == "org_todo_keywords":
            # One sequence, or several, each perhaps starting with
            # `sequence` or `type`.
            sequences = value if value and isinstance(value[0], list) else [value]
            def sequence(words):
                kind = words[0] if words and words[0] in ("sequence", "type") else None
                words = words[1:] if kind else words
                return "(" + (kind or "sequence") + " " + " ".join(lisp_string(word) for word in words) + ")"
            out["org-todo-keywords"] = "'(" + " ".join(sequence(words) for words in sequences) + ")"
        else:
            raise ValueError(f"no Emacs setting for option {key}")
    return " ".join(f"({name} {value})" for name, value in out.items())


def command_argument(command):
    if command == "checkbox":
        return "'checkbox"
    if command == "note":
        return "'note"
    if command == "yesterday":
        return "'yesterday"
    if command == "cycle":
        return "'cycle"
    if command in ("nextset", "previousset"):
        return "'" + command
    if command == "next":
        return "'right"
    if command == "previous":
        return "'left"
    if "key" in command:
        return "nil"
    return lisp_string(command["state"])


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for clock, group in by_clock.items():
            forms = [DRIVER]
            outputs = []
            for index, case in enumerate(group):
                offset = case["text"].index(CURSOR)
                source = os.path.join(directory, f"in-{index}.org")
                target = os.path.join(directory, f"out-{index}.org")
                with open(source, "w", encoding="utf-8") as handle:
                    handle.write(case["text"].replace(CURSOR, "", 1))
                outputs.append((case, target))
                forms.append(
                    f"(let ({bindings(case.get('options'))})\n"
                    f"  (org-lsp-todo-case {lisp_string(source)} {lisp_string(target)} {offset}"
                    f" {command_argument(case['command'])}"
                    f" {lisp_string(case['command']['key']) if isinstance(case['command'], dict) and 'key' in case['command'] else 'nil'}"
                    f" {lisp_string(case['note']) if 'note' in case else 'nil'}))"
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
                timeout=1200,
            )
            for case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = handle.read()
                # A change Org blocked: what it said (Emacs in batch says it
                # rather than signal it).
                with open(target + ".blocked", encoding="utf-8") as handle:
                    blocked = handle.read()
                if blocked:
                    case["blocked"] = blocked
    return cases


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
