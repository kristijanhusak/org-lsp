"""Run Emacs's `org-capture` over test cases and record what it does.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",            the pretended current time
      "files": {"notes.org": "* Tasks\\n"},    Org files
      "from": "notes.org",                    optional: the file captured from
      "template": {                           a capture template, as the
        "type": "entry",                      server's `captureTemplates` has
        "target": ["file+headline", "notes.org", "Tasks"],
        "template": "* TODO %?\\n%U",
        "prepend": true, "empty_lines": 1, "empty_lines_before": 0,
        "empty_lines_after": 0, "immediate_finish": false
      },
      "answers": ["fri"],                     what is typed at the prompts
      "clock_in": ["notes.org", "Tasks"],     optional: a clock started on
                                              that headline first
      "typed": "Call Bob",                    what is typed in the capture
      "options": {"org_adapt_indentation": true}   optional: server settings
    }                                         buffer, where the cursor is

`reference(cases)` adds `expected` to each case: `{"buffer": ...,
"files": {...}}`, the capture buffer before typing, with ¦ for the cursor,
and the files after `org-capture-finalize`; or `ERROR: …` when Emacs
refuses. Emacs runs under faketime. Set ORG_LISP_DIR to the `lisp`
directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import os
import subprocess
import tempfile

from todo_reference import CURSOR, lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'org-capture)
(require 'org-clock)
(require 'cl-lib)
(setq org-read-date-popup-calendar nil)
;; Under a faked clock files seem changed on disk; nobody is there to ask.
(load "userlock" nil t)
(defun ask-user-about-supersession-threat (&rest _) nil)
(defun userlock--ask-user-about-supersession-threat (&rest _) nil)
(defun org-lsp-answer (answer default)
  (if (and (or (null answer) (equal answer "")) default)
      (if (consp default) (car default) default)
    (or answer "")))
(defun org-lsp-capture-case (files from template answers typed out &optional clock)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (let* ((shown nil)
         (result
          (condition-case err
              (let ((org-capture-templates (list template))
                    (org-agenda-files files))
                ;; An empty answer takes the default, as at Emacs's prompts.
                (cl-letf (((symbol-function 'read-string)
                           (lambda (_prompt &optional _initial _history default &rest _)
                             (org-lsp-answer (pop answers) default)))
                          ((symbol-function 'read-from-minibuffer)
                           (lambda (&rest _) (or (pop answers) "")))
                          ((symbol-function 'completing-read)
                           (lambda (_prompt _collection &optional _predicate _require _initial _history default &rest _)
                             (org-lsp-answer (pop answers) default)))
                          ((symbol-function 'completing-read-multiple)
                           (lambda (&rest _) (split-string (or (pop answers) "") "[ \t]*:[ \t]*" t)))
                          ((symbol-function 'y-or-n-p) (lambda (&rest _) t))
                          ((symbol-function 'yes-or-no-p) (lambda (&rest _) t)))
                  (when clock
                    (with-current-buffer (find-file-noselect (car clock))
                      (goto-char (point-min))
                      (re-search-forward (concat "^\\*+ .*" (regexp-quote (cadr clock))))
                      (org-clock-in)))
                  (set-buffer (if from (find-file-noselect from) (get-buffer-create "*scratch*")))
                  (org-capture nil "t")
                  (let ((capture (seq-find (lambda (buffer) (string-prefix-p "CAPTURE-" (buffer-name buffer)))
                                           (buffer-list))))
                    (when capture (set-buffer capture)))
                  (when (and (boundp 'org-capture-mode) org-capture-mode)
                    (setq shown (concat (buffer-substring-no-properties (point-min) (point))
                                        "¦"
                                        (buffer-substring-no-properties (point) (point-max))))
                    (insert typed)
                    (org-capture-finalize)))
                (mapconcat
                 (lambda (file)
                   ;; Not `find-file-noselect': under a faked clock it takes
                   ;; the file for changed on disk and asks what to do.
                   (with-current-buffer (or (get-file-buffer file) (find-file-noselect file))
                     (concat (file-name-nondirectory file) "\0" (buffer-string))))
                 files "\0"))
            (error (format "ERROR: %S" err)))))
    (when (org-clocking-p) (org-clock-cancel))
    (with-temp-file out
      (insert (or shown "") "\2" result))))
"""


def template_form(template, paths):
    """The entry of `org-capture-templates` for the server's template."""
    target = template["target"]
    kind = target[0]
    rest = target[1:]
    if kind in ("file", "file+headline", "file+olp", "file+olp+datetree", "file+regexp"):
        rest = [paths[rest[0]]] + rest[1:]
    target_form = "(" + kind + " " + " ".join(lisp_string(part) for part in rest) + ")"
    properties = []
    names = {
        "prepend": ":prepend",
        "empty_lines": ":empty-lines",
        "empty_lines_before": ":empty-lines-before",
        "empty_lines_after": ":empty-lines-after",
        "immediate_finish": ":immediate-finish",
        "time_prompt": ":time-prompt",
        "table_line_pos": ":table-line-pos",
    }
    for key, name in names.items():
        if key in template:
            value = template[key]
            if isinstance(value, str):
                value = lisp_string(value)
            properties.append(f"{name} {'t' if value is True else 'nil' if value is False else value}")
    if "tree_type" in template:
        tree = template["tree_type"]
        tree = "(" + " ".join(tree) + ")" if isinstance(tree, list) else tree
        properties.append(f":tree-type '{tree}")
    return (
        f"(list \"t\" \"Task\" '{template.get('type', 'entry')} '{target_form}"
        f" {lisp_string(template['template'])} {' '.join(properties)})"
    )


def clock_form(clock, paths):
    if not clock:
        return "nil"
    return f"(list {lisp_string(paths[clock[0]])} {lisp_string(clock[1])})"


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
                folder = os.path.join(directory, f"{clock[:10]}-{index}")
                os.makedirs(folder)
                paths = {}
                for name, text in sorted(case["files"].items()):
                    paths[name] = os.path.join(folder, name)
                    with open(paths[name], "w", encoding="utf-8") as handle:
                        handle.write(text)
                target = os.path.join(folder, "out")
                outputs.append((case, target))
                files = " ".join(lisp_string(paths[name]) for name in sorted(paths))
                answers = " ".join(lisp_string(answer) for answer in case.get("answers", []))
                source = lisp_string(paths[case["from"]]) if "from" in case else "nil"
                adapt = case.get("options", {}).get("org_adapt_indentation", False)
                adapt = "t" if adapt is True else "nil" if adapt is False else "'" + adapt
                forms.append(
                    f"(let ((org-adapt-indentation {adapt}))"
                    f" (org-lsp-capture-case (list {files}) {source} {template_form(case['template'], paths)}"
                    f" (list {answers}) {lisp_string(case.get('typed', ''))} {lisp_string(target)}"
                    f" {clock_form(case.get('clock_in'), paths)}))"
                )
            driver = os.path.join(directory, f"driver-{clock[:10]}.el")
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
                    shown, result = handle.read().split("\2", 1)
                if result.startswith("ERROR:"):
                    case["expected"] = result
                else:
                    parts = result.split("\0")
                    case["expected"] = {"buffer": shown, "files": dict(zip(parts[0::2], parts[1::2]))}
    return cases
