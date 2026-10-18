"""Run Emacs's `org-open-at-point` over test cases and record what it does.

A case is a JSON object:

    {
      "files": {"a.org": "See [[*B]]¦\\n* B\\n"},  files; ¦ marks the cursor,
                                                  in exactly one of them
      "answer": "1"                               optional: the key pressed
                                                  when Emacs offers the links
                                                  of an entry
      "exact": "nil"                              optional: the Lisp value of
                                                  `org-link-search-must-match-exact-headline'
      "create": true                              optional: yes to making a
                                                  missing headline
      "doi": "https://dx.doi.org/"                optional: `org-link-doi-server-url'
      "skip": "^echo"                             optional: `org-link-shell-skip-confirm-regexp'
      "man": "NAME\\n  ls - list\\n"              optional: `ol-man' loaded, and the
                                                  text `man' shows for any page
      "info_other": {"libc": "https://…"}         optional: `org-info-other-documents'
      "ctags": {"functions": [...],               optional: `org-ctags' on, with
                "answers": "yn",                  `org-ctags-open-link-functions',
                "template": "* <<%t>>\\n"}         the answers to `y-or-n-p' and
                                                  `org-ctags-new-topic-template'; a
                                                  file `TAGS' is the tags table
    }

`reference(cases)` adds `expected` to each case, one of

    {"goto": {"file": "a.org", "line": 2, "column": 0}}   where point went
    {"external": "https://orgmode.org"}                    `browse-url`
    {"system": "doc.pdf"}                                  the system's app
    {"agenda": "2026-05-18", "span": 1}                    `org-agenda-list`
    {"tags": "work"}                                       `org-tags-view`
    {"shell": "echo hi"}                                   `shell-command`
    {"man": "ls", "line": 4}                               `man`, and the line
                                                           point is left on
    {"info": ["elisp", "Top"], "url": "https://…#Top"}     `Info-find-node`, and
                                                           the URL Org exports
    {"error": "No link found"}                             a refusal

with `"offered": "..."` as well when Emacs listed the entry's links to
pick from, and `"text"`, the buffer point is in, when a headline was
made or a topic appended (`org-ctags`). With `org-ctags`, `"asked"` has
what `y-or-n-p` asked, and `"ran"` the command that made the tags table,
the case's directory written `DIR/`. The `.org` files are the agenda files, where `id:` links are
looked for. PDF and HTML files go to the system's application, as Org's
`org-file-apps` has it by default. Set ORG_LISP_DIR to the `lisp`
directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

from todo_reference import CURSOR, lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'org-id)
(require 'org-agenda)
(require 'cl-lib)
(require 'man)
(require 'ol-man)
(require 'info)
(require 'ol-info)
(require 'org-ctags)
(setq text-quoting-style 'curve)
(defun org-lsp-open-case (files cursor-file offset answer out exact create doi skip
                                man info-other ctags)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (setq tags-file-name nil tags-table-list nil)
  (let* ((recorded nil)
         (offered nil)
         (asked nil)
         (ran nil)
         (answers (and ctags (append (alist-get 'answers ctags) nil)))
         (result
          (condition-case err
              (let* ((org-agenda-files (seq-filter (lambda (file) (string-suffix-p ".org" file)) files))
                     (org-id-locations-file (make-temp-file "org-lsp-ids"))
                     (org-id-locations nil)
                     (org-id-track-globally t)
                     (org-link-search-must-match-exact-headline exact)
                     (org-link-doi-server-url doi)
                     (org-link-shell-skip-confirm-regexp skip)
                     (org-info-other-documents (or info-other org-info-other-documents))
                     (org-open-link-functions
                      (and ctags (mapcar #'intern (or (alist-get 'functions ctags)
                                                      '("org-ctags-find-tag"
                                                        "org-ctags-ask-rebuild-tags-file-then-find-tag"
                                                        "org-ctags-ask-append-topic")))))
                     (org-mode-hook (and ctags '(org-ctags--visit-tags-table)))
                     (org-ctags-new-topic-template
                      (or (alist-get 'template ctags) org-ctags-new-topic-template))
                     (org-file-apps '((auto-mode . emacs) (directory . emacs)
                                      ("\\.mm\\'" . "SYSTEM %s") ("\\.x?html?\\'" . "SYSTEM %s")
                                      ("\\.pdf\\'" . "SYSTEM %s"))))
                (org-id-update-id-locations org-agenda-files t)
                (cl-letf (((symbol-function 'browse-url)
                           (lambda (url &rest _) (setq recorded (format "external\0%s" url))))
                          ((symbol-function 'org-agenda-list)
                           (lambda (&optional _arg start-day span _with-hour)
                             (setq recorded
                                   (format "agenda\0%s\0%s"
                                           (format-time-string "%Y-%m-%d" (org-time-from-absolute start-day) t)
                                           span))))
                          ((symbol-function 'org-tags-view)
                           (lambda (&optional _todo-only match) (setq recorded (format "tags\0%s" match))))
                          ((symbol-function 'start-process-shell-command)
                           (lambda (_name _buffer command)
                             (setq recorded (format "system\0%s" (file-name-nondirectory
                                                                  (replace-regexp-in-string "\\`SYSTEM \\|['\"]" "" command))))
                             nil))
                          ((symbol-function 'shell-command)
                           (lambda (command &rest _)
                             (if ctags
                                 (progn (setq ran command) 0)
                               (setq recorded (format "shell\0%s" command)))))
                          ((symbol-function 'man)
                           (lambda (command)
                             (setq recorded (format "man\0%s" command))
                             (with-current-buffer (get-buffer-create (format "*Man %s*" command))
                               (let ((inhibit-read-only t))
                                 (erase-buffer)
                                 (insert man))
                               (current-buffer))))
                          ((symbol-function 'set-window-point)
                           (lambda (_window point)
                             (when (string-prefix-p "man\0" (or recorded ""))
                               (setq recorded (format "%s\0%d" recorded (line-number-at-pos point))))))
                          ((symbol-function 'set-window-start) #'ignore)
                          ((symbol-function 'Info-find-node)
                           (lambda (file node &rest _)
                             (setq recorded (format "info\0%s\0%s\0%s#%s" file node
                                                    (org-info-map-html-url file)
                                                    (org-info--expand-node-name node)))))
                          ((symbol-function 'y-or-n-p)
                           (lambda (prompt)
                             (push prompt asked)
                             (eq (pop answers) ?y)))
                          ((symbol-function 'yes-or-no-p) (lambda (&rest _) create))
                          ((symbol-function 'read-char-exclusive)
                           (lambda (&rest _)
                             (setq offered (with-current-buffer "*Select Link*" (buffer-string)))
                             (string-to-char answer))))
                  (switch-to-buffer (find-file-noselect cursor-file))
                  (goto-char (1+ offset))
                  (org-open-at-point)
                  (or recorded
                      (format "goto\0%s\0%d\0%d%s"
                              (file-name-nondirectory (or (buffer-file-name) (buffer-name)))
                              (line-number-at-pos) (current-column)
                              (if (or create ctags) (concat "\0" (buffer-string)) "")))))
            (error (format "error\0%s" (error-message-string err))))))
    (with-temp-file out
      (insert (or offered "") "\1" result "\1" (mapconcat #'identity (reverse asked) "\2")
              "\1" (or ran "")))))
"""


