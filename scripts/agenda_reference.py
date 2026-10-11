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
        {"view": "search", "match": "+work -home"},   `org-search-view`
        {"view": "search-todo", "match": "report"},     with only TODO entries
        {"view": "custom", "key": "c"},
        {"view": "stuck"},                       `org-agenda-list-stuck-projects`
        {"view": "todo", "filters": [...]}       with the agenda's filters (see
                                                `filter_form`)
        {"view": "todo", "entry_text": 5}       with entry text mode (`E`), at
                                                most 5 lines an entry
        {"view": "todo", "archives": "trees"}   with archived trees
                                                (`files`: archive files too)
        {"view": "agenda", "inactive": true}    with inactive timestamps
                                     (`org-agenda-include-inactive-timestamps`)
      ],
      "options": {...},               optional: settings for org-lsp
      "emacs": "(setq ...)"           optional: the same settings for Emacs
                                      (in both, `{dir}` is the case's
                                      directory)
    }

Options of org-super-agenda (`org_super_agenda_mode`, `…_groups` and the
others) are given to Emacs as well, from `options`: set
ORG_LSP_EMACS_PACKAGES to a directory with checkouts of org-super-agenda
and what it needs (compat, dash, s, ht, ts).

`reference(cases)` writes `expected-N.txt` next to each `case.json`, one
per run. Emacs runs under faketime, so both programs see the same clock
(starting at it: a frozen clock makes Emacs wait forever where it pauses).
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
(let ((packages (getenv "ORG_LSP_EMACS_PACKAGES")))
  (when packages
    (dolist (dir (directory-files packages t "^[^.]"))
      (when (file-directory-p dir) (add-to-list 'load-path dir)))
    (require 'org-super-agenda)
    (setq org-super-agenda-show-message nil)))
(setq org-agenda-tags-column -80)
(defun org-lsp-run (out fn)
  (let ((text (condition-case err
                  (with-output-to-string (funcall fn))
                (error (format "ERROR: %S\n" err)))))
    (with-temp-file out (insert text))))
(defun org-lsp-visible-string ()
  "The agenda buffer without the lines its filters hide, with the entry
text `E' shows after the lines."
  (let ((inhibit-read-only t))
    (dolist (o (overlays-in (point-min) (point-max)))
      ;; Not after a line the filters hide. (Emacs's display shows the
      ;; text of a hidden line when the line after it is not hidden,
      ;; under the visible line above; org-lsp leaves that out.)
      (when (and (eq (overlay-get o 'org-overlay-type) 'agenda-entry-content)
                 (not (eq (get-text-property (overlay-start o) 'invisible) 'org-filtered)))
        (save-excursion
          (goto-char (overlay-end o))
          (insert (overlay-get o 'after-string))))))
  (let ((pos (point-min)) (out ""))
    (while (< pos (point-max))
      (let ((next (next-single-property-change pos 'invisible nil (point-max))))
        (unless (eq (get-text-property pos 'invisible) 'org-filtered)
          (setq out (concat out (buffer-substring-no-properties pos next))))
        (setq pos next)))
    out))
(defun org-lsp-reset ()
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer) (kill-buffer buffer))))
"""

# Everything a case may change, back to its default before the next one.
DEFAULTS = r"""
(setq org-agenda-entry-text-leaders "    > "
      org-agenda-entry-text-exclude-regexps nil
      org-agenda-entry-text-maxlines 5)
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
      org-agenda-search-view-always-boolean nil
      org-agenda-search-view-force-full-words nil
      org-agenda-search-view-max-outline-level 0
      org-agenda-custom-commands nil
      org-agenda-log-mode-items '(closed clock)
      org-tag-alist nil
      org-group-tags t
      org-agenda-text-search-extra-files nil
      org-stuck-projects '("+LEVEL=2/-DONE" ("TODO" "NEXT" "NEXTACTION") nil "")
      org-agenda-prefix-format '((agenda . " %i %-12:c%?-12t% s") (todo . " %i %-12:c")
                                 (tags . " %i %-12:c") (search . " %i %-12:c"))
      org-agenda-todo-keyword-format "%-1s"
      org-agenda-scheduled-leaders '("Scheduled: " "Sched.%2dx: ")
      org-agenda-deadline-leaders '("Deadline:  " "In %3d d.: " "%2d d. ago: ")
      org-agenda-timerange-leaders '("" "(%d/%d): ")
      org-agenda-remove-tags nil
      org-agenda-breadcrumbs-separator "->"
      org-agenda-format-date 'org-agenda-format-date-aligned
      org-use-tag-inheritance t
      org-tags-exclude-from-inheritance nil
      org-use-property-inheritance nil
      org-habit-show-habits t
      org-habit-show-habits-only-for-today t
      org-habit-show-all-today nil
      org-habit-graph-column 40
      org-habit-preceding-days 21
      org-habit-following-days 7
      org-habit-today-glyph ?!
      org-habit-completed-glyph ?*
      org-habit-show-done-always-green nil
      org-habit-scheduled-past-days nil
      org-agenda-time-grid '((daily today require-timed)
                             (800 1000 1200 1400 1600 1800 2000)
                             "......" "----------------"))
(when (featurep 'org-super-agenda)
  (org-super-agenda-mode -1)
  (setq org-super-agenda-groups nil
        org-super-agenda-unmatched-name "Other items"
        org-super-agenda-unmatched-order 99
        org-super-agenda-header-separator "\n"
        org-super-agenda-header-prefix " "
        org-super-agenda-final-group-separator ""
        org-super-agenda-date-format "%e %B %Y"
        org-super-agenda-hide-empty-groups nil
        org-super-agenda-keep-order nil
        org-super-agenda-group-property-name "agenda-group"
        org-super-agenda-properties-inherit t))
"""

# The arguments of these selectors that are symbols in Emacs, not strings.
SUPER_SYMBOLS = {
    "date": {"today"},
    "deadline": {"past", "today", "future", "before", "on", "after"},
    "scheduled": {"past", "today", "future", "before", "on", "after"},
    "children": {"todo"},
    "log": {"close", "closed", "clock", "clocked", "changed", "state"},
    "auto-ts": {"reverse"},
}


def super_value(key, value):
    """A selector's argument as Emacs Lisp: `true` is `t`, `false` is `nil`,
    and the words a selector takes as symbols are symbols."""
    if value is True:
        return "t"
    if value is False or value is None:
        return "nil"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return value if value in SUPER_SYMBOLS.get(key, ()) else lisp_string(value)
    if isinstance(value, dict):
        return "(" + " ".join(f":{name} {super_value(name, item)}" for name, item in value.items()) + ")"
    return "(" + " ".join(super_value(key, item) for item in value) + ")"


def super_group(group):
    """A group of `org-super-agenda-groups` as Emacs Lisp: an object of
    selectors, or a list of selectors and their arguments in turn."""
    pairs = list(group.items()) if isinstance(group, dict) else list(zip(group[::2], group[1::2]))
    out = []
    for key, value in pairs:
        key = key.lstrip(":")
        if key == "name" and value is False:
            text = "none"
        elif key in ("and", "not", "discard"):
            text = super_group(value)
        elif key == "take":
            text = f"({value[0]} {super_group(value[1])})"
        elif key == "order-multi":
            text = "(" + " ".join([str(value[0])] + [super_group(item) for item in value[1:]]) + ")"
        else:
            text = super_value(key, value)
        out.append(f":{key} {text}")
    return "(" + " ".join(out) + ")"


def super_settings(options):
    """The org-super-agenda options of a case's `options` as Emacs Lisp."""
    forms = []
    for name, value in options.items():
        if not name.startswith("org_super_agenda_") or name == "org_super_agenda_mode":
            continue
        variable = name.replace("_", "-")
        if name == "org_super_agenda_groups":
            text = "'(" + " ".join(super_group(group) for group in value) + ")"
        elif name.endswith("_separator") and isinstance(value, str) and len(value) == 1:
            # One character is repeated across the window, as `?-` is.
            text = "?" + value
        elif isinstance(value, str):
            text = lisp_string(value)
        else:
            text = super_value("", value)
        forms.append(f"(setq {variable} {text})")
    if options.get("org_super_agenda_mode"):
        forms.append("(org-super-agenda-mode 1)")
    return "\n".join(forms)


def org_files(directory):
    return sorted(name for name in os.listdir(directory) if name.endswith(".org"))


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def shown(form):
    """A view command, and the agenda buffer printed as `org-batch-agenda` does."""
    return f"(progn {form} (set-buffer org-agenda-buffer-name) (princ (buffer-string)))"


def filter_form(step):
    """One step of the agenda's filters, as its commands take it."""
    if step.get("remove"):
        return "(org-agenda-filter-remove-all)"
    if "string" in step:
        negate = "'(4)" if step.get("negate") else "nil"
        return (f"(cl-letf (((symbol-function 'completing-read) (lambda (&rest _) {lisp_string(step['string'])})))"
                f" (org-agenda-filter {negate}))")
    if "top" in step:
        negative = "t" if step.get("negative") else "nil"
        return (f"(progn (org-agenda-filter-show-all-top-filter)"
                f" (org-agenda-filter-top-headline-apply {lisp_string(step['top'])} {negative}))")
    kind = step["type"]
    values = " ".join(lisp_string(value) for value in step["filter"])
    expand = "nil" if step.get("expand") is False else "t"
    return (f"(progn (org-agenda-remove-filter '{kind})"
            f" (org-agenda-filter-apply (setq org-agenda-{kind}-filter (list {values})) '{kind} {expand}))")


def run_form(run):
    form = view_form(run)
    if "filters" in run or "entry_text" in run:
        steps = " ".join(filter_form(step) for step in run.get("filters", []))
        if "entry_text" in run:
            steps += f" (org-agenda-entry-text-mode {run['entry_text']})"
        form = (f"(progn (let ((standard-output #'ignore)) {form}) (set-buffer org-agenda-buffer-name)"
                f" {steps} (princ (org-lsp-visible-string)))")
    if run.get("inactive"):
        form = f"(let ((org-agenda-include-inactive-timestamps t)) {form})"
    archives = {"trees": "'trees", "files": "t"}.get(run.get("archives"))
    return f"(let ((org-agenda-start-with-archives-mode {archives})) {form})" if archives else form


def view_form(run):
    match = lisp_string(run.get("match", ""))
    if run["view"] == "todo":
        return shown(f"(org-todo-list {match})") if "match" in run else '(org-batch-agenda "t")'
    if run["view"] == "tags":
        return shown(f"(org-tags-view nil {match})")
    if run["view"] == "tags-todo":
        return shown(f"(org-tags-view t {match})")
    if run["view"] == "search":
        return shown(f"(org-search-view nil {match})")
    if run["view"] == "search-todo":
        return shown(f"(org-search-view t {match})")
    if run["view"] == "stuck":
        return shown("(org-agenda-list-stuck-projects)")
    if run["view"] == "custom":
        return f"(org-batch-agenda {lisp_string(run['key'])})"
    parameters = []
    span = run.get("span", "week")
    parameters.append("org-agenda-span " + (str(span) if isinstance(span, int) else "'" + span))
    if "start" in run:
        parameters.append("org-agenda-start-day " + lisp_string(run["start"]))
    if run.get("log"):
        parameters.append("org-agenda-start-with-log-mode t")
    if run.get("clockreport"):
        parameters.append("org-agenda-start-with-clockreport-mode t")
    return '(org-batch-agenda "a" ' + " ".join(parameters) + ")"


def clean(text, run):
    """Drop what a server cannot or should not reproduce."""
    lines = text.split("\n")
    # A hint about Emacs key bindings, and its continuation lines.
    kept = []
    skipping = False
    for line in lines:
        if line.startswith("Press ") and ("to search again" in line or "to add/sub word" in line):
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
            # `{dir}` in the settings is the case's directory.
            forms.append(super_settings(case.get("options", {})))
            forms.append(case.get("emacs", "").replace("{dir}", directory))
            # The group tags of the files, which Emacs reads when it builds
            # an agenda and a match string uses from the one built before.
            forms.append("(org-agenda-prepare-buffers (org-agenda-files nil 'ifmode))")
            for index, run in enumerate(case["runs"]):
                out = os.path.join(directory, f"expected-{index}.txt")
                outputs.append((out, run))
                forms.append(f"(org-lsp-run {lisp_string(out)} (lambda () {run_form(run)}))")
        with tempfile.NamedTemporaryFile("w", suffix=".el", delete=False, encoding="utf-8") as driver:
            driver.write("\n".join(forms))
        try:
            subprocess.run(
                ["faketime", "-f", f"@{today} {now}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver.name],
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
            # The clock report links to the files: by where the case is.
            text = text.replace(os.path.dirname(out) + "/", "{dir}/")
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
    for flag in ("log", "clockreport", "inactive"):
        if run.get(flag):
            arguments.append("--" + flag)
    if "archives" in run:
        arguments += ["--archives", run["archives"]]
    if "entry_text" in run:
        arguments += ["--entry-text", str(run["entry_text"])]
    if "filters" in run:
        arguments += ["--filters", json.dumps(run["filters"])]
    if "options" in case:
        arguments += ["--options", json.dumps(case["options"]).replace("{dir}", directory)]
    return arguments + [os.path.join(directory, name) for name in org_files(directory)]
