"""Make sparse trees in Emacs (`org-sparse-tree`) and record the matches.

A case is a JSON object:

    {
      "text": "* a\\n",   an Org document
      "type": "r",       r (`org-occur`), t (`org-show-todo-tree`), T
                         (keywords), m (`org-match-sparse-tree`), d
                         (`org-check-deadlines`), b, a (before, after a
                         date), D (between two dates)
      "args": ["x"]      what is asked: the regexp, the keywords, the
                         match, the date(s)
    }

`reference(cases)` adds `expected`: `{"highlights": [[start, end]…]
(characters), "message": "..."}`, or `{"error": "..."}`. Emacs runs on
Monday 18 May 2026 at 10:30.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(defvar org-lsp-now (org-time-string-to-time "<2026-05-18 10:30>"))
(advice-add 'current-time :override (lambda () org-lsp-now))
(defvar org-lsp-message nil)
(advice-add 'message :around (lambda (f &rest args) (when (car args) (setq org-lsp-message (apply #'format-message args))) nil))
(defun org-lsp-sparse-case (in out type args)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (switch-to-buffer (current-buffer))
    (setq org-lsp-message nil)
    (pcase type
      ("r" (org-occur (car args)))
      ("t" (org-show-todo-tree nil))
      ("T" (cl-letf (((symbol-function 'completing-read) (lambda (&rest _) (car args)))) (org-show-todo-tree '(4))))
      ("m" (org-match-sparse-tree nil (car args)))
      ("d" (org-check-deadlines nil))
      ("b" (org-check-before-date (car args)))
      ("a" (org-check-after-date (car args)))
      ("D" (org-check-dates-range (car args) (cadr args))))
    (let ((highlights (sort (mapcar (lambda (o) (list (1- (overlay-start o)) (1- (overlay-end o)))) org-occur-highlights)
                            (lambda (a b) (< (car a) (car b))))))
      (with-temp-file out
        (insert (json-encode (list (cons 'highlights (vconcat (mapcar #'vconcat highlights)))
                                   (cons 'message (or org-lsp-message :json-null)))))))))
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


def reference(cases, timeout=None):
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            args = "(list " + " ".join(lisp_string(arg) for arg in case.get("args", [])) + ")"
            forms.append(
                f"(condition-case err (org-lsp-sparse-case {lisp_string(source)} {lisp_string(target)}"
                f" {lisp_string(case['type'])} {args})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err))))))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC")
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True, env=env,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
