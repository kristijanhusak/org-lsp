;;; parse_page.el --- check docs/emacs-options.org as Org sees it.
(add-to-list 'load-path "/root/org-mode/lisp")
(require 'org) (require 'org-element)
(with-temp-buffer
  (insert-file-contents "/home/user/org-lsp/docs/emacs-options.org")
  (org-mode)
  (let ((tree (org-element-parse-buffer)) (levels (make-hash-table)) (h3 nil) (stray nil) (bad-blocks 0) (kw 0))
    (org-element-map tree 'headline
      (lambda (h)
        (let ((l (org-element-property :level h)) (title (org-element-property :raw-value h)))
          (puthash l (1+ (gethash l levels 0)) levels)
          (when (= l 3) (push title h3)
                (unless (equal (org-element-property :CUSTOM_ID h) (downcase (replace-regexp-in-string "[^[:alnum:] _-]" "" title)))
                  (princ (format "CUSTOM_ID? %s %s\n" title (org-element-property :CUSTOM_ID h)))))
          (when (> l 3) (push (format "L%d %s" l title) stray)))))
    ;; Lines with #+begin_/#+end_ that are not blocks (unbalanced) end up in paragraphs.
    (org-element-map tree '(paragraph)
      (lambda (p)
        (let ((s (buffer-substring-no-properties (org-element-property :contents-begin p) (org-element-property :contents-end p))))
          (when (string-match-p "^[ \t]*#\\+\\(begin\\|end\\)_" s) (cl-incf bad-blocks) (princ (format "UNBALANCED near line %d\n" (line-number-at-pos (org-element-property :begin p))))))))
    (org-element-map tree 'keyword (lambda (k) (cl-incf kw) (unless (member (org-element-property :key k) '("TITLE" "OPTIONS")) (princ (format "KEYWORD %s at %d\n" (org-element-property :key k) (line-number-at-pos (org-element-property :begin k)))))))
    (maphash (lambda (k v) (princ (format "level %d: %d\n" k v))) levels)
    (princ (format "stray deeper headlines: %S\nbad blocks %d\n" (reverse stray) bad-blocks))
    (with-temp-file "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/emacs/h3.txt" (insert (string-join (reverse h3) "\n")))))
