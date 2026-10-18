"""Run Emacs's clock resolution over test cases and record what it asks and
the file after it.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",        the pretended current time
      "text": "* A\\n:LOGBOOK:\\n…",       an Org file, with clocks left running
      "steps": [["resolve"], ["in", "C"]],
                                          `org-resolve-clocks` (`["resolve", "4"]`
                                          with `C-u`), `org-clock-in` on the
                                          headline with that text
      "answers": ["K", "", "i"],          what is typed at each question: a
                                          key, a number of minutes (empty for
                                          the default), a time
      "options": {"org_clock_auto_clock_resolution": true}   optional
    }

`reference(cases)` adds `expected`: the file after the steps, or `ERROR: …`;
`asked`: the prompts, in order; and `message`: the last thing said.
Emacs runs under faketime, so both programs see the same clock. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that version
instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

from todo_reference import lisp_string, lisp_value

DRIVER = r"""
(require 'org-clock)
(setq org-read-date-popup-calendar nil)
(defvar org-lsp-answers nil)
(defvar org-lsp-asked nil)
(defvar org-lsp-said nil)
(defun org-lsp-answer (prompt)
  (push prompt org-lsp-asked)
  (or (pop org-lsp-answers) (error "No answer for %s" prompt)))
;; Each question takes the next answer; what was asked is kept.
(advice-add 'read-char-exclusive :override
            (lambda (prompt &rest _) (string-to-char (org-lsp-answer prompt))))
(advice-add 'read-from-minibuffer :override (lambda (prompt &rest _) (org-lsp-answer prompt)))
(advice-add 'read-string :override (lambda (prompt &rest _) (org-lsp-answer prompt)))
(advice-add 'y-or-n-p :override (lambda (prompt) (equal (org-lsp-answer prompt) "y")))
;; The last message, leaving out the `Matched …' of a clock resumed.
(advice-add 'message :around
            (lambda (fn format &rest args)
              (let ((said (apply fn format args)))
                (when (and (stringp said) (not (string-empty-p said))
                           (not (string-prefix-p "Matched " said)))
                  (setq org-lsp-said said))
                said)))
(defun org-lsp-resolve-case (file out steps answers)
  (setq org-lsp-answers answers org-lsp-asked nil org-lsp-said nil)
  (when (org-clocking-p) (org-clock-cancel))
  (setq org-clock-leftover-time nil)
  (let* ((buffer (find-file-noselect file))
         (result
          ;; The buffer goes in any case: its clocks would be open in the
          ;; next case's `org-files-list'.
          (unwind-protect
              (condition-case err
                  (with-current-buffer buffer
                    (dolist (step steps)
                      (pcase (car step)
                        ("resolve" (org-resolve-clocks (and (cadr step) '(4))))
                        ("in" (goto-char (point-min))
                         (re-search-forward (concat "^\\*+ .*" (regexp-quote (cadr step))))
                         (org-clock-in))))
                    (buffer-string))
                (error (format "ERROR: %S" err)))
            (let ((said org-lsp-said))
              (when (org-clocking-p) (org-clock-cancel))
              (setq org-lsp-said said))
            (with-current-buffer buffer (set-buffer-modified-p nil))
            (kill-buffer buffer))))
    (with-temp-file out
      (insert result "\0" (mapconcat #'identity (reverse org-lsp-asked) "\n") "\0" (or org-lsp-said "")))))
"""

# The options and their Emacs variables.
OPTIONS = {
    "org_clock_auto_clock_resolution": "org-clock-auto-clock-resolution",
    "org_clock_resolve_expert": "org-clock-resolve-expert",
}


def bindings(options):
    out = "(org-clock-in-switch-to-state nil) (org-clock-out-switch-to-state nil)"
    for key, value in (options or {}).items():
        value = "'when-no-clock-is-running" if value == "when-no-clock-is-running" else lisp_value(value)
        out += f" ({OPTIONS[key]} {value})"
    return out


def steps_form(steps):
    return "'(" + " ".join("(" + " ".join(lisp_string(part) for part in step) + ")" for step in steps) + ")"


def reference(cases):
    """Fill in `expected`, `asked` and `message` for every case."""
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for number, (clock, group) in enumerate(by_clock.items()):
            forms = [DRIVER]
            outputs = []
            for index, case in enumerate(group):
                path = os.path.join(directory, f"case-{number}-{index}.org")
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(case["text"])
                target = path + ".out"
                outputs.append((case, target))
                answers = "'(" + " ".join(lisp_string(answer) for answer in case["answers"]) + ")"
                forms.append(
                    f"(let ({bindings(case.get('options'))})\n"
                    f"  (org-lsp-resolve-case {lisp_string(path)} {lisp_string(target)} "
                    f"{steps_form(case['steps'])} {answers}))"
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
                    text, asked, said = handle.read().split("\0")
                case["expected"] = text
                case["asked"] = asked.splitlines()
                case["message"] = said
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
    PATH = os.path.join(ROOT, "tests", "clock", "resolve.json")
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
