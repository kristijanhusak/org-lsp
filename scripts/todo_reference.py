"""Run Emacs's `org-todo` over test cases and record what it does.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",     the pretended current time
      "text": "* TODO a¦\\n",          an Org document; ¦ marks the cursor
      "command": "next",               "next", "previous" or {"state": "DONE"}
      "options": {"logDone": "time"}   optional: server settings
    }

`reference(cases)` adds `expected`, the document after the command, to each
case. Emacs runs under faketime, so both programs see the same clock. Set
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
(require 'cl-lib)
(defun org-lsp-finish-notes ()
  "Store a pending log note the way a user would who writes no text."
  (when (memq 'org-add-log-note post-command-hook)
    (org-add-log-note)
    (when (get-buffer "*Org Note*")
      (with-current-buffer "*Org Note*" (org-ctrl-c-ctrl-c)))))
(defun org-lsp-todo-case (in out offset arg)
  ;; A case that stopped with an error may have left a note pending.
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil)
  (let ((result
         (condition-case err
             (with-temp-buffer
               (insert-file-contents in)
               (org-mode)
               (goto-char (1+ offset))
               ;; A long catch-up asks for confirmation; say yes.
               (cl-letf (((symbol-function 'y-or-n-p) (lambda (&rest _) t)))
                 (org-todo arg)
                 (org-lsp-finish-notes))
               (buffer-string))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))))
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


def bindings(options):
    """The Emacs variables that correspond to the server options."""
    symbol = lambda value: "'" + value if isinstance(value, str) else lisp_value(value)
    out = {
        "org-log-done": "nil",
        "org-log-repeat": "'time",
        "org-log-into-drawer": "nil",
        "org-tags-column": "-77",
        "org-todo-repeat-to-state": "nil",
        "org-todo-keywords": "'((sequence \"TODO\" \"DONE\"))",
    }
    for key, value in (options or {}).items():
        if key == "logDone":
            out["org-log-done"] = symbol(value)
        elif key == "logRepeat":
            out["org-log-repeat"] = symbol(value)
        elif key == "logIntoDrawer":
            out["org-log-into-drawer"] = lisp_value(value)
        elif key == "tagsColumn":
            out["org-tags-column"] = str(value)
        elif key == "todoRepeatToState":
            out["org-todo-repeat-to-state"] = lisp_value(value)
        elif key == "todoKeywords":
            out["org-todo-keywords"] = "'((sequence " + " ".join(lisp_string(word) for word in value) + "))"
        else:
            raise ValueError(f"no Emacs setting for option {key}")
    return " ".join(f"({name} {value})" for name, value in out.items())


def command_argument(command):
    if command == "next":
        return "'right"
    if command == "previous":
        return "'left"
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
                    f" {command_argument(case['command'])}))"
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
                    case["expected"] = handle.read()
    return cases


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
