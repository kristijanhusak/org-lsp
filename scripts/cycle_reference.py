"""Press TAB (`org-cycle`) in Emacs on new, empty headlines and items, and
record what each press made of the text: `org-cycle-level` and
`org-cycle-item-indentation`, which `org-cycle-level-after-item/entry-creation`
turns on.

A case is a JSON object:

    {
      "text": "* A\\n** ",   an Org document
      "point": 7,           where the cursor is (a byte offset)
      "times": 5            how many times TAB is pressed in a row
    }

`reference(cases)` adds `expected`: for each press `{"text", "point"}`
(a byte offset), or `{"error": "..."}` and no more presses. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(advice-add 'message :override #'ignore)
(defun org-lsp-cycle-case (in out point times)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (switch-to-buffer (current-buffer))
    (goto-char (1+ point))
    (setq last-command nil)
    (let ((steps nil) (stop nil))
      (dotimes (_ times)
        (unless stop
          (push
           (condition-case err
               (progn
                 (setq this-command 'org-cycle)
                 (call-interactively #'org-cycle)
                 (setq last-command this-command)
                 (list (cons 'text (buffer-substring-no-properties (point-min) (point-max)))
                       (cons 'point (1- (point)))))
             (error (setq stop t) (list (cons 'error (error-message-string err)))))
           steps)))
      (with-temp-file out (insert (json-encode (vconcat (nreverse steps))))))))
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
                f"(org-lsp-cycle-case {lisp_string(source)} {lisp_string(target)} "
                f"{to_chars(case['text'], case['point'])} {case.get('times', 1)})"
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
                steps = json.load(handle)
            for step in steps:
                if "point" in step:
                    step["point"] = to_bytes(step["text"], step["point"])
            case["expected"] = steps
    return cases


# Texts with the cursor at the end of their last line.
TEXTS = [
    "* A\n** ",
    "* A\n** B\n*** ",
    "* A\n* ",
    "** ",
    "Intro\n*** ",
    "* A\n** B\n* ",
    "* A\n*** B\n** ",
    "* A\n** B\n*** C\n**** ",
    "* A\n* TODO ",
    "* A\n* TODO",
    "* A\n**  ",
    "* A :x:\n** ",
    "- a\n- ",
    "- a\n  - b\n  - ",
    "1. a\n2. ",
    "- a\n- [ ] ",
    "- ",
    "- a\n  - ",
    "+ a\n+ b\n+ ",
    "1) a\n   1) b\n   2) ",
    "* H\n- a\n- ",
    "- a\n- b",
    "* A\n** B",
]


def default_cases():
    cases = [{"text": text, "point": len(text.encode("utf-8")), "times": 6} for text in TEXTS]
    # An empty item in the middle of its list, and an empty headline
    # with the cursor not at its end.
    cases.append({"text": "- a\n- \n- c\n", "point": 6, "times": 4})
    cases.append({"text": "- a\n- \n- c\n", "point": 4, "times": 4})
    cases.append({"text": "* A\n** \n", "point": 4, "times": 2})
    return cases


if __name__ == "__main__":
    import sys

    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "cycle", "cases.json")
    cases = default_cases() if "--new" in sys.argv or not os.path.exists(PATH) else load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
