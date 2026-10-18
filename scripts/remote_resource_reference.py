#!/usr/bin/env python3
"""Record what Org decides before it downloads a remote resource
(`org--should-fetch-remote-resource-p`).

For each case of `tests/link/remote-resources.json`:

    {
      "uri": "https://example.com/a.png",
      "file": "/notes/a.org",       the file asking (its true name), or null
      "safe": ["\\\\`https://..."],   `org-safe-remote-resources`
      "policy": "prompt",           `org-resource-download-policy`: true,
                                    "prompt", "safe" or false
      "key": "d"                    the key typed when asked
    }

`expected` gets `fetch` (whether it is downloaded), and when Org asked
(`org--confirm-resource-safe`, run as if interactive): `prompt`, the
prompt read, `text`, the `*Org Remote Resource*` buffer, and `saved`, the
regexp pushed to `org-safe-remote-resources`, or null. Set ORG_LISP_DIR to
the `lisp` directory of an Org checkout to use that version instead of the
one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "link", "remote-resources.json")

DRIVER = r"""
(require 'org)
(require 'json)
(require 'cl-lib)
(defun org-lsp-remote (uri file safe policy key out)
  (let (prompt text saved)
    (with-temp-buffer
      (setq buffer-file-name file)
      (let ((org-safe-remote-resources safe)
            (org-resource-download-policy policy)
            (noninteractive nil))
        (cl-letf (((symbol-function 'read-char-choice)
                   (lambda (asked _chars)
                     (setq prompt asked
                           text (with-current-buffer "*Org Remote Resource*"
                                  (buffer-substring-no-properties (point-min) (point-max))))
                     key))
                  ((symbol-function 'pop-to-buffer) #'ignore)
                  ((symbol-function 'quit-window) #'ignore)
                  ((symbol-function 'window-body-height) (lambda (&rest _) 1000))
                  ((symbol-function 'customize-push-and-save)
                   (lambda (_symbol values) (setq saved (car values)))))
          (let ((fetch (org--should-fetch-remote-resource-p uri)))
            (setq buffer-file-name nil)
            (with-temp-file out
              (insert (json-encode
                       (list (cons 'fetch (if fetch t :json-false))
                             (cons 'prompt prompt)
                             (cons 'text text)
                             (cons 'saved saved)))))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_value(value):
    if value is True:
        return "t"
    if value is False or value is None:
        return "nil"
    if isinstance(value, list):
        return "'(" + " ".join(lisp_string(item) for item in value) + ")"
    return "'" + value


def main():
    with open(PATH, encoding="utf-8") as handle:
        cases = json.load(handle)
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            target = os.path.join(directory, f"out-{index}.json")
            file = lisp_string(case["file"]) if case.get("file") else "nil"
            key = f"?{case['key']}" if case["key"] != " " else "?\\s"
            forms.append(
                f"(org-lsp-remote {lisp_string(case['uri'])} {file} {lisp_value(case['safe'])} "
                f"{lisp_value(case['policy'])} {key} {lisp_string(target)})"
            )
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
                case["expected"] = json.load(handle)
    with open(PATH, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"updated {len(cases)} cases")


if __name__ == "__main__":
    main()
