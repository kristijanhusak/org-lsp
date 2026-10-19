"""Run the commands of `ol-bibtex.el` in Emacs and record what they do.

A case is a JSON object:

    {
      "command": "create",   create, create-here, check, check-all, yank,
                             import, copy or export
      "text": "* a\\n",       an Org document
      "point": 3,            where the cursor is, in characters from the start
      "optional": true,      the prefix argument: optional fields too (create,
                             create-here, check), or for yank the data added
                             to the entry at point
      "answers": ["article"], what is typed at each prompt, in turn
      "clipboard": "@misc{…", what yank reads (the latest kill)
      "bib": "@misc{…",       the file import reads
      "options": {"org-bibtex-prefix": "\\"BIB_\\""}
                             options, each a Lisp expression
    }

`reference(cases)` adds `expected`: `text` and `point` after the command,
`prompts` (each `prompt`, the `initial` text and the `completions`
offered), `message` (what Emacs said last, the exported file's name as
FILE), `result` (what copy puts in the kill ring), `file` (what export
writes) and `autokey` (the entry `bibtex-generate-autokey` reads), or
`error` when Org stops with one. An answer missing is an error
(`org-lsp: no answer for …`).
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'ol-bibtex)
(require 'json)
(require 'cl-lib)
(setq-default indent-tabs-mode nil)
;; `bibtex-generate-autokey' needs the dialect's regexps, which only
;; `bibtex-mode' sets: as after a BibTeX file was opened.
(bibtex-set-dialect 'BibTeX)
(defvar org-lsp-answers nil)
(defvar org-lsp-prompts nil)
(defvar org-lsp-message nil)
(defvar org-lsp-autokey nil)
(defun org-lsp-ask (prompt initial completions)
  (push (list (cons 'prompt prompt)
              (cons 'initial initial)
              (cons 'completions (and completions (vconcat completions))))
        org-lsp-prompts)
  (if org-lsp-answers
      (pop org-lsp-answers)
    (error "org-lsp: no answer for %s" prompt)))
(advice-add 'message :around
            (lambda (old format &rest args)
              (let ((said (apply old format args)))
                (when (and format said) (setq org-lsp-message said))
                said)))
(advice-add 'bibtex-generate-autokey :before
            (lambda (&rest _)
              (setq org-lsp-autokey (buffer-substring-no-properties (point-min) (point-max)))))
(defun org-lsp-bibtex-case (in out bibfile)
  (let* ((case (json-read-file in))
         (command (alist-get 'command case))
         (optional (eq (alist-get 'optional case) t))
         (options (alist-get 'options case))
         (result nil) (file nil) (text nil) (point nil) (error-text nil))
    (setq org-lsp-answers (append (alist-get 'answers case) nil)
          org-lsp-prompts nil
          org-lsp-message nil
          org-lsp-autokey nil
          org-bibtex-entries nil
          kill-ring nil)
    (when (alist-get 'clipboard case) (kill-new (alist-get 'clipboard case)))
    (when (alist-get 'bib case)
      (with-temp-file bibfile (insert (alist-get 'bib case))))
    (with-temp-buffer
      (insert (alist-get 'text case))
      (org-mode)
      (switch-to-buffer (current-buffer))
      (goto-char (1+ (alist-get 'point case)))
      (cl-progv (mapcar (lambda (pair) (car pair)) options)
          (mapcar (lambda (pair) (eval (car (read-from-string (cdr pair))) t)) options)
        (cl-letf (((symbol-function 'read-from-minibuffer)
                   (lambda (prompt &optional initial &rest _)
                     (org-lsp-ask prompt initial nil)))
                  ((symbol-function 'completing-read)
                   (lambda (prompt collection &optional _pred _req initial &rest _)
                     (org-lsp-ask prompt initial (all-completions "" collection)))))
          (condition-case err
              (pcase command
                ("create" (org-bibtex-create optional))
                ("create-here" (org-bibtex-create-in-current-entry optional))
                ("check" (org-bibtex-check optional))
                ("check-all" (org-bibtex-check-all))
                ("yank" (org-bibtex-yank optional))
                ("import" (org-bibtex-import-from-file bibfile))
                ("copy" (setq result (org-bibtex-export-to-kill-ring)))
                ("export" (org-bibtex bibfile)
                 (when (file-exists-p bibfile)
                   (setq file (with-temp-buffer (insert-file-contents bibfile) (buffer-string))))))
            (error (setq error-text (error-message-string err)))))
        (setq text (buffer-substring-no-properties (point-min) (point-max))
              point (1- (point)))))
    (when (get-file-buffer bibfile) (kill-buffer (get-file-buffer bibfile)))
    (when (file-exists-p bibfile) (delete-file bibfile))
    (with-temp-file out
      (insert
       (json-encode
        (if error-text
            (list (cons 'error error-text)
                  (cons 'prompts (vconcat (reverse org-lsp-prompts))))
          (list (cons 'text text)
                (cons 'point point)
                (cons 'prompts (vconcat (reverse org-lsp-prompts)))
                (cons 'message (and org-lsp-message
                                    (replace-regexp-in-string (regexp-quote bibfile) "FILE" org-lsp-message t t)))
                (cons 'result result)
                (cons 'file file)
                (cons 'autokey org-lsp-autokey))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases, timeout=1200):
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        bibfile = os.path.join(directory, "out.bib")
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.json")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                json.dump({key: value for key, value in case.items() if key != "expected"}, handle)
            outputs.append((case, target))
            forms.append(
                f"(condition-case err (org-lsp-bibtex-case {lisp_string(source)} {lisp_string(target)}"
                f" {lisp_string(bibfile)})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err))))))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC")
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True, env=env,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       timeout=timeout)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
