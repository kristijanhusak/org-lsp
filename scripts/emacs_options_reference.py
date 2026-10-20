"""Record what Emacs says of each of Org's options that `emacs_options`
sets: those marked `emacs` in scripts/org-options.json. For each, Emacs
loads the library it is in (the file it is listed under) and writes down

    {
      "doc": "...",          its docstring, `substitute-command-keys`
                             applied with `foo' quoting kept
      "default": "...",      its standard value as `pp` prints it
      "form": "...",         or, in its place, the form Org computes it
                             with, as the library writes it, when its
                             value depends on where Org loads (a
                             program found, a path, the version of Emacs)
      "type": "...",         its `custom-type`, as `pp` prints it
      "version": "Org 9.6",  `:package-version`, or `custom-version`
                             ("Emacs 24.1"), when it has one
      "json": ...,           the standard value as `emacs_options` takes
                             it, when there is such a value: the bridge
                             makes it the same value again
      "schema": {...}        a JSON Schema of what it takes, when its
                             type is simple (true or false, a number, a
                             string, one of a few constants)
    }

or `{"error": "..."}` when its library does not load or it is no option.
`json` is checked with the bridge's own conversion
(server/org-lsp/src/bridge.el): a value that does not come back equal to
the default, or that its type does not match, is left out. Printed values
longer than LONGEST characters are cut at a line, with `…`. The result is
scripts/emacs-options.json, by file, in the order of
scripts/org-options.json; scripts/generate-docs.mjs writes
docs/emacs-options.org from it and fails when its names are not those of
org-options.json. The server's schema of `emacs_options` (what VS Code
completes) is made from it too.

    ORG_LISP_DIR=~/src/org-mode/lisp python3 scripts/emacs_options_reference.py

Set ORG_LISP_DIR to the `lisp` directory of an Org 9.8.10 checkout: the
Org that Emacs ships with has other options and docstrings.
"""

import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OPTIONS = os.path.join(ROOT, "scripts", "org-options.json")
BRIDGE = os.path.join(ROOT, "server", "org-lsp", "src", "bridge.el")
OUTPUT = os.path.join(ROOT, "scripts", "emacs-options.json")

# Longer printed values are cut there, with `…`.
LONGEST = 1500

