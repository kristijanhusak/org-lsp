"""Run Emacs's clock commands over test cases and record the file after them.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",        the pretended current time
      "text": "* TODO a\\n* TODO b\\n",    an Org file
      "steps": [["in", "a"], ["in", "b"], ["out"]],
                                          `org-clock-in` on the headline
                                          with that text, `org-clock-out`,
                                          `org-clock-cancel`
      "options": {"org_clock_in_switch_to_state": "NEXT"}   optional
    }

`reference(cases)` adds `expected`: the file after the steps, or `ERROR: …`.
Emacs runs under faketime, so both programs see the same clock. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that version
instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

import todo_reference
from todo_reference import lisp_string, lisp_value

DRIVER = r"""
(require 'org-clock)
(defun org-lsp-clock-case (file out steps)
  (remove-hook 'post-command-hook 'org-add-log-note)
  (setq org-log-setup nil)
  (when (org-clocking-p) (org-clock-cancel))
  (let ((result
         (condition-case err
             (with-current-buffer (find-file-noselect file)
               (dolist (step steps)
                 (pcase (car step)
                   ("in" (goto-char (point-min))
                    (re-search-forward (concat "^\\*+ .*" (regexp-quote (cadr step))))
                    (org-clock-in))
                   ("out" (org-clock-out))
                   ("cancel" (org-clock-cancel)))
                 (org-lsp-finish-notes))
               (prog1 (buffer-string)
                 (when (org-clocking-p) (org-clock-cancel))
                 (set-buffer-modified-p nil)
                 (kill-buffer)))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))))
"""

# The clock options and their Emacs variables; the others go to
# `todo_reference.bindings`.
CLOCK_OPTIONS = {
    "org_clock_in_switch_to_state": "org-clock-in-switch-to-state",
    "org_clock_out_switch_to_state": "org-clock-out-switch-to-state",
    "org_clock_into_drawer": "org-clock-into-drawer",
}


def bindings(options):
    options = dict(options or {})
    clock = {name: options.pop(key) for key, name in CLOCK_OPTIONS.items() if key in options}
    out = todo_reference.bindings(options)
    out += " (org-clock-in-switch-to-state nil) (org-clock-out-switch-to-state nil)"
    for name, value in clock.items():
        out += f" ({name} {lisp_value(value)})"
    return out


def steps_form(steps):
    return "'(" + " ".join("(" + " ".join(lisp_string(part) for part in step) + ")" for step in steps) + ")"


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for number, (clock, group) in enumerate(by_clock.items()):
            forms = [todo_reference.DRIVER, DRIVER]
            outputs = []
            for index, case in enumerate(group):
                path = os.path.join(directory, f"case-{number}-{index}.org")
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(case["text"])
                target = path + ".out"
                outputs.append((case, target))
                forms.append(
                    f"(let ({bindings(case.get('options'))})\n"
                    f"  (org-lsp-clock-case {lisp_string(path)} {lisp_string(target)} {steps_form(case['steps'])}))"
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
        json.dump(cases, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


if __name__ == "__main__":
    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "clock", "cases.json")
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
