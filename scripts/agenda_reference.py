"""Run Emacs's agenda over test cases and store what it prints.

A case is a directory with Org files and a `case.json`:

    {
      "today": "2026-10-05",          the pretended current date
      "now": "10:30",                 and time
      "runs": [                       agenda commands to run
        {"view": "agenda", "span": "week"},
        {"view": "agenda", "span": "day", "start": "2026-10-07"},
        {"view": "todo"},
        {"view": "todo", "match": "NEXT|WAIT"},
        {"view": "tags", "match": "work-home"},
        {"view": "tags-todo", "match": "PRIORITY=\"A\""},
        {"view": "custom", "key": "c"}
      ],
      "options": {...},               optional: settings for org-lsp
      "emacs": "(setq ...)"           optional: the same settings for Emacs
    }

`reference(cases)` writes `expected-N.txt` next to each `case.json`, one
per run. Emacs runs under faketime, so both programs see the same clock.
Set ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

PRELUDE = r"""
(require 'org)
(require 'org-agenda)
(setq org-agenda-tags-column -80)
(defun org-lsp-run (out fn)
  (let ((text (condition-case err
                  (with-output-to-string (funcall fn))
                (error (format "ERROR: %S\n" err)))))
    (with-temp-file out (insert text))))
(defun org-lsp-reset ()
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer) (kill-buffer buffer))))
"""

# Everything a case may change, back to its default before the next one.
DEFAULTS = r"""
(setq org-agenda-skip-scheduled-if-done nil
      org-agenda-skip-deadline-if-done nil
      org-agenda-skip-timestamp-if-done nil
      org-agenda-show-future-repeats t
      org-agenda-show-all-dates t
      org-agenda-start-on-weekday 1
      org-agenda-use-time-grid t
      org-agenda-show-current-time-in-grid t
      org-deadline-warning-days 14
      org-scheduled-delay-days 0
      org-scheduled-past-days 10000
      org-deadline-past-days 10000
      org-agenda-tags-column -80
      org-agenda-custom-commands nil
      org-agenda-time-grid '((daily today require-timed)
                             (800 1000 1200 1400 1600 1800 2000)
                             "......" "----------------"))
"""


def org_files(directory):
    return sorted(name for name in os.listdir(directory) if name.endswith(".org"))


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def shown(form):
    """A view command, and the agenda buffer printed as `org-batch-agenda` does."""
    return f"(progn {form} (set-buffer org-agenda-buffer-name) (princ (buffer-string)))"


def run_form(run):
    match = lisp_string(run.get("match", ""))
    if run["view"] == "todo":
        return shown(f"(org-todo-list {match})") if "match" in run else '(org-batch-agenda "t")'
    if run["view"] == "tags":
        return shown(f"(org-tags-view nil {match})")
    if run["view"] == "tags-todo":
        return shown(f"(org-tags-view t {match})")
    if run["view"] == "custom":
        return f"(org-batch-agenda {lisp_string(run['key'])})"
    parameters = []
    span = run.get("span", "week")
    parameters.append("org-agenda-span " + (str(span) if isinstance(span, int) else "'" + span))
    if "start" in run:
        parameters.append("org-agenda-start-day " + lisp_string(run["start"]))
    return '(org-batch-agenda "a" ' + " ".join(parameters) + ")"


def clean(text, run):
    """Drop what a server cannot or should not reproduce."""
    lines = text.split("\n")
    # A hint about Emacs key bindings, and its continuation lines.
    kept = []
    skipping = False
    for line in lines:
        if line.startswith("Press ") and "to search again" in line:
            skipping = True
            continue
        if skipping and line.startswith("                     "):
            continue
        skipping = False
        kept.append(line)
    lines = kept
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines) + "\n"


def reference(directories):
    """Write expected-N.txt for every run of every case directory."""
    by_clock = {}
    for directory in directories:
        with open(os.path.join(directory, "case.json"), encoding="utf-8") as handle:
            case = json.load(handle)
        by_clock.setdefault((case["today"], case["now"]), []).append((directory, case))

    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    for (today, now), cases in by_clock.items():
        forms = [PRELUDE]
        outputs = []
        for directory, case in cases:
            files = " ".join(lisp_string(os.path.join(directory, name)) for name in org_files(directory))
            forms.append(DEFAULTS)
            forms.append("(org-lsp-reset)")
            forms.append(f"(setq org-agenda-files (list {files}))")
            forms.append(case.get("emacs", ""))
            for index, run in enumerate(case["runs"]):
                out = os.path.join(directory, f"expected-{index}.txt")
                outputs.append((out, run))
                forms.append(f"(org-lsp-run {lisp_string(out)} (lambda () {run_form(run)}))")
        with tempfile.NamedTemporaryFile("w", suffix=".el", delete=False, encoding="utf-8") as driver:
            driver.write("\n".join(forms))
        try:
            subprocess.run(
                ["faketime", f"{today} {now}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver.name],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=dict(os.environ, TZ="UTC"),
            )
        finally:
            os.unlink(driver.name)
        for out, run in outputs:
            with open(out, encoding="utf-8") as handle:
                text = handle.read()
            with open(out, "w", encoding="utf-8") as handle:
                handle.write(clean(text, run))


def run_arguments(directory, case, run):
    """The `org-lsp agenda` arguments that correspond to a run."""
    arguments = ["agenda", "--today", case["today"], "--now", case["now"], "--view", run["view"]]
    if run["view"] == "agenda":
        arguments += ["--span", str(run.get("span", "week"))]
        if "start" in run:
            arguments += ["--start", run["start"]]
    if "match" in run:
        arguments += ["--match", run["match"]]
    if "key" in run:
        arguments += ["--key", run["key"]]
    if "options" in case:
        arguments += ["--options", json.dumps(case["options"])]
    return arguments + [os.path.join(directory, name) for name in org_files(directory)]
