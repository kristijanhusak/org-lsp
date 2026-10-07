"""Run `org-cite-insert` (`C-c C-x @`, Org's `basic` processor) in Emacs
for each case and record what it does.

A case has `text` (the Org document, `¦` where the cursor is, whose
`#+BIBLIOGRAPHY:` lines name the files), `files` (name to content), `arg`
(the prefix argument), `keys` (the answers to the key prompts, in order;
an empty one ends a list of keys) and `style` (the answer to the style
prompt). Emacs gives back, under `expected`, `text` (the document after)
and `prompts` (the prompts asked, in order), or `error`.
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
(defvar org-lsp-keys nil)
(defvar org-lsp-style nil)
(defvar org-lsp-prompts nil)
(defun org-lsp-completing-read (prompt collection &rest _)
  (push prompt org-lsp-prompts)
  (if (string-prefix-p "Style" prompt)
      (or org-lsp-style (error "No answer"))
    (let ((answer (if org-lsp-keys (pop org-lsp-keys) (error "No answer")))
          (line nil))
      (when (hash-table-p collection)
        (maphash (lambda (k v) (when (and (stringp k) (equal v answer)) (setq line k))) collection))
      (or line answer))))
(advice-add 'completing-read :override #'org-lsp-completing-read)
(defun org-lsp-cite-insert-case (in out arg keys style)
  (setq org-cite-basic--bibliography-cache nil
        org-cite-basic--file-id-cache nil
        org-lsp-keys keys
        org-lsp-style style
        org-lsp-prompts nil)
  (clrhash org-cite-basic--completion-cache)
  (let ((result
         (with-current-buffer (find-file-noselect in)
           (org-mode)
           (goto-char (point-min))
           (search-forward "¦")
           (delete-char -1)
           (prog1
               (condition-case err
                   (progn
                     (org-cite-insert arg)
                     (list (cons 'text (buffer-substring-no-properties (point-min) (point-max)))
                           (cons 'prompts (vconcat (reverse org-lsp-prompts)))))
                 (error (list (cons 'error (error-message-string err))
                              (cons 'prompts (vconcat (reverse org-lsp-prompts))))))
             (set-buffer-modified-p nil)
             (kill-buffer)))))
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
    """Fill in `expected` for every case, each in a directory of its own."""
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
                with open(os.path.join(directory, name), "w", encoding="utf-8") as handle:
                    handle.write(content)
            target = os.path.join(directory, "out.json")
            outputs.append((case, target))
            keys = "'(" + " ".join(lisp_string(key) for key in case.get("keys", [])) + ")"
            style = lisp_string(case["style"]) if case.get("style") is not None else "nil"
            arg = "'(4)" if case.get("arg") else "nil"
            forms.append(f"(org-lsp-cite-insert-case {lisp_string(source)} {lisp_string(target)} {arg} {keys} {style})")
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
