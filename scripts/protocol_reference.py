"""Hand `org-protocol:` URLs to Org in Emacs (`org-protocol.el`) and record
what it does with them.

A case is a JSON object:

    {
      "url": "org-protocol://capture?url=…",
      "options": {                          all optional
        "default_template_key": "x",        org-protocol-default-template-key
        "data_separator": "/+\\\\|\\\\?",   org-protocol-data-separator
        "projects": [{                      org-protocol-project-alist
          "name": "Worg", "base_url": "https://orgmode.org/worg/",
          "working_directory": "$DIR/worg/", "online_suffix": ".html",
          "working_suffix": ".org", "rewrites": [["org/?$", "index.org"]]
        }]
      },
      "files": ["worg/index.org"]           files made under $DIR first
    }

The URL goes through `expand-file-name` in the root directory, as Emacs's
server reads what `emacsclient` gives it, then through
`org-protocol-check-filename-for-protocol`, with Org's handlers. Capture
stops at `org-capture`, which records the keys it gets and
`org-store-link-plist`. `reference(cases)` adds `expected`:

    {
      "handler": "capture" | "store-link" | "open-source" | null,
      "result": what the check returns (the file to visit, the name itself
                when it names no handler, or null),
      "stored": org-stored-links, [[link, description], …],
      "kill": the latest kill, or null,
      "capture": { "template", "plist": [[key, value], …] } or null, the
                 value of :query as [[key, value], …],
      "messages": what was said, in order,
      "error": Emacs's error, if it stopped with one
    }

`$DIR` in the options and in what is recorded stands for the directory the
files are in, the directory Emacs runs in. Characters Unicode has no place
for are recorded as U+FFFD. Set ORG_LISP_DIR to the `lisp` directory of an
Org checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r""";;; -*- lexical-binding: t -*-
(require 'org)
(require 'org-protocol)
(require 'json)
(setq text-quoting-style 'curve)
(defvar org-lsp-messages nil)
(defvar org-lsp-handler nil)
(defvar org-lsp-captured nil)
(advice-add 'message :around
            (lambda (_orig format &rest args)
              (when format (push (apply #'format-message format args) org-lsp-messages))
              nil))
(advice-add 'warn :override #'ignore)
(fset 'server-edit #'ignore)
(fset 'raise-frame #'ignore)
(advice-add 'org-capture :override
            (lambda (&optional _goto keys)
              (setq org-lsp-captured (list keys (copy-sequence org-store-link-plist)))))
(dolist (handler '((org-protocol-capture . "capture")
                   (org-protocol-store-link . "store-link")
                   (org-protocol-open-source . "open-source")))
  (advice-add (car handler) :before
              (let ((name (cdr handler))) (lambda (&rest _) (setq org-lsp-handler name)))))

(defvar org-lsp-dir nil)
(defun org-lsp-clean (value)
  "VALUE as JSON can have it: a string with $DIR for the directory, and
U+FFFD for characters Unicode has no place for; nil for anything else."
  (when (stringp value)
    (let ((text (apply #'string
                       (mapcar (lambda (c)
                                 (if (or (> c #x10FFFF) (and (>= c #xD800) (<= c #xDFFF))) #xFFFD c))
                               value))))
      (string-replace org-lsp-dir "$DIR" text))))
(defun org-lsp-pairs (plist)
  (let (pairs)
    (while plist
      (let ((key (pop plist)) (value (pop plist)))
        (push (vector (symbol-name key)
                      (if (and (eq key :query) (listp value)) (org-lsp-pairs value) (org-lsp-clean value)))
              pairs)))
    (vconcat (nreverse pairs))))

(defun org-lsp-protocol-case (url options out)
  (setq org-stored-links nil kill-ring nil kill-ring-yank-pointer nil org-lsp-messages nil
        org-lsp-handler nil org-lsp-captured nil org-store-link-plist nil
        org-protocol-default-template-key (alist-get 'default_template_key options)
        org-protocol-data-separator (or (alist-get 'data_separator options) "/+\\|\\?")
        org-protocol-project-alist
        (mapcar (lambda (project)
                  (append (list (alist-get 'name project))
                          (let (plist)
                            (dolist (field '((base_url . :base-url) (working_directory . :working-directory)
                                             (online_suffix . :online-suffix) (working_suffix . :working-suffix)))
                              (when (alist-get (car field) project)
                                (setq plist (append plist (list (cdr field) (alist-get (car field) project))))))
                            (when (alist-get 'rewrites project)
                              (setq plist (append plist
                                                  (list :rewrites
                                                        (mapcar (lambda (rewrite) (cons (aref rewrite 0) (aref rewrite 1)))
                                                                (alist-get 'rewrites project))))))
                            plist)))
                (alist-get 'projects options)))
  (let (result error)
    (condition-case err
        (with-temp-buffer
          (org-mode)
          (setq result (org-protocol-check-filename-for-protocol (expand-file-name url "/") nil nil)))
      (error (setq error (error-message-string err))))
    (with-temp-file out
      (insert (json-encode
               (append
                (list (cons 'handler org-lsp-handler)
                      (cons 'result (org-lsp-clean result))
                      (cons 'stored (vconcat (mapcar (lambda (link) (vector (org-lsp-clean (car link))
                                                                            (org-lsp-clean (cadr link))))
                                                     org-stored-links)))
                      (cons 'kill (org-lsp-clean (car kill-ring)))
                      (cons 'capture (and org-lsp-captured
                                          (list (cons 'template (org-lsp-clean (car org-lsp-captured)))
                                                (cons 'plist (org-lsp-pairs (cadr org-lsp-captured))))))
                      (cons 'messages (vconcat (mapcar #'org-lsp-clean (reverse org-lsp-messages)))))
                (and error (list (cons 'error (org-lsp-clean error))))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def reference(cases, timeout=1200):
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as scratch:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            # Each case's files in a directory of their own, where Emacs
            # runs for it.
            directory = os.path.join(scratch, f"case-{index}")
            os.makedirs(directory)
            for name in case.get("files", []):
                path = os.path.join(directory, name)
                if name.endswith("/"):
                    os.makedirs(path, exist_ok=True)
                    continue
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write("")
            options = json.dumps(case.get("options", {})).replace("$DIR", directory)
            target = os.path.join(scratch, f"out-{index}.json")
            input_path = os.path.join(scratch, f"in-{index}.json")
            with open(input_path, "w", encoding="utf-8") as handle:
                json.dump({"url": case["url"], "options": json.loads(options)}, handle, ensure_ascii=False)
            outputs.append((case, target))
            forms.append(
                f"(let* ((json-array-type 'vector) (json-object-type 'alist)"
                f" (input (json-read-file {lisp_string(input_path)}))"
                f" (default-directory {lisp_string(directory + '/')}))"
                f" (setq org-lsp-dir {lisp_string(directory)})"
                f" (org-lsp-protocol-case (alist-get 'url input) (alist-get 'options input)"
                f" {lisp_string(target)}))"
            )
        driver = os.path.join(scratch, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC")
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True, env=env,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=timeout)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
    return cases
