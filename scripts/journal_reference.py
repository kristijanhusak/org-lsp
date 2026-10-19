"""Run org-journal's commands over test cases and record what they do.

A case is a JSON object:

    {
      "clock": "2026-10-10 14:05",            the pretended current time
      "files": {"20261009": "* Friday…\\n"},   the files of `org-journal-dir`
      "options": {                            optional: settings, as the
        "org_journal_file_type": "weekly",    server takes them
        "org_journal_carryover_items": "TODO=\\"TODO\\""
      },
      "command": "new-entry",                 new-entry, new-date-entry,
                                              new-scheduled-entry, reschedule,
                                              next, previous, open-current,
                                              agenda-files, search,
                                              search-forever, search-future,
                                              search-future-scheduled,
                                              schedule-view
      "prefix": 1,                            optional: how many C-u
      "from": "20261009", "point": 12,        optional: the file the
                                              command runs in, and where in
                                              it (characters from its start)
      "date": "2026-10-12 10:00",             optional: what is typed at the
                                              `Date:` prompt; a list for
                                              several prompts (the search's
                                              start and end)
      "search": "standup",                    optional: the string searched
      "answers": ["y"]                        optional: answers to the
                                              questions asked (y-or-n-p)
      "day": "2026-10-10"                     for the commands org-journal
                                              binds in `M-x calendar`
                                              (calendar-read, calendar-display,
                                              calendar-next, calendar-previous,
                                              calendar-new-date-entry,
                                              calendar-search-week, -month,
                                              -year): the day the calendar's
                                              cursor is on
    }

`reference(cases)` adds `expected`: `{"files", "buffer", "point",
"hidden", "messages"}`, and for `agenda-files` the files
`org-journal--update-org-agenda-files` gives the agenda (`agenda`), and for
the search and the schedule view the text of their buffer and its buttons,
each `[place, file, point]` (`view`), for `reschedule` the file the window
shows and its point (`window`): for the calendar's commands the day its cursor is on after them
(`calendar`) and the journal file another window shows, with its point and
hidden lines (`shown`): the files of the directory after the command
(each buffer saved), the file the command left the cursor in and where
(characters from its start), the lines of that file its headlines' folds
left hidden (1-based; drawers fold on their own), and the messages it
showed, with the directory written `/journal/`; or `ERROR: …` when it
refuses.

Emacs runs under faketime, in the C locale. org-journal is not part of
Emacs: set ORG_LSP_EMACS_PACKAGES to a directory with a checkout of it
(`org-journal`, 2.2.0). Set ORG_LISP_DIR to the `lisp` directory of an Org
checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

from todo_reference import lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'org-journal)
(require 'cl-lib)
(setq org-read-date-popup-calendar nil)
;; Not `visual-line-mode', nor encrypting when saving.
(setq org-journal-mode-hook nil)
;; Under a faked clock files seem changed on disk; nobody is there to ask.
(load "userlock" nil t)
(defun ask-user-about-supersession-threat (&rest _) nil)
(defun userlock--ask-user-about-supersession-threat (&rest _) nil)
(defvar org-lsp-messages nil)
(defvar org-lsp-answers nil)
(defvar org-lsp-date nil)
(defvar org-lsp-agenda nil)
;; The agenda files org-journal sets are kept, not saved.
(advice-add 'org-store-new-agenda-file-list :override (lambda (list) (setq org-lsp-agenda list)))
(advice-add 'message :around
            (lambda (orig format &rest args)
              (when (and format (not inhibit-message) (not (equal format "")))
                (push (apply #'format-message format args) org-lsp-messages))
              (apply orig format args)))
(defun org-lsp-ask (prompt &rest _)
  (let ((answer (pop org-lsp-answers)))
    (unless answer (error "No answer to: %s" prompt))
    (member answer '("y" "yes"))))
(advice-add 'y-or-n-p :override #'org-lsp-ask)
(advice-add 'yes-or-no-p :override #'org-lsp-ask)
;; The `Date:' prompt reads what the case types.
(advice-add 'org-read-date :filter-args
            (lambda (args)
              (list (nth 0 args) (nth 1 args)
                    (or (if (consp org-lsp-date) (pop org-lsp-date) org-lsp-date) (error "No date"))
                    (nth 3 args) (nth 4 args) (nth 5 args) (nth 6 args))))
(defun org-lsp-hidden ()
  (save-excursion
    (goto-char (point-min))
    (let ((line 1) hidden)
      (while (not (eobp))
        (when (org-fold-folded-p (point) 'headline) (push line hidden))
        (forward-line 1)
        (setq line (1+ line)))
      (vconcat (nreverse hidden)))))
(defun org-lsp-view (name)
  "The text of the buffer NAME, and where its buttons lead."
  (when (get-buffer name)
    (with-current-buffer name
      (let (links)
        (let ((button (next-button (point-min))))
          (while button
            (let ((target (button-get button 'org-journal-link)))
              (push (vector (1- (button-start button))
                            (if (cdr target)
                                (file-relative-name (org-journal--get-entry-path (cdr target)) org-journal-dir)
                              :null)
                            (1- (car target)))
                    links))
            (setq button (next-button (button-end button)))))
        (list (cons 'text (buffer-substring-no-properties (point-min) (point-max)))
              (cons 'links (vconcat (nreverse links))))))))
(defun org-lsp-calendar-day ()
  "The day the calendar's cursor is on, as YYYY-MM-DD."
  (when (get-buffer calendar-buffer)
    (with-current-buffer calendar-buffer
      (let ((date (calendar-cursor-to-date)))
        (and date (format "%04d-%02d-%02d" (nth 2 date) (nth 0 date) (nth 1 date)))))))
(defun org-lsp-shown ()
  "The journal file a window other than the calendar's shows: its name,
the window's point and the lines hidden in it."
  (let ((window (seq-find (lambda (window) (buffer-file-name (window-buffer window)))
                          (window-list))))
    (if window
        (with-current-buffer (window-buffer window)
          (vector (file-relative-name (buffer-file-name) org-journal-dir)
                  (1- (window-point window))
                  (save-excursion (goto-char (window-point window)) (org-lsp-hidden))))
      :null)))
(defun org-lsp-journal-case (dir out command prefix from point date answers search &optional day)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (dolist (name '("*Org-journal search*" "*Org-journal schedule*"))
    (when (get-buffer name) (kill-buffer name)))
  (setq org-lsp-messages nil org-lsp-answers answers org-lsp-date date org-lsp-agenda nil
        kill-ring nil kill-ring-yank-pointer nil)
  (switch-to-buffer "*scratch*")
  (delete-other-windows)
  (let* ((org-journal-dir (file-name-as-directory dir))
         (result
          (condition-case err
              (progn
                (clrhash org-journal--dates)
                (setq org-journal--sorted-dates nil)
                (when from
                  (find-file (expand-file-name from org-journal-dir))
                  (goto-char (1+ point)))
                (when (get-buffer calendar-buffer) (kill-buffer calendar-buffer))
                ;; The calendar, its cursor on DAY, alone in the frame as
                ;; `calendar' shows it in a window below another.
                (when day
                  (calendar)
                  (let ((parts (mapcar #'string-to-number (split-string day "-"))))
                    (calendar-goto-date (list (nth 1 parts) (nth 2 parts) (nth 0 parts)))))
                (let ((current-prefix-arg (and (> prefix 0) (list (expt 4 prefix))))
                      (arg (and (> prefix 0) (list (expt 4 prefix)))))
                  (pcase command
                    ("new-entry" (org-journal-new-entry arg))
                    ("new-date-entry" (org-journal-new-date-entry arg))
                    ("new-scheduled-entry" (org-journal-new-scheduled-entry arg))
                    ("reschedule" (org-journal-reschedule-scheduled-entry))
                    ("next" (org-journal-next-entry))
                    ("previous" (org-journal-previous-entry))
                    ("open-current" (org-journal-open-current-journal-file))
                    ("search" (org-journal-search search))
                    ("search-forever" (org-journal-search-forever search))
                    ("search-future" (org-journal-search-future search))
                    ("search-future-scheduled" (org-journal-search-future-scheduled))
                    ("schedule-view" (org-journal-schedule-view))
                    ("calendar-read" (org-journal-read-entry arg))
                    ("calendar-display" (org-journal-display-entry arg))
                    ("calendar-next" (org-journal-next-entry))
                    ("calendar-previous" (org-journal-previous-entry))
                    ("calendar-new-date-entry" (org-journal-new-date-entry arg))
                    ("calendar-search-week" (org-journal-search-calendar-week search))
                    ("calendar-search-month" (org-journal-search-calendar-month search))
                    ("calendar-search-year" (org-journal-search-calendar-year search))
                    ("agenda-files"
                     (let ((org-journal-enable-agenda-integration t) (org-agenda-files nil))
                       (org-journal--update-org-agenda-files)))))
                (let* ((file (buffer-file-name))
                       (name (and file (file-relative-name file org-journal-dir)))
                       (where (1- (point)))
                       (hidden (if file (org-lsp-hidden) [])))
                  (save-some-buffers t)
                  (append
                   (list (cons 'buffer (or name :null))
                         (cons 'point (if file where :null))
                         (cons 'hidden hidden))
                   (when (or (string-prefix-p "search" command) (string-prefix-p "calendar-search" command))
                     (list (cons 'view (org-lsp-view "*Org-journal search*"))))
                   (when day
                     (list (cons 'calendar (or (org-lsp-calendar-day) :null))
                           (cons 'shown (org-lsp-shown))))
                   (when (equal command "reschedule")
                     (let ((window (window-buffer (selected-window))))
                       (list (cons 'window
                                   (vector (if (buffer-file-name window)
                                               (file-relative-name (buffer-file-name window) org-journal-dir)
                                             :null)
                                           (1- (window-point (selected-window))))))))
                   (when (equal command "schedule-view")
                     (list (cons 'view (org-lsp-view "*Org-journal schedule*"))))
                   (when (equal command "agenda-files")
                     (list (cons 'agenda (vconcat (mapcar (lambda (file) (file-relative-name file org-journal-dir))
                                                          org-lsp-agenda))))))))
            (error (format "ERROR: %s" (error-message-string err))))))
    (with-temp-file out
      (set-buffer-file-coding-system 'utf-8)
      (if (stringp result)
          (insert result)
        (let ((files (make-hash-table :test 'equal)))
          (dolist (file (directory-files-recursively org-journal-dir ""))
            (puthash (file-relative-name file org-journal-dir)
                     (with-temp-buffer (insert-file-contents file) (buffer-string))
                     files))
          (insert (json-serialize
                   (append result
                           (list (cons 'files files)
                                 (cons 'messages (vconcat (reverse org-lsp-messages))))))))))))
"""

