"""Hide the custom properties of Org texts in Emacs
(`org-toggle-custom-properties-visibility`) and record which lines its
overlays cover.

A case is a JSON object:

    {
      "text": "* A\n:PROPERTIES:\n:ID: 1\n:END:\n",   an Org document
      "names": ["ID"]                                  `org-custom-properties`
    }

`reference(cases)` adds `expected`: the lines hidden, from 0, each
overlay covering one line and its newline. Set ORG_LISP_DIR to the `lisp`
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
(defun org-lsp-custom-properties-case (in out names)
  (setq org-custom-properties names)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (org-toggle-custom-properties-visibility)
    (let ((lines nil))
      (dolist (overlay org-custom-properties-overlays)
        (let ((start (overlay-start overlay)) (end (overlay-end overlay)))
          ;; Each covers a whole line and its newline, if it has one.
          (unless (and (= start (save-excursion (goto-char start) (line-beginning-position)))
                       (or (= end (point-max))
                           (= end (save-excursion (goto-char start) (1+ (line-end-position))))))
            (error "Not one line: %S %S" start end))
          (push (1- (line-number-at-pos start)) lines)))
      (with-temp-file out (insert (json-encode (vconcat (sort lines #'<))))))))
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
            names = "'(" + " ".join(lisp_string(name) for name in case["names"]) + ")"
            forms.append(f"(org-lsp-custom-properties-case {lisp_string(source)} {lisp_string(target)} {names})")
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


TEXTS = [
    # The ordinary case: some of a drawer's properties, in two entries.
    "* A\n:PROPERTIES:\n:ID: 1\n:CATEGORY: work\n:CUSTOM_ID: a\n:END:\ntext\n"
    "* B\nSCHEDULED: <2026-10-05 Mon>\n:PROPERTIES:\n:CUSTOM_ID: b\n:ID: 2\n:END:\n",
    # Indented, without a value, in other cases of letters, the last line
    # without a newline.
    "* A\n  :properties:\n  :ID:\n  :id: 3\n  :Other: x\n  :end:\n* B\n:PROPERTIES:\n:ID: 4\n:END:",
    # Lines that look like properties outside a drawer: in the text, in a
    # drawer of another name, after the drawer, before the first headline.
    "#+TITLE: x\n:ID: 0\n* A\n:ID: 1\n:PROPERTIES:\n:ID: 2\n:END:\n* B\n:LOGBOOK:\n:ID: 3\n:END:\n"
    "* C\n:PROPERTIES:\n:X: 1\n:END:\n:ID: 4\n",
    # A drawer at the top of the file, after comments; and one before the
    # first headline that is not first.
    "# comment\n#\n:PROPERTIES:\n:ID: top\n:END:\n* A\n:PROPERTIES:\n:ID: a\n:END:\n",
    "#+TITLE: x\n:PROPERTIES:\n:ID: top\n:END:\n* A\n",
    # A drawer broken by a line that is not a property, and one where the
    # name has more after it.
    "* A\n:PROPERTIES:\n:ID: 1\nnot a property\n:END:\n* B\n:PROPERTIES:\n:ID+: 2\n:IDX: 3\n:ID:x\n:ID:\t4\n:END:\n",
    "* A\n:PROPERTIES:\n:ID+: 2\n:IDX: 3\n:ID:\t4\n:ID:  \n:END:\n",
    # Several of a drawer's lines, and properties in a subtree's children.
    "* A\n:PROPERTIES:\n:ID: 1\n:ID: 2\n:END:\n** B\n:PROPERTIES:\n:CATEGORY: c\n:END:\n*** C\n:PROPERTIES:\n:ID: 3\n:END:\n",
    "",
    "* A\n",
]
NAMES = [["ID"], ["ID", "CATEGORY"], [], ["CUSTOM_ID", "X"], ["id"]]


def default_cases():
    return [{"text": text, "names": names} for names in NAMES for text in TEXTS]


if __name__ == "__main__":
    import sys

    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "properties", "custom.json")
    cases = default_cases() if "--new" in sys.argv or not os.path.exists(PATH) else load(PATH)
    os.makedirs(os.path.dirname(PATH), exist_ok=True)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