def lisp_alist(pairs):
    """An alist of strings, or nil."""
    if not pairs:
        return "nil"
    return "'(" + " ".join(f"({lisp_string(key)} . {lisp_string(value)})" for key, value in pairs.items()) + ")"


def lisp_ctags(ctags):
    """The `ctags` of a case as an alist: `answers` a string, `functions`
    a list of names, `template` a string."""
    if ctags is None:
        return "nil"
    parts = [f"(answers . {lisp_string(ctags.get('answers', ''))})"]
    if "functions" in ctags:
        parts.append("(functions . (" + " ".join(lisp_string(name) for name in ctags["functions"]) + "))")
    if "template" in ctags:
        parts.append(f"(template . {lisp_string(ctags['template'])})")
    return "'(" + " ".join(parts) + ")"


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            folder = os.path.join(directory, f"case-{index}")
            os.makedirs(folder)
            paths = {}
            cursor = None
            for name, text in sorted(case["files"].items()):
                paths[name] = os.path.join(folder, name)
                os.makedirs(os.path.dirname(paths[name]), exist_ok=True)
                if CURSOR in text:
                    cursor = (paths[name], text.index(CURSOR))
                with open(paths[name], "w", encoding="utf-8") as handle:
                    handle.write(text.replace(CURSOR, "", 1))
            target = os.path.join(folder, "out")
            outputs.append((case, target))
            files = " ".join(lisp_string(paths[name]) for name in sorted(paths))
            exact = case.get("exact", "'query-to-create")
            forms.append(
                f"(org-lsp-open-case (list {files}) {lisp_string(cursor[0])} {cursor[1]}"
                f" {lisp_string(case.get('answer', '1'))} {lisp_string(target)}"
                f" {exact} {'t' if case.get('create') else 'nil'}"
                f" {lisp_string(case.get('doi', 'https://doi.org/'))}"
                f" {lisp_string(case.get('skip', ''))}"
                f" {lisp_string(case['man']) if 'man' in case else 'nil'}"
                f" {lisp_alist(case.get('info_other'))} {lisp_ctags(case.get('ctags'))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, HOME=directory, TZ="UTC"),
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                offered, result, asked, ran = handle.read().split("\1")
            kind, *values = result.split("\0")
            if kind == "goto":
                expected = {"goto": {"file": values[0], "line": int(values[1]), "column": int(values[2])}}
                if len(values) > 3:
                    expected["text"] = values[3]
            elif kind == "man":
                expected = {"man": values[0]}
                if len(values) > 1:
                    expected["line"] = int(values[1])
            elif kind == "info":
                expected = {"info": values[0:2], "url": values[2]}
            elif kind == "agenda":
                expected = {"agenda": values[0], "span": int(values[1])}
            else:
                expected = {kind: values[0]}
            if offered:
                expected["offered"] = offered
            if "ctags" in case:
                here = os.path.dirname(target) + "/"
                expected["asked"] = [prompt.replace(here, "DIR/") for prompt in asked.split("\2")] if asked else []
                if ran:
                    expected["ran"] = ran.replace(here, "DIR/")
            case["expected"] = expected
    return cases