# The options of a case as Emacs's variables.
STRING = lisp_string
OPTIONS = {
    "org_journal_file_type": lambda v: "'" + v,
    "org_journal_start_on_weekday": str,
    "org_journal_file_format": STRING,
    "org_journal_date_format": STRING,
    "org_journal_date_prefix": STRING,
    "org_journal_time_format": STRING,
    "org_journal_time_format_post_midnight": STRING,
    "org_journal_time_prefix": STRING,
    "org_journal_hide_entries_p": lambda v: "t" if v else "nil",
    "org_journal_carryover_items": STRING,
    "org_journal_skip_carryover_drawers": lambda v: "(list " + " ".join(STRING(x) for x in v) + ")",
    "org_journal_carryover_delete_empty_journal": lambda v: "'" + v,
    "org_journal_file_header": STRING,
    "org_journal_created_property_timestamp_format": STRING,
    "org_journal_scheduled_string": STRING,
    "org_journal_enable_encryption": lambda v: "t" if v else "nil",
    "org_extend_today_until": str,
    "org_tags_column": str,
    "org_journal_search_result_date_format": STRING,
    "org_journal_search_results_order_by": lambda v: ":" + v,
    "org_journal_follow_mode": lambda v: "t" if v else "nil",
    "calendar_week_start_day": str,
    "org_todo_keywords": lambda v: "'((sequence " + " ".join(STRING(x) for x in v) + "))",
}


