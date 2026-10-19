"""Fontify Org texts in Emacs and record the face each headline takes
(`org-get-level-face`): which of `org-level-1` to `org-level-8` its stars
and title are in, as `org-n-level-faces`, `org-cycle-level-faces` and
`org-odd-levels-only` say, and whether that face goes on past the end of
its text, over the newline (`org-fontify-whole-heading-line`).

A case is a JSON object:

    {
      "text": "* One\n** Two\n",   an Org document
      "n": 8,                      `org-n-level-faces` (8 when left out)
      "cycle": true,               `org-cycle-level-faces` (t when left out)
      "odd": false,                `org-odd-levels-only`
      "whole": false               `org-fontify-whole-heading-line`
    }

`reference(cases)` adds `expected`: a list of `[line, stars, title,
newline]` for each line whose stars or title have a level face, from 0;
`stars` is the level of the face of the last star, `title` that of the
first character after `* ` (null for none), `newline` that of the
newline ending the line (null for none). Set ORG_LISP_DIR to the `lisp`
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
(defun org-lsp-level-face-at (pos)
  "The N of the `org-level-N' face at POS, or nil."
  (let* ((face (get-text-property pos 'face))
         (faces (if (listp face) face (list face)))
         (found nil))
    (dolist (one faces)
      (when (and (symbolp one) (not found)
                 (string-match "\\`org-level-\\([1-8]\\)\\'" (symbol-name one)))
        (setq found (string-to-number (match-string 1 (symbol-name one))))))
    found))
(defun org-lsp-level-faces-case (in out n cycle odd whole)
  ;; Set, not bound: `#+STARTUP: odd' makes `org-odd-levels-only' local
  ;; to the buffer, which does not work under a `let' of it.
  (setq org-n-level-faces n
        org-cycle-level-faces cycle
        org-odd-levels-only odd
        org-fontify-whole-heading-line whole)
  (let ((found nil))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (font-lock-ensure)
      (goto-char (point-min))
      (while (not (eobp))
        (when (looking-at "^\\(\\**\\)\\(\\* \\)")
          (let* ((title-pos (match-end 2))
                 (stars (org-lsp-level-face-at (1- title-pos)))
                 (title (and (< title-pos (line-end-position))
                             (org-lsp-level-face-at title-pos)))
                 (newline (and (< (line-end-position) (point-max))
                               (org-lsp-level-face-at (line-end-position)))))
            (when (or stars title newline)
              (push (vector (1- (line-number-at-pos)) stars title newline) found))))
        (forward-line 1)))
    (with-temp-file out (insert (json-encode (vconcat (nreverse found)))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_bool(value):
    return "t" if value else "nil"


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


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
                f"(org-lsp-level-faces-case {lisp_string(source)} {lisp_string(target)} {case.get('n', 8)} "
                f"{lisp_bool(case.get('cycle', True))} {lisp_bool(case.get('odd', False))} "
                f"{lisp_bool(case.get('whole', False))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC")
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True, env=env,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, timeout=600)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases


# Every level from 1 to 19, then what is around headlines: a done one, an
# archived one, one without a title, stars that are not a headline, the
# last line without a newline.
DEEP = "".join("*" * level + f" Level {level}\n" for level in range(1, 20))
TEXTS = [
    DEEP,
    "* TODO One :work:\n** DONE Two\n*** Three :ARCHIVE:\n**** \n*****\ntext\n*bold* text\n",
    "Before\n* One\n  body\n** Two",
    "#+STARTUP: odd\n* One\n*** Two\n***** Three\n******* Four\n** Even\n",
]
OPTIONS = [
    {},
    {"n": 4},
    {"n": 3, "cycle": False},
    {"cycle": False},
    {"n": 1},
    {"n": 5, "odd": True},
    {"whole": True},
    {"n": 2, "cycle": False, "whole": True},
]


def default_cases():
    return [{"text": text, **options} for options in OPTIONS for text in TEXTS]


if __name__ == "__main__":
    import sys

    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "level-faces", "cases.json")
    cases = default_cases() if "--new" in sys.argv or not os.path.exists(PATH) else load(PATH)
    os.makedirs(os.path.dirname(PATH), exist_ok=True)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
