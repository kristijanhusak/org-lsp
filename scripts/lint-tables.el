;;; What org-lint reads from Emacs and Org: the header arguments of Babel
;;; languages, the export options and the default properties, as JSON.
;;; Run by scripts/update-lint-tables.py.
(require 'org)
(require 'org-lint)
(require 'json)
(org-babel-do-load-languages
 'org-babel-load-languages
 (mapcar (lambda (language) (cons language t))
         '(C R awk calc clojure css ditaa dot emacs-lisp eshell forth fortran gnuplot groovy
           haskell java js julia latex lilypond lisp lua makefile matlab maxima ocaml octave org
           perl plantuml processing python ruby sass scheme screen sed shell sql sqlite)))
(defun dump-value (value)
  (cond ((eq value :any) ":any")
        ((null value) nil)
        ((listp value) (vconcat (mapcar (lambda (group) (vconcat (mapcar (lambda (v) (format "%s" v)) group))) value)))
        (t (format "%s" value))))
(defun dump-args (alist)
  (vconcat (mapcar (lambda (entry) (vector (symbol-name (car entry)) (dump-value (cdr entry)))) alist)))
(let* ((languages (mapcar (lambda (l) (symbol-name (car l))) org-babel-load-languages))
       (per-language
        (delq nil
              (mapcar (lambda (lang)
                        (let ((v (intern (format "org-babel-header-args:%s" lang))))
                          (and (boundp v) (cons lang (dump-args (symbol-value v))))))
                      (append languages '("C++" "D" "cpp" "elisp" "sh" "bash" "zsh" "fish" "csh" "ash" "dash" "ksh" "mksh" "posh" "cmd" "powershell")))))
       (allowed (delq nil
		      (append
		       (mapcar (lambda (o) (nth 2 o)) org-export-options-alist)
		       (cl-mapcan
			(lambda (b)
			  (mapcar (lambda (o) (nth 2 o))
				  (org-export-backend-options b)))
			org-export-registered-backends))))
       common-options options-alist)
  (dolist (opt org-export-options-alist)
    (when (stringp (nth 1 opt))
      (cl-pushnew (nth 1 opt) common-options :test #'equal)))
  (dolist (backend org-export-registered-backends)
    (dolist (opt (org-export-backend-options backend))
      (when (stringp (nth 1 opt))
        (cl-pushnew (or (org-export-backend-name backend) 'anonymous)
                    (alist-get (nth 1 opt) options-alist nil nil #'equal)))))
  (setq options-alist (nreverse options-alist))
  (princ
   (json-encode
    `((common . ,(dump-args org-babel-common-header-args-w-values))
      (languages . ,(vconcat languages))
      (header_args . ,(mapcar (lambda (entry) (cons (car entry) (cdr entry))) per-language))
      (options_items . ,(vconcat (delete-dups allowed)))
      (common_options . ,(vconcat (reverse common-options)))
      (backend_options . ,(vconcat (mapcar (lambda (entry) (vector (car entry) (vconcat (mapcar #'symbol-name (cdr entry))))) options-alist)))
      (default_properties . ,(vconcat org-default-properties))
      (special_properties . ,(vconcat org-special-properties))))))
