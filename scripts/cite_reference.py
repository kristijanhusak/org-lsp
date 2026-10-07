"""Run Org's citation support (oc.el with its `basic` processor) in Emacs
for each case: an Org document and the bibliography files beside it;
record what Org makes of them.

A case has `text` (the Org document, whose `#+BIBLIOGRAPHY:` lines name
the files), `files` (name to content), and optionally `unknown`, keys to
find close keys for. Emacs gives back, under `expected`:

- `entries`: per bibliography file as `org-cite-basic--parse-bibliography`
  lists them, its name and its entries in order, each a list of
  `[field, value]` (with `id` and `type`);
- `keys`: `org-cite-basic--all-keys`;
- `plain`: per key, the entry as the help echo shows it
  (`org-cite-basic--print-entry` with `plain`);
- `completion`: per key, its line in the completion table;
- `goto`: per key, the file and the 0-based offset `org-cite-basic-goto`
  goes to;
- `close`: per unknown key, `org-cite-basic--close-keys`;
- `error`, when Emacs fails.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'oc)
(require 'oc-basic)
(require 'json)
(setq-default indent-tabs-mode nil)
(defun org-lsp-cite-case (in out unknown)
  (setq org-cite-basic--bibliography-cache nil
        org-cite-basic--file-id-cache nil)
  (clrhash org-cite-basic--completion-cache)
  (let ((result
         (condition-case err
             (with-current-buffer (find-file-noselect in)
               (org-mode)
               (let* ((bibliography (org-cite-basic--parse-bibliography))
                      (keys (org-cite-basic--all-keys))
                      (table (org-cite-basic--key-completion-table))
                      (completion nil))
                 (when table
                   (maphash (lambda (line key) (when (stringp key) (push (cons key line) completion))) table))
                 (prog1
                     (list
                      (cons 'entries
                            (vconcat
                             (mapcar (lambda (pair)
                                       (let (entries)
                                         (maphash (lambda (key fields)
                                                    (push (vector key (vconcat (mapcar (lambda (f) (vector (symbol-name (car f)) (cdr f))) fields)))
                                                          entries))
                                                  (cdr pair))
                                         (vector (file-name-nondirectory (car pair)) (vconcat (nreverse entries)))))
                                     bibliography)))
                      (cons 'keys (vconcat keys))
                      (cons 'plain
                            (vconcat
                             (mapcar (lambda (key)
                                       (vector key (condition-case e
                                                       (org-element-interpret-data
                                                        (org-cite-basic--print-entry (org-cite-basic--get-entry key) "plain"))
                                                     (error (concat "ERROR: " (error-message-string e))))))
                                     keys)))
                      (cons 'completion
                            (vconcat (mapcar (lambda (pair) (vector (car pair) (substring-no-properties (cdr pair))))
                                             (nreverse completion))))
                      (cons 'goto
                            (vconcat
                             (mapcar (lambda (key)
                                       ;; Each key in a fresh buffer: where Org finds no
                                       ;; entry, the file shows from its start.
                                       (dolist (buffer (buffer-list))
                                         (when (and (buffer-file-name buffer) (not (eq buffer (get-file-buffer in))))
                                           (kill-buffer buffer)))
                                       (condition-case e
                                           (save-window-excursion
                                             (let ((org-link-frame-setup '((file . find-file))))
                                               (org-cite-basic-goto
                                                (org-element-create 'citation-reference (list :key key)) nil)
                                               (vector key (file-name-nondirectory (buffer-file-name)) (1- (point)))))
                                         (error (vector key (concat "ERROR: " (error-message-string e)) -1))))
                                     keys)))
                      (cons 'close
                            (vconcat (mapcar (lambda (key) (vector key (vconcat (org-cite-basic--close-keys key keys))))
                                             unknown))))
                   (dolist (buffer (buffer-list))
                     (when (buffer-file-name buffer) (with-current-buffer buffer (set-buffer-modified-p nil)) (kill-buffer buffer))))))
           (error (list (cons 'error (error-message-string err)))))))
    (with-temp-file out
      (insert (json-encode result)))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases, timeout=120):
    """Fill in `expected` for every case. Each case gets a directory of
    its own, as its files name each other relatively."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as root:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            directory = os.path.join(root, f"case-{index}")
            os.makedirs(directory)
            source = os.path.join(directory, "doc.org")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            for name, content in case.get("files", {}).items():
                path = os.path.join(directory, name)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(content)
            target = os.path.join(directory, "out.json")
            outputs.append((case, target))
            unknown = "'(" + " ".join(lisp_string(key) for key in case.get("unknown", [])) + ")"
            forms.append(f"(org-lsp-cite-case {lisp_string(source)} {lisp_string(target)} {unknown})")
        driver = os.path.join(root, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        try:
            subprocess.run(
                ["emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=False, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            pass
        for case, target in outputs:
            if os.path.exists(target):
                with open(target, encoding="utf-8") as handle:
                    case["expected"] = json.load(handle)
            else:
                case["expected"] = {"error": "Emacs did not finish"}
    return cases
