#!/usr/bin/env python3
"""Record the appointments Emacs's `org-agenda-to-appt` makes of the Org
files in tests/appt, which `org-lsp notify` reminds of.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/appt_reference.py

Each FILE.org is the only agenda file, on Thursday 22 October 2026 at
noon; FILE.json is `appt-time-msg-list` after `org-agenda-to-appt` with its
defaults: a list of [minutes after midnight, message, warning time or
null]. Set ORG_LISP_DIR to the `lisp` directory of an Org checkout to use
that version instead of the one bundled with Emacs.
"""

import glob
import json
import os
import subprocess
import sys
import tempfile

DRIVER = r""";;; -*- lexical-binding: t -*-
(require 'org-agenda)
(require 'appt)
(require 'json)
(defconst org-lsp-now (encode-time 0 0 12 22 10 2026))
(advice-add 'current-time :override (lambda () org-lsp-now))
(advice-add 'time-to-days :around
            (lambda (f &optional time) (funcall f (or time org-lsp-now))))
(advice-add 'message :override #'ignore)
(defun org-lsp-appt (file out)
  (let ((org-agenda-files (list file)))
    (setq appt-time-msg-list nil)
    (org-agenda-to-appt)
    (when (timerp appt-timer) (cancel-timer appt-timer))
    (with-temp-file out
      (insert (json-encode
               (vconcat (mapcar (lambda (appt)
                                  (vector (caar appt) (substring-no-properties (cadr appt)) (nth 3 appt)))
                                appt-time-msg-list))))
      (insert "\n"))))
"""


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    files = sorted(glob.glob(os.path.join(root, "tests", "appt", "*.org")))
    if not files:
        sys.exit("no tests/appt/*.org")
    load = []
    if os.environ.get("ORG_LISP_DIR"):
        load = ["-L", os.environ["ORG_LISP_DIR"]]
    calls = "".join(f'(org-lsp-appt {json.dumps(path)} {json.dumps(path[:-4] + ".json")})\n' for path in files)
    with tempfile.NamedTemporaryFile("w", suffix=".el", delete=False, encoding="utf-8") as driver:
        driver.write(DRIVER + calls)
    try:
        subprocess.run(["emacs", "--batch", "-Q", *load, "-l", driver.name], check=True)
    finally:
        os.unlink(driver.name)
    for path in files:
        print(f"wrote {os.path.relpath(path[:-4] + '.json', root)}")


if __name__ == "__main__":
    main()
