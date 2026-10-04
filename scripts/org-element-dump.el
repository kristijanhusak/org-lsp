;;; org-element-dump.el --- Dump org-element types and ranges  -*- lexical-binding: t; -*-

;; Usage: emacs --batch -Q -l scripts/org-element-dump.el FILE.org...
;;
;; Prints one line per element and object found by `org-element-parse-buffer':
;;
;;   TYPE BEGIN END POST-BLANK CONTENTS-BEGIN CONTENTS-END
;;
;; Positions are 0-based character offsets; a missing contents range is -1.
;; The parser tests compare this reference output with org-syntax.

(require 'org)
(require 'org-element)

(defun org-lsp-dump (file)
  "Print the dump of FILE to standard output."
  (with-temp-buffer
    (insert-file-contents file)
    (let ((org-inhibit-startup t)
          (org-element-use-cache nil))
      (org-mode)
      (let ((tree (org-element-parse-buffer))
            (rows nil))
        (org-element-map tree (append org-element-all-elements org-element-all-objects)
          (lambda (el)
            (let ((cb (org-element-property :contents-begin el))
                  (ce (org-element-property :contents-end el)))
              (push (format "%s %d %d %d %d %d"
                            (org-element-type el)
                            (1- (org-element-property :begin el))
                            (1- (org-element-property :end el))
                            (or (org-element-property :post-blank el) 0)
                            (if cb (1- cb) -1)
                            (if ce (1- ce) -1))
                    rows))
            nil))
        (dolist (row (nreverse rows))
          (princ row)
          (terpri))))))

;; With one file, print to standard output. With several, write FILE.elements
;; next to each one; a file Org itself cannot parse gets no dump.
(if (= 1 (length command-line-args-left))
    (org-lsp-dump (car command-line-args-left))
  (dolist (file command-line-args-left)
    (condition-case nil
        (let ((dump (with-output-to-string (org-lsp-dump file))))
          (with-temp-file (concat (file-name-sans-extension file) ".elements")
            (insert dump)))
      (error nil))))
