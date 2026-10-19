"""Cut, copy and paste subtrees in Emacs and record the result.

A case is a JSON object:

    {
      "text": "* a\\n",   an Org document
      "point": 3,        where the cursor is, in characters from the start
      "op": "cut",       "cut" (`org-cut-subtree`), "copy"
                         (`org-copy-subtree`), "paste"
                         (`org-paste-subtree`) or "yank" (`org-yank`)
      "count": 2,        the prefix argument of a cut or copy (optional)
      "kill": "* b\\n",  for a paste or a yank, what is pasted
      "level": 3,        for a paste, a numeric prefix argument (optional)
      "adapt": "nil",    org-adapt-indentation: nil, t or headline-data
      "adjusted": true,  for a yank, `org-yank-adjusted-subtrees` (true
                         when left out)
      "folded": true,    for a yank, `org-yank-folded-subtrees` (true when
                         left out)
      "fold": true,      for a cut or copy, the subtree at the cursor
                         folded first (optional)
      "paste": 4         for a cut or copy, then `org-paste-subtree` at
                         this point of the text after it (optional)
    }

`reference(cases)` adds `expected`: `{"text", "point", "kill"}` after the
command (`kill` for a cut or copy), or `{"error": "..."}` when Org stops
with an error. After a yank, or a cut or copy and its paste, `folds` lists
the lines (from 0) that show with their end hidden, the headlines folded,
and a yank has `message`, what Org said last. The commands run as called
interactively.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(setq-default indent-tabs-mode nil)
(defvar org-lsp-last-message nil)
(advice-add 'message :filter-return (lambda (said) (setq org-lsp-last-message said)))
(defun org-lsp-subtree-folds ()
  "The lines, from 0, that show with their end hidden."
  (let (lines)
    (save-excursion
      (goto-char (point-min))
      (while (not (eobp))
        (when (and (not (org-invisible-p (line-beginning-position)))
                   (org-invisible-p (line-end-position)))
          (push (1- (line-number-at-pos)) lines))
        (forward-line 1)))
    (vconcat (nreverse lines))))
(defun org-lsp-subtree-case (in out point op count kill level adapt adjusted folded fold paste)
  (let ((org-adapt-indentation adapt) (folds nil) (said :json-null))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (switch-to-buffer (current-buffer))
      (goto-char (1+ point))
      (setq kill-ring nil org-subtree-clip nil org-subtree-clip-folded nil org-lsp-last-message nil)
      (when fold
        (save-excursion (org-back-to-heading t) (org-fold-subtree t)))
      (cond
       ((equal op "cut") (let ((current-prefix-arg count)) (call-interactively #'org-cut-subtree)))
       ((equal op "copy") (let ((current-prefix-arg count)) (call-interactively #'org-copy-subtree)))
       ((equal op "yank")
        (kill-new kill)
        (let ((org-yank-adjusted-subtrees adjusted) (org-yank-folded-subtrees folded))
          (call-interactively #'org-yank))
        (setq folds (org-lsp-subtree-folds) said (or org-lsp-last-message :json-null)))
       (t (kill-new kill)
          (let ((current-prefix-arg level)) (call-interactively #'org-paste-subtree))))
      (let ((copied (when (member op '("cut" "copy")) (car kill-ring))))
        (when paste
          (org-fold-show-all)
          (goto-char (1+ paste))
          (call-interactively #'org-paste-subtree)
          (setq folds (org-lsp-subtree-folds)))
        (let ((text (buffer-substring-no-properties (point-min) (point-max)))
              (at (1- (point))))
          (with-temp-file out
            (insert (json-encode (append (list (cons 'text text) (cons 'point at)
                                               (cons 'kill (or copied :json-null)))
                                         (when folds (list (cons 'folds folds)))
                                         (when (equal op "yank") (list (cons 'message said))))))))))))
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
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            adapt = case.get("adapt", "nil")
            adapt = adapt if adapt in ("nil", "t") else "'" + adapt
            count = str(case["count"]) if case.get("count") else "nil"
            level = str(case["level"]) if case.get("level") else "nil"
            adjusted = "nil" if case.get("adjusted") is False else "t"
            folded = "nil" if case.get("folded") is False else "t"
            fold = "t" if case.get("fold") else "nil"
            paste = str(case["paste"]) if case.get("paste") is not None else "nil"
            forms.append(
                f"(condition-case err (org-lsp-subtree-case {lisp_string(source)} {lisp_string(target)}"
                f" {case['point']} {lisp_string(case['op'])} {count} {lisp_string(case.get('kill', ''))} {level} {adapt}"
                f" {adjusted} {folded} {fold} {paste})"
                f" (error (with-temp-file {lisp_string(target)}"
                f" (insert (json-encode (list (cons 'error (error-message-string err))))))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
