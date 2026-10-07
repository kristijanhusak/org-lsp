"""Run Org's property commands in Emacs for each case: a document, where
the cursor is, the command, the answers to its questions and the options;
record the document after it (or the error), the questions asked and
Org's last message.

A case has `text`, `point` (0-based), `command` and `answers`, what each
completion prompt returns in turn. The commands:

- `set`: `org-set-property` (`C-c C-x p`)
- `delete`: `org-delete-property`
- `delete-globally`: `org-delete-property-globally`
- `next`, `previous`: `org-property-next-allowed-value` and
  `org-property-previous-allowed-value` (`S-<right>`, `S-<left>` on a
  property line)
- `effort`: `org-set-effort` (`C-c C-x e`); `inc-effort`: `org-inc-effort`
- `action`: `org-property-action` (`C-c C-c` in a property drawer), with
  `key` the key pressed at its menu
- `ordered`: `org-toggle-ordered-property` (`C-c C-x o`)

`options` may hold `org_use_property_inheritance` (`true`, `false`, a
list of names or a regexp) and `org_global_properties` (an object).

What Emacs gives back, under `expected`: `text` or `error`, `prompts`
(each with `prompt`, `collection`, `require_match` and `default`) and
`message`.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'org-clock)
(require 'json)
(require 'cl-lib)
(setq-default indent-tabs-mode nil)
(defun org-lsp-properties-case (in out point command answers key inheritance globals)
  (let* ((prompts nil)
         (last-message nil)
         (org-use-property-inheritance inheritance)
         (org-global-properties globals)
         (org-last-set-property nil)
         (org-last-set-property-value nil))
    (cl-letf (((symbol-function 'completing-read)
               (lambda (prompt collection &optional _predicate require-match initial _hist def &rest _)
                 (push (list (cons 'prompt prompt)
                             (cons 'collection
                                   (vconcat (delq nil (mapcar (lambda (c) (let ((c (if (consp c) (car c) c)))
                                                                            (and c (substring-no-properties c))))
                                                              (if (functionp collection) nil collection)))))
                             (cons 'require_match (if require-match t :json-false))
                             (cons 'initial (and initial (substring-no-properties initial)))
                             (cons 'default (and (stringp def) (substring-no-properties def))))
                       prompts)
                 ;; An empty answer is the default, as `completing-read'
                 ;; has it.
                 (let ((answer (if answers (pop answers) (error "Asked once too often")))
                       (def (if (consp def) (car def) def)))
                   (if (and (equal answer "") def) def answer))))
              ((symbol-function 'read-char-exclusive)
               (lambda (&rest _) (if key (prog1 key (setq key nil)) (error "Asked for a key twice"))))
              ((symbol-function 'message)
               (lambda (format &rest args)
                 (when format (setq last-message (apply #'format-message format args)))
                 last-message)))
      (with-temp-buffer
        (insert-file-contents in)
        (org-mode)
        (switch-to-buffer (current-buffer))
        (goto-char (1+ point))
        (let* ((buffer (current-buffer))
               (result
                (condition-case err
                    (progn
                      (pcase command
                        ("set" (call-interactively #'org-set-property))
                        ("delete" (call-interactively #'org-delete-property))
                        ("delete-globally" (call-interactively #'org-delete-property-globally))
                        ("next" (call-interactively #'org-property-next-allowed-value))
                        ("previous" (call-interactively #'org-property-previous-allowed-value))
                        ("effort" (call-interactively #'org-set-effort))
                        ("inc-effort" (call-interactively #'org-inc-effort))
                        ("action" (call-interactively #'org-property-action))
                        ("ordered" (call-interactively #'org-toggle-ordered-property)))
                      nil)
                  (error (error-message-string err))
                  (quit "Quit")))
               (text (with-current-buffer buffer (buffer-substring-no-properties (point-min) (point-max))))
               (at (with-current-buffer buffer (1- (point)))))
          (with-temp-file out
            (insert (json-encode
                     (append
                      (if result
                          (list (cons 'error result))
                        (list (cons 'text text) (cons 'point at)))
                      (list (cons 'prompts (vconcat (nreverse prompts)))
                            (cons 'message last-message)))))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lisp_inheritance(value):
    if value is None or value is False:
        return "nil"
    if value is True:
        return "t"
    if isinstance(value, list):
        return "'(" + " ".join(lisp_string(name) for name in value) + ")"
    return lisp_string(value)


def lisp_globals(value):
    if not value:
        return "nil"
    return "'(" + " ".join(f"({lisp_string(k)} . {lisp_string(v)})" for k, v in value.items()) + ")"


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


HANG = "Emacs did not finish"


def reference(cases, timeout=None):
    """Fill in `expected` for every case, a hundred at a time; one that
    keeps Emacs from finishing within `timeout` seconds gets
    `{"error": HANG}`, and those after it run again."""
    timeout = timeout or 60
    done = 0
    while done < len(cases):
        chunk = cases[done:done + 100]
        finished = run(chunk, timeout)
        done += finished
        if finished < len(chunk):
            cases[done]["expected"] = {"error": HANG}
            done += 1
    return cases


def run(cases, timeout):
    """Run `cases` in one Emacs; how many finished."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            options = case.get("options", {})
            answers = "'(" + " ".join(lisp_string(answer) for answer in case.get("answers", [])) + ")"
            key = f"?{case['key']}" if case.get("key") else "nil"
            forms.append(
                f"(org-lsp-properties-case {lisp_string(source)} {lisp_string(target)} {case['point']}"
                f" {lisp_string(case['command'])} {answers} {key}"
                f" {lisp_inheritance(options.get('org_use_property_inheritance'))}"
                f" {lisp_globals(options.get('org_global_properties'))})"
            )
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        try:
            subprocess.run(
                ["emacs", "--batch", "-Q", *org_path, "-l", driver],
                check=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=timeout,
            )
        except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
            pass
        finished = 0
        for case, target in outputs:
            if not os.path.exists(target):
                break
            with open(target, encoding="utf-8") as handle:
                case["expected"] = json.load(handle)
            finished += 1
        return finished
