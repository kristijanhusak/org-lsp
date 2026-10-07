"""Run `org-set-tags-command` (`C-c C-q`) in Emacs for each case: a
document, where the cursor is, what is typed, and the options; record the
document after it, or the error.

A case has `text`, `point` (0-based), `typed` (the keys after `C-c C-q`,
as characters: `\\r` for RET, `\\t` for TAB; at the completion prompt the
answer, typed after the initial input is cleared) and `options`:
`org_tag_alist`, `org_tag_persistent_alist` (strings written as
`#+TAGS:` lines), `org_use_fast_tag_selection` (`true`, `false` or
`"auto"`), `org_fast_tag_selection_single_key`, `org_tags_sort_function`
(`"string<"`, `"string>"`), `org_auto_align_tags`.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(setq-default indent-tabs-mode nil)
(defun org-lsp-tags-case (in out point typed alist persistent fast single sort align)
  (let ((org-tag-alist (org-tag-string-to-alist alist))
        (org-tag-persistent-alist (org-tag-string-to-alist persistent))
        (org-use-fast-tag-selection fast)
        (org-fast-tag-selection-single-key single)
        (org-tags-sort-function sort)
        (org-auto-align-tags align))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (switch-to-buffer (current-buffer))
      (goto-char (1+ point))
      (setq last-command nil this-command nil real-this-command nil real-last-command nil)
      (let* ((buffer (current-buffer))
             (result
              (condition-case err
                  (progn (execute-kbd-macro (concat (kbd "C-c C-q") typed)) nil)
                (error (error-message-string err))
                (quit "Quit")))
             (text (with-current-buffer buffer (buffer-substring-no-properties (point-min) (point-max)))))
        (with-temp-file out
          (insert (json-encode
                   (if result
                       (list (cons 'error result))
                     (list (cons 'text text))))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_value(value, default):
    if value is None:
        value = default
    if value is True:
        return "t"
    if value is False:
        return "nil"
    return "'" + value


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


HANG = "Emacs did not finish"


def reference(cases, timeout=None):
    """Fill in `expected` for every case. Cases run a hundred at a time;
    one Emacs does not finish within `timeout` seconds (a minute by
    default) gets `{"error": HANG}`, and those after it run again."""
    timeout = timeout or 60
    done = 0
    while done < len(cases):
        chunk = cases[done:done + 100]
        finished = run(chunk, timeout)
        done += finished
        if finished < len(chunk):
            cases[done]["expected"] = {"error": HANG}
            done += 1
    return cases


def run(cases, timeout):
    """Run `cases` in one Emacs; how many finished."""
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
            options = case.get("options", {})
            sort = options.get("org_tags_sort_function")
            forms.append(
                f"(org-lsp-tags-case {lisp_string(source)} {lisp_string(target)} {case['point']}"
                f" {lisp_string(case['typed'])}"
                f" {lisp_string(options.get('org_tag_alist', ''))}"
                f" {lisp_string(options.get('org_tag_persistent_alist', ''))}"
                f" {lisp_value(options.get('org_use_fast_tag_selection'), 'auto')}"
                f" {lisp_value(options.get('org_fast_tag_selection_single_key'), False)}"
                f" {'#' + chr(39) + sort if sort else 'nil'}"
                f" {lisp_value(options.get('org_auto_align_tags'), True)})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        try:
            subprocess.run(
                ["emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            pass
        finished = 0
        for case, target in outputs:
            if not os.path.exists(target):
                break
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
            finished += 1
        return finished
