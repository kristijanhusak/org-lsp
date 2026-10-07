"""Run Emacs's clock table commands over test cases and record the file
after them.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",        the pretended current time
      "text": "* a\\n#+BEGIN: clocktable¦\\n#+END:\\n",
                                          the Org file, `¦` where the
                                          cursor is (the start without one)
      "files": [["other.org", "* b\\n"]],  optional: more files beside it,
                                          which are the agenda files after
                                          `main.org` (but `…_archive` files)
      "steps": [["update"]],              `org-dblock-update` (`["update",
                                          "all"]` with `C-u`),
                                          `org-clock-report` (`["report"]`,
                                          `["report", "first"]` with `C-u`),
                                          `org-ctrl-c-ctrl-c` (`["ctrl-c"]`),
                                          `org-shiftright` and its relatives
                                          (`["shift", "right"]`, with a count
                                          after it), `["goto", "text"]`: the
                                          cursor to the start of that text
      "options": {"org_extend_today_until": 4}   optional
    }

`reference(cases)` adds `expected`: the file after the steps with `¦`
where the cursor is, or `ERROR: …`. The file is `main.org` in a directory
of its own; its name in the result is `/org`.
Emacs runs under faketime, so both programs see the same clock. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that version
instead of the one bundled with Emacs.
"""

import json
import os
import shutil
import subprocess

import todo_reference
from todo_reference import lisp_string, lisp_value

CURSOR = "¦"
# Where the files of the cases go. The results name it `/org`.
BASE = "/tmp/org-lsp-clocktable"

DRIVER = r"""
(require 'org-clock)
;; Archive files are Org files, as Org's autoloads make them.
(add-to-list 'auto-mode-alist '("\\.org_archive\\'" . org-mode))
(defun org-lsp-clocktable-case (file out offset steps)
  (let ((result
         (condition-case err
             (with-current-buffer (find-file-noselect file)
               ;; Shown in the selected window, as Org expects of the
               ;; buffer it updates a block in.
               (set-window-buffer nil (current-buffer))
               (goto-char (1+ offset))
               (dolist (step steps)
                 (pcase (car step)
                   ("goto" (goto-char (point-min))
                    (search-forward (cadr step))
                    (goto-char (match-beginning 0)))
                   ("update" (org-dblock-update (and (cadr step) '(4))))
                   ("report" (org-clock-report (and (cadr step) '(4))))
                   ("ctrl-c" (org-ctrl-c-ctrl-c))
                   ("shift"
                    (let ((count (and (nth 2 step) (string-to-number (nth 2 step)))))
                      (pcase (cadr step)
                        ("right" (org-shiftright count))
                        ("left" (org-shiftleft count))
                        ("up" (org-shiftup count))
                        ("down" (org-shiftdown count)))))))
               (prog1 (concat (buffer-substring (point-min) (point)) "¦"
                              (buffer-substring (point) (point-max)))
                 (set-buffer-modified-p nil)))
           (error (format "ERROR: %s" (error-message-string err))))))
    (dolist (buffer (buffer-list))
      (when (buffer-file-name buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))
    (with-temp-file out (insert result))))
"""

# The clock table options and their Emacs variables; the others go to
# `todo_reference.bindings`.
OPTIONS = {
    "org_extend_today_until": "org-extend-today-until",
    "org_clock_total_time_cell_format": "org-clock-total-time-cell-format",
    "org_clock_file_time_cell_format": "org-clock-file-time-cell-format",
}


def bindings(options, agenda):
    options = dict(options or {})
    own = {name: options.pop(key) for key, name in OPTIONS.items() if key in options}
    defaults = options.pop("org_clock_clocktable_default_properties", None)
    out = todo_reference.bindings(options)
    for name, value in own.items():
        out += f" ({name} {lisp_value(value)})"
    if defaults is not None:
        out += f" (org-clock-clocktable-default-properties (car (read-from-string {lisp_string('(' + defaults + ')')})))"
    out += " (org-agenda-files '(" + " ".join(lisp_string(path) for path in agenda) + "))"
    return out


def steps_form(steps):
    return "'(" + " ".join("(" + " ".join(lisp_string(part) for part in step) + ")" for step in steps) + ")"


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for index, case in enumerate(cases):
        by_clock.setdefault(case["clock"], []).append((index, case))
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    shutil.rmtree(BASE, ignore_errors=True)
    os.makedirs(BASE)
    try:
        for number, (clock, group) in enumerate(by_clock.items()):
            forms = [todo_reference.DRIVER, DRIVER]
            outputs = []
            for index, case in group:
                directory = os.path.join(BASE, str(index))
                os.makedirs(directory)
                text = case["text"]
                offset = text.find(CURSOR)
                text = text.replace(CURSOR, "", 1)
                path = os.path.join(directory, "main.org")
                agenda = [path]
                for name, other in case.get("files", []):
                    with open(os.path.join(directory, name), "w", encoding="utf-8") as handle:
                        handle.write(other)
                    if not name.endswith("_archive"):
                        agenda.append(os.path.join(directory, name))
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(text)
                target = os.path.join(directory, "out")
                outputs.append((case, target, directory))
                forms.append(
                    f"(let ({bindings(case.get('options'), agenda)})\n"
                    f"  (org-lsp-clocktable-case {lisp_string(path)} {lisp_string(target)} {max(offset, 0)} "
                    f"{steps_form(case['steps'])}))"
                )
            driver = os.path.join(BASE, f"driver-{number}.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write("\n".join(forms))
            subprocess.run(
                ["faketime", f"{clock}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                # File times as they are: Emacs asks about a file changed on disk
                # after it was read otherwise.
                env=dict(os.environ, TZ="UTC", NO_FAKE_STAT="1"),
            )
            for case, target, directory in outputs:
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = handle.read().replace(directory, "/org")
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
    PATH = os.path.join(ROOT, "tests", "clocktable", "cases.json")
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
