;;; conv.el --- load bridge.el's functions without its loop, and try conversions.
;; emacs --batch -Q -l conv.el --eval '(conv-try "org-latex-classes" "FEATURE" "[...]")'
(add-to-list 'load-path "/root/org-mode/lisp")
(with-temp-buffer
  (insert-file-contents "/home/user/org-lsp/server/org-lsp/src/bridge.el")
  (goto-char (point-max))
  (search-backward "(org-lsp-bridge-main)")
  (delete-region (point) (point-max))
  (eval-buffer))
;; Notices printed instead of sent.
(advice-add 'org-lsp-bridge--send :override (lambda (a) (message "SEND %S" a)))
(require 'cl-lib) (require 'seq) (require 'wid-edit)
(defun conv-try (name feature json)
  (require (intern feature))
  (let* ((symbol (intern name))
         (type (get symbol 'custom-type))
         (value (let ((json-object-type 'alist) (json-array-type 'vector) (json-key-type 'string)
                      (json-false :false) (json-null nil))
                  (json-read-from-string json)))
         (lisp (org-lsp-bridge--option-value type value)))
    (message "%s %s\n  -> %S\n  matches: %S" name json lisp (org-lsp-bridge--type-matches type lisp))))
