"""Run Org's line commands in Emacs from every position of a text and
record what they did: `org-beginning-of-line` (`C-a`) and
`org-end-of-line` (`C-e`) by `org-special-ctrl-a/e`, `org-kill-line`
(`C-k`) by `org-special-ctrl-k`.

A case is a JSON object:

    {
      "text": "* TODO a :x:\\n",       an Org document
      "command": "org-end-of-line",
      "special": true,                 `org-special-ctrl-a/e`: false, true,
                                       "reversed", or [C-a, C-e]; for
                                       `org-kill-line`, `org-special-ctrl-k`
      "times": 1,                      how many times the command runs in a
                                       row (a repeated C-a, C-k appending)
      "count": 1                       the prefix argument (1 when left out)
    }

`reference(cases)` adds `expected`: one result for each position of the
text, in characters from 0 to its end, given as byte offsets: `{"point":
byte offset}`, with `"text"` and `"kill"` (the latest kill) for
`org-kill-line`, or `{"error": "..."}`. Set ORG_LISP_DIR to the `lisp`
directory of an Org checkout to use that version instead of the one
bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(advice-add 'message :override #'ignore)
(defun org-lsp-line-value (value)
  (cond ((eq value :json-false) nil)
        ((equal value "reversed") 'reversed)
        ((vectorp value) (cons (org-lsp-line-value (aref value 0)) (org-lsp-line-value (aref value 1))))
        (t value)))
(defun org-lsp-line-case (in out command special times count)
  (let ((results nil) (text (with-temp-buffer (insert-file-contents in) (buffer-string))))
    (dotimes (point (1+ (length text)))
      (with-temp-buffer
        (insert text)
        (org-mode)
        (switch-to-buffer (current-buffer))
        (goto-char (1+ point))
        (setq last-command nil kill-ring nil)
        (let ((org-special-ctrl-a/e special)
              (org-special-ctrl-k special))
          (push
           (condition-case err
               (progn
                 (dotimes (_ times)
                   (let ((current-prefix-arg (unless (= count 1) count)))
                     (setq this-command (intern command))
                     (call-interactively (intern command))
                     (setq last-command this-command)))
                 (append (list (cons 'point (1- (point))))
                         (when (eq (intern command) 'org-kill-line)
                           (list (cons 'text (buffer-string))
                                 (cons 'kill (if kill-ring (substring-no-properties (car kill-ring)) :null))))))
             (error (list (cons 'error (error-message-string err)))))
           results))))
    (with-temp-file out (insert (json-encode (vconcat (nreverse results)))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_value(value):
    if value is False:
        return "nil"
    if value is True:
        return "t"
    if value == "reversed":
        return "'reversed"
    if isinstance(value, list):
        return f"'({lisp_value(value[0]).lstrip(chr(39))} . {lisp_value(value[1]).lstrip(chr(39))})"
    raise ValueError(value)


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("[\n")
        handle.write(",\n".join(json.dumps(case, ensure_ascii=False) for case in cases))
        handle.write("\n]\n")


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
                f"(org-lsp-line-case {lisp_string(source)} {lisp_string(target)} "
                f"{lisp_string(case['command'])} {lisp_value(case['special'])} "
                f"{case.get('times', 1)} {case.get('count', 1)})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                results = json.load(handle)
            for result in results:
                if "point" in result:
                    after = result.get("text", case["text"])
                    result["point"] = to_bytes(after, result["point"])
            case["expected"] = results
    return cases


TEXTS = [
    "* TODO [#A] Title here :tag:other:\nbody\n",
    "** DONE Done\n",
    "* Title\t:a:\n",
    "*** \n",
    "***\n* x\n",
    "* [#B] only priority\n",
    "* TODO\n",
    "* COMMENT commented :c:\n",
    "* Déjà vu  :été:\nnext\n",
    "- item one\n- [ ] box\n- [X]  wide box\n",
    "  + nested\n  1. numbered\n  2) [-] partial\n",
    "- term :: description\n",
    "- [@3] counter\n",
    "a. alpha\n",
    "#+begin_src emacs-lisp\n- not an item\n#+end_src\n",
    "Plain text line\n  indented\n",
    "-not an item\n",
    "* Heading :not tags\n",
    "* Tags only :x:y:  \n",
    "- \n",
    "* a\n** b :t:\n",
    "line\n\n   \nlast",
    "*\t:a:\n**a\n",
    "* TODO:x:\n* DONE  [#10] numbered priority\t:p:\n",
    " * star item\n1) [X] done\n",
    "* 日本語 heading :tag:\n",
]


def default_cases():
    cases = []
    specials = [False, True, "reversed", [True, "reversed"], ["reversed", False]]
    for text in TEXTS:
        for command in ("org-beginning-of-line", "org-end-of-line"):
            for special in specials:
                for times in (1, 2):
                    cases.append({"text": text, "command": command, "special": special, "times": times})
            for count in (2, 3, 0, -1):
                cases.append({"text": text, "command": command, "special": True, "times": 1, "count": count})
        for special in (False, True):
            for times in (1, 2):
                cases.append({"text": text, "command": "org-kill-line", "special": special, "times": times})
    return cases


if __name__ == "__main__":
    import sys

    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "line", "cases.json")
    cases = default_cases() if "--new" in sys.argv or not os.path.exists(PATH) else load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
