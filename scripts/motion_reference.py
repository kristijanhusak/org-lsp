"""Move about in Emacs with Org's motion commands and record where the point
ends.

A case is a JSON object:

    {
      "text": "* a\\n* b\\n",   an Org document
      "point": 0,               where the cursor is (a byte offset)
      "command": "org-next-visible-heading",
      "count": 1,               the prefix argument (1 when left out)
      "times": 1                how many times the command runs in a row
                                (the link search wraps around on a
                                repeat after it failed)
    }

The commands: `org-next-visible-heading` (`C-c C-n`),
`org-previous-visible-heading` (`C-c C-p`),
`org-forward-heading-same-level` (`C-c C-f`),
`org-backward-heading-same-level` (`C-c C-b`), `outline-up-heading`
(`C-c C-u`), `org-next-link` (`C-c C-x C-n`), `org-previous-link`
(`C-c C-x C-p`).

`reference(cases)` adds `expected`: `{"point": byte offset, "message":
what Emacs said last or null}`, or `{"error": "..."}`. Set ORG_LISP_DIR to
the `lisp` directory of an Org checkout to use that version instead of the
one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(defvar org-lsp-message nil)
(advice-add 'message :around
            (lambda (f &rest args)
              (when (car args) (setq org-lsp-message (apply #'format-message args)))
              nil))
(defun org-lsp-motion-case (in out point command count times)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (switch-to-buffer (current-buffer))
    (goto-char (1+ point))
    (setq org-lsp-message nil last-command nil)
    (let ((result
           (condition-case err
               (progn
                 (dotimes (_ times)
                   (let ((current-prefix-arg (unless (= count 1) count)))
                     (setq this-command (intern command) org-lsp-message nil)
                     (call-interactively (intern command))
                     (setq last-command this-command)))
                 (list (cons 'point (1- (point)))
                       (cons 'message org-lsp-message)))
             (error (list (cons 'error (error-message-string err)))))))
      (with-temp-file out (insert (json-encode result))))))
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


def to_chars(text, offset):
    return len(text.encode("utf-8")[:offset].decode("utf-8"))


def to_bytes(text, offset):
    return len(text[:offset].encode("utf-8"))


def reference(cases):
    """Fill in `expected` for every case."""
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
            forms.append(
                f"(org-lsp-motion-case {lisp_string(source)} {lisp_string(target)} "
                f"{to_chars(case['text'], case['point'])} {lisp_string(case['command'])} "
                f"{case.get('count', 1)} {case.get('times', 1)})"
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
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                result = json.load(handle)
            if "point" in result:
                result["point"] = to_bytes(case["text"], result["point"])
            case["expected"] = result
    return cases


if __name__ == "__main__":
    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "motion", "cases.json")
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
