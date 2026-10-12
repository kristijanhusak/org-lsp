"""Run Emacs's `org-read-date` over answers to its prompt and record the result.

A case is a JSON object:

    {
      "clock": "2026-05-18 10:30",     the pretended current time
      "answer": "fri 3pm",             what the user typed at the prompt
      "default": "2026-05-20 09:00",   optional: the date the prompt starts
                                       from, as when a timestamp is changed
      "preferFuture": true             optional: org-read-date-prefer-future,
                                       true, false or "time"
    }

`reference(cases)` adds `expected` to each case: the date `org-read-date`
returns, with ` HH:MM` when the answer gave a time, and ` | END` when it
gave the end of a time range (`org-end-time-was-given`, as typed). Emacs
runs under faketime, so both programs see the same clock. Set
ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(defun org-lsp-read-date-case (answer default out)
  (let ((result
         (condition-case err
             (let ((org-time-was-given nil)
                   (org-end-time-was-given nil))
               (let ((date (org-read-date nil nil answer nil default)))
                 (concat date
                         (if org-end-time-was-given
                             (concat " | " org-end-time-was-given)
                           ""))))
           (error (format "ERROR: %S" err)))))
    (with-temp-file out (insert result))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_default(default):
    if not default:
        return "nil"
    date, _, time = default.partition(" ")
    year, month, day = (int(part) for part in date.split("-"))
    hour, minute = (int(part) for part in (time or "0:00").split(":"))
    return f"(org-encode-time 0 {minute} {hour} {day} {month} {year})"


def lisp_prefer_future(value):
    if value is False:
        return "nil"
    if value == "time":
        return "'time"
    return "t"


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases):
    """Fill in `expected` for every case."""
    by_clock = {}
    for case in cases:
        by_clock.setdefault(case["clock"], []).append(case)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        for clock, group in by_clock.items():
            forms = [DRIVER]
            outputs = []
            for index, case in enumerate(group):
                target = os.path.join(directory, f"out-{index}.txt")
                outputs.append((case, target))
                forms.append(
                    f"(let ((org-read-date-prefer-future {lisp_prefer_future(case.get('preferFuture', True))}))\n"
                    f"  (org-lsp-read-date-case {lisp_string(case['answer'])} {lisp_default(case.get('default'))}"
                    f" {lisp_string(target)}))"
                )
            driver = os.path.join(directory, "driver.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write("\n".join(forms))
            subprocess.run(
                ["faketime", f"{clock}:00", "emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=dict(os.environ, TZ="UTC"),
                timeout=1200,
            )
            for case, target in outputs:
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = handle.read()
    return cases
