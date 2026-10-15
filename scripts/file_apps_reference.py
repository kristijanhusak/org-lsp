"""Ask Emacs's `org-open-file` what opens a file of a link, under
`org-file-apps`, and record it.

A case is a JSON object:

    {
      "apps": [["pdf", "evince %s"], ["directory", "emacs"]],   `org-file-apps`
                     (Org's default when left out); the cars `directory`,
                     `remote`, `auto-mode`, `system` and `t` are Emacs's
                     symbols, the cdrs `emacs`, `default`, `system` and
                     `mailcap` too
      "file": "a.pdf",       a file of the case's directory (`sub/` one there)
      "search": "5",         what follows `::` in the link, if anything
      "prefix": 4,           `C-u` (4) or `C-u C-u` (16), if any
      "vars": {"org-open-non-existing-files": true}   Org's options, if any
    }

The system's own entries are `(remote . emacs) (system . "xdg-open %s")
(t . "xdg-open %s")`, so that what the system opens shows as that command;
`auto-mode` stands for every file. `reference(cases)` adds `expected`:
`{"emacs": true}`, `{"command": "..."}` with the directory as `{dir}`, or
`{"error": "..."}`. Set ORG_LISP_DIR to the `lisp` directory of an Org
checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

FILES = ["a.pdf", "b.html", "c.txt", "D.PDF", "e.tar.gz", "notes.org", "x.xyz", "it's here.pdf"]

DRIVER = r"""
(require 'org)
(require 'json)
(defun org-lsp-file-apps-case (dir out apps file search prefix vars)
  (let* ((result
          (catch 'org-lsp-done
            (condition-case err
                (cl-progv (mapcar #'car vars) (mapcar #'cdr vars)
                 (cl-letf* ((org-file-apps (if (eq apps 'default) org-file-apps apps))
                           (auto-mode-alist '(("" . fundamental-mode)))
                           ((symbol-function 'org--file-default-apps)
                            (lambda () '((remote . emacs) (system . "xdg-open %s") (t . "xdg-open %s"))))
                           ((symbol-function 'start-process-shell-command)
                            (lambda (_name _buffer command) (throw 'org-lsp-done (list (cons 'command command)))))
                           ((symbol-function 'find-file) (lambda (&rest _) (throw 'org-lsp-done '((emacs . t)))))
                           ((symbol-function 'find-file-other-window)
                            (lambda (&rest _) (throw 'org-lsp-done '((emacs . t)))))
                           ((symbol-function 'dired) (lambda (&rest _) (throw 'org-lsp-done '((emacs . t))))))
                  (org-open-file (expand-file-name file dir) prefix nil search)
                  '((none . t))))
              (error (list (cons 'error (error-message-string err))))))))
    (with-temp-file out
      (insert (json-encode
               (mapcar (lambda (pair)
                         (if (stringp (cdr pair))
                             (cons (car pair) (replace-regexp-in-string (regexp-quote dir) "{dir}" (cdr pair) t t))
                           pair))
                       result))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_vars(variables):
    """Org's options as an alist of their values: JSON's true and false are
    `t` and `nil`."""
    pairs = []
    for name, value in (variables or {}).items():
        lisp = {True: "t", False: "nil"}.get(value) if isinstance(value, bool) else json.dumps(value)
        pairs.append(f"({name} . {lisp})")
    return "'(" + " ".join(pairs) + ")"


def lisp_apps(apps):
    if apps is None:
        return "'default"
    symbols = {"directory", "remote", "auto-mode", "system", "t"}
    commands = {"emacs", "default", "system", "mailcap"}
    pairs = []
    for key, command in apps:
        car = key if key in symbols else lisp_string(key)
        cdr = command if command in commands else lisp_string(command)
        pairs.append(f"({car} . {cdr})")
    return "'(" + " ".join(pairs) + ")"


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
    with tempfile.TemporaryDirectory(prefix="orgfileapps") as directory:
        directory = os.path.realpath(directory)
        for name in FILES:
            with open(os.path.join(directory, name), "w", encoding="utf-8") as handle:
                handle.write("x\n")
        os.makedirs(os.path.join(directory, "sub"))
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            target = os.path.join(directory, f"out-{index}.json")
            outputs.append((case, target))
            search = lisp_string(case["search"]) if case.get("search") else "nil"
            prefix = f"'({case['prefix']})" if case.get("prefix") else "nil"
            forms.append(
                f"(org-lsp-file-apps-case {lisp_string(directory)} {lisp_string(target)} {lisp_apps(case.get('apps'))}"
                f" {lisp_string(case['file'])} {search} {prefix} {lisp_vars(case.get('vars'))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, timeout=600)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases


APPS = [
    None,
    [["pdf", "evince %s"], ["\\.x?html?\\'", "firefox %s"], ["directory", "emacs"], ["t", "default"]],
    [["\\.pdf::\\([0-9]+\\)\\'", "evince -p %1 %s"], ["pdf", "zathura '%s'"], ["auto-mode", "emacs"]],
    [["txt", "system"], ["gz", "zcat %s | less"], ["tar.gz", "tar tzf \"%s\""], ["org", "emacs"], ["t", "less %s"]],
    [["directory", "ls %s"], ["\\.PDF\\'", "upper %s"], ["system", "open %s"], ["xyz", "system"]],
    [["\\(?:pdf\\|txt\\)\\'", "view \\%s %s"], ["html", "bad %x"]],
]
QUERIES = [
    (file, search, prefix)
    for file in FILES + ["sub", "missing.pdf", "missing.txt"]
    for search, prefix in [(None, None), ("5", None), ("*Heading", None), (None, 4), (None, 16)]
]


def default_cases():
    cases = []
    for apps in APPS:
        for file, search, prefix in QUERIES:
            case = {"file": file}
            if apps is not None:
                case["apps"] = apps
            if search:
                case["search"] = search
            if prefix:
                case["prefix"] = prefix
            cases.append(case)
    # `org-open-non-existing-files`: what is not there opens as well,
    # except in Emacs, which visits it either way.
    for apps in APPS:
        for file in ["missing.pdf", "missing.txt", "missing.org"]:
            for prefix in [None, 4, 16]:
                case = {"file": file, "vars": {"org-open-non-existing-files": True}}
                if apps is not None:
                    case["apps"] = apps
                if prefix:
                    case["prefix"] = prefix
                cases.append(case)
    return cases


if __name__ == "__main__":
    import sys

    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "file-apps", "cases.json")
    cases = default_cases() if "--new" in sys.argv or not os.path.exists(PATH) else load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
