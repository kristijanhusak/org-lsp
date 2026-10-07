"""Run Emacs's `org-lint` over test cases and record its reports.

A case is a JSON object:

    {
      "text": "* a\\n#+BEGIN_SRC\\n#+END_SRC\\n",   `main.org`
      "files": [["other.org", "* b\\n"]],           optional: more files
                                                    beside it (for links
                                                    to files, IDs, setup
                                                    files)
    }

`reference(cases)` adds `expected`: the reports, each `[offset, trust,
message, checker]` with the byte offset of its place in the text, in the
order `org-lint` lists them; or `ERROR: …`. The files are in `/org`.
Every Babel language Org has is loaded, as a configuration that uses
them would have. Set ORG_LISP_DIR to the `lisp` directory of an Org
checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import shutil
import subprocess

from todo_reference import lisp_string

BASE = "/tmp/org-lsp-lint"

DRIVER = r"""
(require 'org)
(require 'org-lint)
(require 'org-id)
(require 'json)
(setq org-id-track-globally t org-id-locations-file (expand-file-name "ids" temporary-file-directory))
(org-babel-do-load-languages
 'org-babel-load-languages
 (mapcar (lambda (language) (cons language t))
         '(C R awk calc clojure css ditaa dot emacs-lisp eshell forth fortran gnuplot groovy
           haskell java js julia latex lilypond lisp lua makefile matlab maxima ocaml octave org
           perl plantuml processing python ruby sass scheme screen sed shell sql sqlite)))
(defun org-lsp-lint-case (dir main out)
  (let ((result
         (condition-case err
             (let ((org-agenda-files (directory-files-recursively dir "\\.org\\'")))
               (setq org-id-locations nil)
               (org-id-update-id-locations org-agenda-files t)
               (with-current-buffer (find-file-noselect main)
                 (let ((default-directory (file-name-directory main)))
                   (json-encode
                    (vconcat
                     (mapcar (lambda (report)
                               (let* ((row (cadr report))
                                      (marker (get-text-property 0 'org-lint-marker (aref row 0))))
                                 (vector (1- (marker-position marker))
                                         (aref row 1) (aref row 2)
                                         (symbol-name (org-lint-checker-name (aref row 3))))))
                             (org-lint)))))))
           (error (format "ERROR: %s" (error-message-string err))))))
    (dolist (buffer (buffer-list))
      (when (buffer-file-name buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))
    (with-temp-file out (insert result))))
"""


def byte_offsets(text, reports):
    """Emacs counts characters; the server counts bytes."""
    out = []
    for offset, trust, message, checker in reports:
        out.append([len(text[:offset].encode("utf-8")), trust, message, checker])
    return out


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    shutil.rmtree(BASE, ignore_errors=True)
    os.makedirs(BASE)
    try:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            directory = os.path.join(BASE, str(index), "org")
            os.makedirs(directory)
            for name, other in case.get("files", []):
                target = os.path.join(directory, name)
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with open(target, "w", encoding="utf-8") as handle:
                    handle.write(other)
            path = os.path.join(directory, "main.org")
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            out = os.path.join(BASE, str(index), "out")
            outputs.append((case, out, directory))
            forms.append(f"(org-lsp-lint-case {lisp_string(directory)} {lisp_string(path)} {lisp_string(out)})")
        driver = os.path.join(BASE, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC", HOME=BASE),
        )
        for case, out, directory in outputs:
            with open(out, encoding="utf-8") as handle:
                result = handle.read().replace(directory, "/org")
            if result.startswith("ERROR:"):
                case["expected"] = result
            else:
                case["expected"] = byte_offsets(case["text"], json.loads(result))
    finally:
        shutil.rmtree(BASE, ignore_errors=True)
    return cases


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


if __name__ == "__main__":
    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "lint", "cases.json")
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
