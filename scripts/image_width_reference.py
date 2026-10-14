#!/usr/bin/env python3
"""Record how wide Emacs shows the images of links (`org-image-actual-width`).

For each case of `tests/link/image-width.json`, a text and a value of
`org-image-actual-width`, `widths` gets what
`org-display-inline-image--width` answers for each link of the text in
turn: pixels, or null for the image's own width. The window is made 1000
pixels wide with its text all of it, so a part of the text's width shows
as that part of 1000. Set ORG_LISP_DIR to the `lisp` directory of an Org
checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "link", "image-width.json")

DRIVER = r"""
(require 'org)
(require 'ox)
(require 'json)
(defun org-lsp-widths (in setting out)
  (let ((org-image-actual-width setting) (widths nil))
    (with-temp-buffer
      (insert-file-contents in)
      (org-mode)
      (cl-letf (((symbol-function 'window-pixel-width) (lambda (&rest _) 1000))
                ((symbol-function 'window-text-width) (lambda (&rest _) 100))
                ((symbol-function 'window-total-width) (lambda (&rest _) 100))
                ((symbol-function 'line-number-display-width) (lambda (&rest _) 0)))
        (setq auto-fill-function nil)
        (org-element-map (org-element-parse-buffer) 'link
          (lambda (link)
            (goto-char (org-element-property :begin link))
            (push (let ((width (org-display-inline-image--width link)))
                    (and (numberp width) width))
                  widths)))))
    (with-temp-file out (insert (json-encode (vconcat (nreverse widths)))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_value(value):
    if value is True:
        return "t"
    if value is False or value is None:
        return "nil"
    if isinstance(value, list):
        return "'(" + " ".join(lisp_value(item) for item in value) + ")"
    return repr(value)


def main():
    with open(PATH, encoding="utf-8") as handle:
        cases = json.load(handle)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            forms.append(f"(org-lsp-widths {lisp_string(source)} {lisp_value(case['setting'])} {lisp_string(target)})")
            outputs.append(target)
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            env=dict(os.environ, HOME=directory),
            timeout=600,
        )
        for case, target in zip(cases, outputs):
            with open(target, encoding="utf-8") as handle:
                case["widths"] = json.load(handle)
    with open(PATH, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"updated {len(cases)} cases")


if __name__ == "__main__":
    main()
