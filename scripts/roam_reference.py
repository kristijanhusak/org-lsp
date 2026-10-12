"""Run org-roam over test cases and record its database.

A case is a JSON object:

    {
      "files": {"a.org": "...", "sub/b.org": "..."},   the files of
                                          `org-roam-directory`; each is
                                          newer than the one before it, in
                                          the order of their names
      "options": {                        optional: settings, as the
        "org_roam_file_exclude_regexp": ["^drafts/"],  server takes them
        "org_tags_exclude_from_inheritance": ["x"],
        "org_use_tag_inheritance": false
      },
      "templates": ["${title}", "${title:*} ${tags:10}"],
                                          optional: `org-roam-node-display-
                                          template`s to list the nodes with,
                                          as `org-roam-node-find` offers
                                          them, in a window 80 wide
      "titles": ["Hello, World!"]         optional: titles to slugify
    }

`reference(cases)` adds `expected`: the rows of org-roam's tables after
`org-roam-db-sync` (`files`, `nodes`, `aliases`, `tags`, `refs`, `links`,
`citations`), file names relative to the directory, then `candidates`
(for each template, what the completion offers, in its order) and `slugs`.
Rows of a node's aliases, tags and refs keep org-roam's order; nodes are
sorted by file and position, links and citations by source and position.

Org-roam and what it needs are not part of Emacs: set
ORG_LSP_EMACS_PACKAGES to a directory with checkouts of org-roam,
emacsql, magit (for `magit-section`), cond-let, llama, compat and dash.
Set ORG_LISP_DIR to the `lisp` directory of an Org checkout to use that
version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

from todo_reference import lisp_string, load, save  # noqa: F401

PACKAGES = ["cond-let", "llama", "compat", "dash.el", "emacsql", "magit/lisp", "org-roam"]

DRIVER = r"""
(require 'org)
(require 'org-roam)
(setq org-roam-list-files-commands nil)
(setq org-roam-verbose nil)
(defun org-lsp-visible (s)
  "S without its invisible parts, as a completion shows it."
  (let ((out "") (i 0))
    (while (< i (length s))
      (unless (get-text-property i 'invisible s)
        (setq out (concat out (string (aref s i)))))
      (setq i (1+ i)))
    out))
(defun org-lsp-json (x)
  (cond ((null x) :null)
        ((stringp x) (substring-no-properties x))
        ((numberp x) x)
        ((eq x t) t)
        ((consp x) (vconcat (mapcar #'org-lsp-json x)))
        (t (format "%S" x))))
(defun org-lsp-roam-case (dir out templates titles)
  (dolist (buffer (buffer-list))
    (when (buffer-file-name buffer) (kill-buffer buffer)))
  (let* ((org-roam-directory (file-name-as-directory dir))
         (org-roam-db-location (expand-file-name "../roam.db" dir))
         (relative (lambda (file) (file-relative-name file org-roam-directory)))
         (rows (lambda (table) (org-roam-db-query (vector :select '* :from table))))
         result)
    (when (file-exists-p org-roam-db-location) (delete-file org-roam-db-location))
    (org-roam-db--close-all)
    (org-roam-db-sync)
    (push (cons 'files (vconcat (mapcar (lambda (row) (vector (funcall relative (nth 0 row)) (nth 1 row)))
                                        (funcall rows 'files))))
          result)
    (push (cons 'nodes
                (vconcat
                 (mapcar
                  (lambda (row)
                    (pcase-let ((`(,id ,file ,level ,pos ,todo ,priority ,scheduled ,deadline ,title ,properties ,olp) row))
                      (vector id (funcall relative file) level pos (org-lsp-json todo) (org-lsp-json priority)
                              (org-lsp-json scheduled) (org-lsp-json deadline) (org-lsp-json title)
                              (vconcat (mapcar (lambda (p)
                                                 (vector (car p)
                                                         (if (equal (car p) "FILE") (funcall relative (cdr p))
                                                           (org-lsp-json (cdr p)))))
                                               properties))
                              (org-lsp-json olp))))
                  (funcall rows 'nodes))))
          result)
    (dolist (table '(aliases tags refs))
      (push (cons table (vconcat (mapcar (lambda (row) (vconcat (mapcar #'org-lsp-json row))) (funcall rows table))))
            result))
    (push (cons 'links
                (vconcat (mapcar (lambda (row)
                                   (pcase-let ((`(,pos ,source ,dest ,type ,props) row))
                                     (vector pos source dest type (org-lsp-json (plist-get props :outline))
                                             (org-lsp-json (plist-get props :search-option)))))
                                 (funcall rows 'links))))
          result)
    (push (cons 'citations
                (vconcat (mapcar (lambda (row)
                                   (pcase-let ((`(,node ,key ,pos ,props) row))
                                     (vector node key pos (org-lsp-json (plist-get props :outline)))))
                                 (funcall rows 'citations))))
          result)
    (push (cons 'candidates
                (vconcat (mapcar (lambda (template)
                                   (let ((org-roam-node-display-template template))
                                     (vconcat (mapcar (lambda (c) (org-lsp-visible (car c)))
                                                      (org-roam-node-read--completions)))))
                                 templates)))
          result)
    (push (cons 'slugs (vconcat (mapcar #'org-roam-node-slugify titles))) result)
    (org-roam-db--close-all)
    (with-temp-file out
      (set-buffer-file-coding-system 'utf-8)
      (insert (json-serialize (nreverse result))))))
"""

# The options of a case as Emacs's variables.
OPTIONS = {
    "org_roam_file_exclude_regexp": lambda v: "(list " + " ".join(lisp_string(x) for x in v) + ")",
    "org_tags_exclude_from_inheritance": lambda v: "(list " + " ".join(lisp_string(x) for x in v) + ")",
    "org_use_tag_inheritance": lambda v: "t" if v is True else "nil" if v is False else lisp_string(v),
    "org_roam_file_extensions": lambda v: "(list " + " ".join(lisp_string(x) for x in v) + ")",
}


def bindings(options):
    out = []
    for name, value in (options or {}).items():
        if name not in OPTIONS:
            raise ValueError(f"no Emacs setting for option {name}")
        out.append(f"({name.replace('_', '-')} {OPTIONS[name](value)})")
    return " ".join(out)


def sort_rows(expected):
    """Rows in an order that does not depend on the order files are read."""
    expected["files"].sort()
    expected["nodes"].sort(key=lambda row: (row[1], row[3]))
    order = {row[0]: index for index, row in enumerate(expected["nodes"])}
    for table in ("aliases", "tags", "refs"):
        expected[table].sort(key=lambda row: order.get(row[0], len(order)))
    expected["links"].sort(key=lambda row: (row[1], row[0]))
    expected["citations"].sort(key=lambda row: (row[0], row[2]))


def reference(cases):
    """Fill in `expected` for every case."""
    packages = os.environ.get("ORG_LSP_EMACS_PACKAGES")
    if not packages:
        raise SystemExit("set ORG_LSP_EMACS_PACKAGES to the directory of org-roam and what it needs")
    load_path = []
    if os.environ.get("ORG_LISP_DIR"):
        load_path += ["-L", os.environ["ORG_LISP_DIR"]]
    for package in PACKAGES:
        load_path += ["-L", os.path.join(packages, package)]
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            folder = os.path.join(directory, f"case-{index}", "roam")
            os.makedirs(folder)
            for number, (name, text) in enumerate(sorted(case["files"].items())):
                path = os.path.join(folder, name)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(text)
                stamp = 1_700_000_000 + number * 60
                os.utime(path, (stamp, stamp))
            target = os.path.join(directory, f"case-{index}", "out.json")
            outputs.append((case, target))
            templates = " ".join(lisp_string(t) for t in case.get("templates", []))
            titles = " ".join(lisp_string(t) for t in case.get("titles", []))
            forms.append(
                f"(condition-case err (let ({bindings(case.get('options'))})"
                f" (org-lsp-roam-case {lisp_string(folder)} {lisp_string(target)}"
                f" (list {templates}) (list {titles})))"
                f" (error (with-temp-file {lisp_string(target)} (insert (format \"ERROR: %S\" err)))))"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        subprocess.run(
            ["emacs", "--batch", "-Q", *load_path, "-l", driver],
            check=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=dict(os.environ, TZ="UTC"),
            timeout=1200,
        )
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                text = handle.read()
            if text.startswith("ERROR:"):
                case["expected"] = text
                continue
            expected = json.loads(text)
            sort_rows(expected)
            case["expected"] = expected
    return cases