DRIVER = r"""
(require 'json)
(require 'cl-lib)
(require 'wid-edit)
(require 'pp)
(require 'subr-x)
(setq-default indent-tabs-mode nil)
(setq text-quoting-style 'grave
      print-escape-newlines t
      print-length nil
      print-level nil)

(defun org-lsp-options-printed (value)
  "VALUE as `pp' prints it, the lines past `org-lsp-options-longest'
characters cut off."
  (let ((lines (split-string (string-trim-right (pp-to-string value)) "\n"))
        (kept nil)
        (length 0))
    (while (and lines (<= (+ length (length (car lines))) org-lsp-options-longest))
      (setq length (+ length 1 (length (car lines))))
      (push (pop lines) kept))
    (cond ((null lines) (string-join (nreverse kept) "\n"))
          (kept (concat (string-join (nreverse kept) "\n") "\n…"))
          (t (concat (substring (car lines) 0 org-lsp-options-longest) "…")))))

(defun org-lsp-options-form (form)
  "The form of `defcustom' in FORM, which Emacs 29 keeps in a closure:
`(funcall #'(lambda () \"\" FORM))'."
  (let ((fn (and (eq (car-safe form) 'funcall)
                 (memq (car-safe (cadr form)) '(function quote))
                 (cadr (cadr form)))))
    (pcase fn
      (`(closure ,_ nil ,(pred stringp) ,body) body)
      (`(lambda nil ,(pred stringp) ,body) body)
      (_ form))))

(defun org-lsp-options-source (symbol)
  "The text of SYMBOL's default in the `defcustom' of its library, as Org
writes it, or nil."
  (let ((file (symbol-file symbol 'defvar)))
    (when file
      (with-temp-buffer
        (insert-file-contents (concat (file-name-sans-extension file) ".el"))
        (emacs-lisp-mode)
        (untabify (point-min) (point-max))
        (when (re-search-forward (concat "^(defcustom[ \t\n]+" (regexp-quote (symbol-name symbol)) "\\_>") nil t)
          (skip-chars-forward " \t\n")
          (let* ((start (point))
                 (column (current-column))
                 (text (progn (forward-sexp) (buffer-substring-no-properties start (point)))))
            ;; The lines after the first, as far left as the first.
            (replace-regexp-in-string (concat "\n" (make-string column ?\s)) "\n" text)))))))

(defconst org-lsp-options-pure
  '(quote function list cons append concat regexp-opt format mapcar lambda
    car cdr nth * + - and or)
  "The functions a form whose value is the same wherever Org loads may call.")

(defun org-lsp-options-pure-p (form)
  "Whether FORM's value is the same wherever Org loads: a constant, Org's
own variables, and only the functions of `org-lsp-options-pure'."
  (cond ((memq form '(nil t)) t)
        ((keywordp form) t)
        ((symbolp form) (string-prefix-p "org-" (symbol-name form)))
        ((consp form)
         (and (memq (car form) org-lsp-options-pure)
              (or (memq (car form) '(quote function))
                  (cl-every #'org-lsp-options-pure-p (cdr form)))))
        (t t)))

(defun org-lsp-options-list-type-p (type)
  "Whether the custom TYPE is a list of things, whose nil is []."
  (let ((head (if (consp type) (car type) type)))
    (or (memq head '(repeat set alist plist))
        (and (memq head '(choice radio))
             (cl-some #'org-lsp-options-list-type-p (org-lsp-bridge--type-args type))))))

(defun org-lsp-options-atom (value)
  "VALUE, an atom, as JSON; `none' when it has no JSON."
  (cond ((eq value t) t)
        ((null value) :json-false)
        ((numberp value) value)
        ((stringp value) value)
        ((keywordp value) 'none)
        ((symbolp value) (symbol-name value))
        (t 'none)))

(defun org-lsp-options-json (value type)
  "VALUE as JSON `emacs_options' could hold, or `none'."
  (cond ((and (null value) (org-lsp-options-list-type-p type)) [])
        ((atom value) (org-lsp-options-atom value))
        ((not (proper-list-p value)) 'none)
        ;; An association list of names and atoms: a table.
        ((cl-every (lambda (item) (and (consp item) (symbolp (car item)) (not (keywordp (car item)))
                                       (atom (cdr item)) (not (eq (org-lsp-options-atom (cdr item)) 'none))))
                   value)
         (if (cl-some (lambda (item) (eq (cdr item) nil)) value)
             'none
           (mapcar (lambda (item) (cons (symbol-name (car item)) (org-lsp-options-atom (cdr item)))) value)))
        (t (let ((items (mapcar #'org-lsp-options-atom value)))
             (if (memq 'none items) 'none (vconcat items))))))

(defun org-lsp-options-round-trip (symbol value type)
  "The JSON text of VALUE when the bridge makes it VALUE again, or nil."
  (let ((json (org-lsp-options-json value type)))
    (unless (eq json 'none)
      (let* ((text (json-encode json))
             (read (let ((json-object-type 'alist)
                         (json-array-type 'vector)
                         (json-key-type 'string)
                         (json-false :false)
                         (json-null nil))
                     (json-read-from-string text)))
             (lisp (condition-case nil (org-lsp-bridge--option-value type read) (error 'none))))
        (when (and (equal lisp value) (org-lsp-bridge--type-matches type lisp))
          text)))))

(defun org-lsp-options-kinds (type)
  "What the custom TYPE takes, when it is simple: (VALUES . TYPES), the
JSON values it takes and the JSON types; or nil."
  (let ((head (if (consp type) (car type) type))
        (keywords (org-lsp-bridge--type-keywords type))
        (args (org-lsp-bridge--type-args type)))
    (pcase head
      ('boolean '(nil "boolean"))
      ((or 'integer 'natnum) '(nil "integer"))
      ((or 'number 'float) '(nil "number"))
      ((or 'string 'regexp 'file 'directory 'color) '(nil "string"))
      ;; A name, which may be t or nil too.
      ((or 'symbol 'coding-system) '((t :json-false) "string"))
      ('const
       (let* ((constant (if (plist-member keywords :value) (plist-get keywords :value) (car args)))
              (json (org-lsp-options-atom constant)))
         (and (not (eq json 'none)) (list (list json)))))
      ((or 'choice 'radio)
       (let ((parts (mapcar #'org-lsp-options-kinds args)))
         (unless (memq nil parts)
           (cons (cl-remove-duplicates (apply #'append (mapcar #'car parts)) :test #'equal :from-end t)
                 (cl-remove-duplicates (apply #'append (mapcar #'cdr parts)) :test #'equal :from-end t)))))
      (_ nil))))

(defun org-lsp-options-simple (type)
  "A JSON Schema of the custom TYPE, when it is simple; or nil."
  (let* ((kinds (org-lsp-options-kinds type))
         (types (cdr kinds))
         ;; The values its types take anyway go; strings stay, for
         ;; completion.
         (values (cl-remove-if (lambda (value)
                                 (or (and (memq value '(t :json-false)) (member "boolean" types))
                                     (and (numberp value) (member "number" types))
                                     (and (integerp value) (member "integer" types))))
                               (car kinds)))
         (typed (and types `((type . ,(if (cdr types) (vconcat types) (car types)))))))
    (cond ((null kinds) nil)
          ((null values) typed)
          ((null types) `((enum . ,(vconcat values))))
          (t `((anyOf . ,(vector `((enum . ,(vconcat values))) typed)))))))

(defun org-lsp-options-version (symbol)
  (let ((package (get symbol 'custom-package-version))
        (emacs (get symbol 'custom-version)))
    (cond (package (format "%s %s" (car package) (cdr package)))
          (emacs (format "Emacs %s" emacs)))))

(defun org-lsp-options-record (name)
  (let ((symbol (intern name)))
    (condition-case err
        (if (not (get symbol 'standard-value))
            `((error . ,(format "%s is not an option" name)))
          (let* ((raw (documentation-property symbol 'variable-documentation t))
                 (doc (and raw (substitute-command-keys raw)))
                 (standard (car (get symbol 'standard-value)))
                 (form (org-lsp-options-form standard))
                 (value (condition-case err (eval standard t) (error (list 'org-lsp-options-error err))))
                 (failed (eq (car-safe value) 'org-lsp-options-error))
                 (type (get symbol 'custom-type))
                 (pure (org-lsp-options-pure-p form))
                 (json (and (not failed) pure
                            (org-lsp-options-round-trip symbol value type)))
                 (schema (org-lsp-options-simple type))
                 (version (org-lsp-options-version symbol)))
            (delq nil
                  (list (cons 'doc (or doc ""))
                        ;; What depends on where Org loads (a program
                        ;; found, a path) is the form it is computed with.
                        (if (and pure (not failed))
                            `(default . ,(org-lsp-options-printed value))
                          `(form . ,(or (org-lsp-options-source symbol) (org-lsp-options-printed form))))
                        (cons 'type (org-lsp-options-printed type))
                        (and version (cons 'version version))
                        (and json (cons 'json (intern json)))
                        (and schema (cons 'schema schema))))))
      (error `((error . ,(error-message-string err)))))))

(defun org-lsp-options-library (feature names out)
  "Load FEATURE and record each of NAMES into OUT."
  (let ((loaded (condition-case err (progn (require (intern feature)) nil)
                  (error (error-message-string err)))))
    (with-temp-file out
      (insert
       (json-encode
        (mapcar (lambda (name)
                  (cons name (if loaded
                                 `((error . ,(format "%s.el does not load: %s" feature loaded)))
                               (org-lsp-options-record name))))
                names))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def emacs_options():
    """The options marked `emacs`, by file, in the order of org-options.json."""
    with open(OPTIONS, encoding="utf-8") as handle:
        files = json.load(handle)
    found = {}
    for file, options in files.items():
        names = [name for name, value in options.items() if (value[0] if isinstance(value, list) else value) == "emacs"]
        if names:
            found[file] = names
    return found


def bridge():
    """The Lisp of the bridge, without the loop it ends by starting."""
    with open(BRIDGE, encoding="utf-8") as handle:
        text = handle.read()
    return text.replace("\n(org-lsp-bridge-main)", "\n")


def reference():
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    files = emacs_options()
    records = {}
    with tempfile.TemporaryDirectory() as directory:
        bridge_file = os.path.join(directory, "bridge.el")
        with open(bridge_file, "w", encoding="utf-8") as handle:
            handle.write(bridge())
        # One Emacs per file: a library that loads others does not change
        # what the next one finds.
        for file, names in sorted(files.items()):
            feature = file.removesuffix(".el")
            out = os.path.join(directory, f"{feature}.json")
            driver = os.path.join(directory, f"{feature}.el")
            with open(driver, "w", encoding="utf-8") as handle:
                handle.write(f"(defvar org-lsp-options-longest {LONGEST})\n")
                handle.write(DRIVER)
                handle.write(f"(org-lsp-options-library {lisp_string(feature)} '({' '.join(lisp_string(n) for n in names)}) {lisp_string(out)})\n")
            env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC", HOME=directory)
            result = subprocess.run(
                ["emacs", "--batch", "-Q", *org_path, "-l", bridge_file, "-l", driver],
                env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                text=True, timeout=300,
            )
            if result.returncode != 0 or not os.path.exists(out):
                error = (result.stderr.strip().splitlines() or ["Emacs failed"])[-1]
                records[file] = {name: {"error": error} for name in names}
                continue
            with open(out, encoding="utf-8") as handle:
                recorded = json.load(handle)
            records[file] = {}
            for name in names:
                entry = recorded[name]
                # Emacs wrote the value as its JSON text.
                if "json" in entry:
                    entry["json"] = json.loads(entry["json"])
                records[file][name] = entry
    return records


ORDER = ["error", "doc", "default", "form", "type", "version", "json", "schema"]


def save(records):
    ordered = {
        file: {name: {key: entry[key] for key in ORDER if key in entry} for name, entry in options.items()}
        for file, options in records.items()
    }
    with open(OUTPUT, "w", encoding="utf-8") as handle:
        json.dump(ordered, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


if __name__ == "__main__":
    records = reference()
    save(records)
    errors = [(name, entry["error"]) for options in records.values() for name, entry in options.items() if "error" in entry]
    count = sum(len(options) for options in records.values())
    print(f"recorded {count} options in {os.path.relpath(OUTPUT, ROOT)}")
    for name, error in errors:
        print(f"  {name}: {error}", file=sys.stderr)
    sys.exit(1 if errors else 0)