def bindings(options):
    out = {"org-journal-carryover-items": STRING("TODO=\"TODO\"")}
    for name, value in (options or {}).items():
        if name not in OPTIONS:
            raise ValueError(f"no Emacs setting for option {name}")
        out[name.replace("_", "-")] = OPTIONS[name](value)
    return " ".join(f"({name} {value})" for name, value in out.items())


def reference(cases):
    """Fill in `expected` for every case."""
    packages = os.environ.get("ORG_LSP_EMACS_PACKAGES")
    if not packages:
        raise SystemExit("set ORG_LSP_EMACS_PACKAGES to the directory of org-journal")
    load_path = []
    if os.environ.get("ORG_LISP_DIR"):
        load_path += ["-L", os.environ["ORG_LISP_DIR"]]
    load_path += ["-L", os.path.join(packages, "org-journal")]
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    with tempfile.TemporaryDirectory() as directory:
        for number, (clock, group) in enumerate(by_clock.items()):
            forms = [DRIVER]
            outputs = []
            for index, case in enumerate(group):
                folder = os.path.join(directory, f"{number}-{index}", "journal")
                os.makedirs(folder)
                for name, text in sorted(case["files"].items()):
                    path = os.path.join(folder, name)
                    os.makedirs(os.path.dirname(path), exist_ok=True)
                    with open(path, "w", encoding="utf-8") as handle:
                        handle.write(text)
                target = os.path.join(directory, f"{number}-{index}", "out.json")
                outputs.append((case, target))
                answers = " ".join(lisp_string(answer) for answer in case.get("answers", []))
                if isinstance(case.get("date"), list):
                    date = "(list " + " ".join(lisp_string(d) for d in case["date"]) + ")"
                else:
                    date = lisp_string(case["date"]) if "date" in case else "nil"
                source = lisp_string(case["from"]) if "from" in case else "nil"
                forms.append(
                    f"(let ({bindings(case.get('options'))})"
                    f" (org-lsp-journal-case {lisp_string(folder)} {lisp_string(target)}"
                    f" {lisp_string(case['command'])} {case.get('prefix', 0)} {source} {case.get('point', 0)}"
                    f" {date} (list {answers}) {lisp_string(case.get('search', ''))}"
                    f" {lisp_string(case['day']) if 'day' in case else 'nil'}))"
                )
            driver = os.path.join(directory, f"driver-{number}.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write("\n".join(forms))
            subprocess.run(
                ["faketime", f"{clock}:00", "emacs", "--batch", "-Q", *load_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=None if os.environ.get("ORG_LSP_DEBUG") else subprocess.DEVNULL,
                env=dict(os.environ, TZ="UTC", LC_ALL="C", LANG="C", NO_FAKE_STAT="1"),
                timeout=1200,
            )
            for case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    text = handle.read()
                # The directory is `/journal/` in messages.
                text = text.replace(os.path.join(os.path.dirname(target), "journal") + "/", "/journal/")
                case["expected"] = text if text.startswith("ERROR:") else json.loads(text)
    return cases
