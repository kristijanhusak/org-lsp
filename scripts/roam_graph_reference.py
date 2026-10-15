"""Run org-roam's graph over test cases and record the Graphviz text it
builds (`org-roam-graph--dot`), before `dot` draws it.

A case is a JSON object:

    {
      "files": {"a.org": "…", "b.org": "…"},  the files of
                                          `org-roam-directory`, each newer
                                          than the one before it in the
                                          order of their names
      "node": "a1",                       optional: the graph of the
      "distance": 0,                      nodes connected to this one
                                          (`org-roam-graph--connected-
                                          component`), within `distance`
                                          steps, 0 for all; without it,
                                          every node
      "options": {                        optional: org-roam's options
        "org-roam-graph-shorten-titles": "wrap",
        "org-roam-graph-max-title-length": 10,
        "org-roam-graph-link-hidden-types": ["file", "https"],
        "org-roam-graph-extra-config": [["rankdir", "LR"]],
        "org-roam-graph-edge-extra-config": [["dir", "back"]]
      }
    }

`reference(cases)` adds `expected`, the text. See roam_reference.py for
ORG_LSP_EMACS_PACKAGES.
"""

import json
import os
import subprocess
import tempfile

from roam_reference import PACKAGES
from todo_reference import lisp_string, load, save  # noqa: F401

DRIVER = r"""
(require 'org)
(require 'org-roam)
(require 'org-roam-graph)
(setq org-roam-list-files-commands nil)
(setq org-roam-verbose nil)
(defun org-lsp-roam-graph (dir out node distance options)
  (let* ((org-roam-directory (file-name-as-directory dir))
         (org-roam-db-location (expand-file-name "../roam.db" dir)))
    (org-roam-db-sync)
    (cl-progv (mapcar #'car options) (mapcar #'cdr options)
      (let ((graph (if node
                       (org-roam-graph--dot (org-roam-graph--connected-component node distance))
                     (org-roam-graph--dot nil 'all-nodes))))
        (org-roam-db--close-all)
        (with-temp-file out
          (set-buffer-file-coding-system 'utf-8)
          (insert (json-serialize (list (cons 'dot graph)))))))))
"""


def lisp_value(value):
    """An option's value as Lisp."""
    if value is None:
        return "nil"
    if isinstance(value, bool):
        return "t" if value else "nil"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return value if value in ("truncate", "wrap") else lisp_string(value)
    if all(isinstance(item, list) and len(item) == 2 for item in value):
        return "(" + " ".join(f"({lisp_string(k)} . {lisp_string(v)})" for k, v in value) + ")"
    return "(" + " ".join(lisp_value(item) for item in value) + ")"


def run(case, load_path):
    """One case, in an Emacs of its own."""
    with tempfile.TemporaryDirectory() as directory:
        folder = os.path.join(directory, "roam")
        os.makedirs(folder)
        for index, (name, text) in enumerate(sorted(case["files"].items())):
            path = os.path.join(folder, name)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(text)
            stamp = 1_700_000_000 + index * 60
            os.utime(path, (stamp, stamp))
        target = os.path.join(directory, "out.json")
        options = " ".join(
            f"({name} . {lisp_value(value)})" for name, value in case.get("options", {}).items()
        )
        node = lisp_string(case["node"]) if case.get("node") else "nil"
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write(DRIVER)
            handle.write(
                f"(org-lsp-roam-graph {lisp_string(folder)} {lisp_string(target)} {node}"
                f" {case.get('distance', 0)} '({options}))"
            )
        subprocess.run(
            ["emacs", "--batch", "-Q", *load_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
            timeout=120,
        )
        with open(target, encoding="utf-8") as handle:
            return json.load(handle)["dot"]


def reference(cases):
    """Fill in `expected` for every case."""
    packages = os.environ.get("ORG_LSP_EMACS_PACKAGES")
    if not packages:
        raise SystemExit("set ORG_LSP_EMACS_PACKAGES to the directory of org-roam and what it needs")
    load_path = []
    if os.environ.get("ORG_LISP_DIR"):
        load_path += ["-L", os.environ["ORG_LISP_DIR"]]
    for package in [*PACKAGES, "org-roam/extensions"]:
        load_path += ["-L", os.path.join(packages, package)]
    for case in cases:
        case["expected"] = run(case, load_path)
    return cases


if __name__ == "__main__":
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "tests", "roam", "graph.json")
    cases = load(path)
    save(path, reference(cases))
    print(f"recorded {len(cases)} cases")
