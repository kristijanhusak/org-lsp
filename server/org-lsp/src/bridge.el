;;; bridge.el --- org-lsp's link to Emacs  -*- lexical-binding: t -*-

;; The server runs `emacs --batch -Q -l bridge.el` when a command needs
;; Org itself (export). Requests come one a line on standard input:
;;
;;   {"id": 1, "method": "export-menu", "params": {...}}
;;
;; and each answer is one line on standard output, after a marker that
;; tells it from anything else Emacs prints:
;;
;;   \x1eorg-lsp {"id": 1, "result": ...}   or   {"id": 1, "error": "..."}
;;
;; Both are JSON with every character beyond ASCII escaped, whatever
;; coding systems Emacs uses for its terminal. Emacs must not read
;; standard input on its own: a yes-or-no question goes to the server,
;;
;;   \x1eorg-lsp {"ask": {"prompt": "Evaluate this emacs-lisp code block? "}}
;;
;; whose answer comes as the next line, {"answer": true}; any other
;; question is an error. What the user should know, such as an option
;; of `emacs_options' that was not set, comes on a line of its own:
;;
;;   \x1eorg-lsp {"notice": "`emacs_options` leaves out ..."}

(require 'json)
(require 'subr-x)

(defvar org-lsp-bridge--reading nil
  "Non-nil while the bridge itself reads a request.")

(defun org-lsp-bridge--ascii (string)
  "STRING with every character beyond ASCII as a JSON escape."
  (replace-regexp-in-string
   "[^[:ascii:]]"
   (lambda (char)
     (let ((code (string-to-char char)))
       (if (< code #x10000)
           (format "\\u%04x" code)
         (let ((code (- code #x10000)))
           (format "\\u%04x\\u%04x"
                   (+ #xd800 (ash code -10))
                   (+ #xdc00 (logand code #x3ff)))))))
   string t t))

(defun org-lsp-bridge--send (answer)
  (princ (concat "\x1eorg-lsp " (org-lsp-bridge--ascii (json-encode answer)) "\n")))

(defun org-lsp-bridge--prompt-error (prompt)
  (error "Emacs asked a question org-lsp cannot answer: %s" (string-trim (format "%s" (or prompt "")))))

(defun org-lsp-bridge--read (old prompt &rest args)
  (if org-lsp-bridge--reading
      (let ((org-lsp-bridge--reading nil))
        (apply old prompt args))
    (org-lsp-bridge--prompt-error prompt)))

(dolist (fn '(read-from-minibuffer read-string completing-read read-passwd read-number))
  (advice-add fn :around #'org-lsp-bridge--read))
(defun org-lsp-bridge--ask (prompt &rest _)
  "Ask the editor PROMPT, a yes-or-no question, and wait for its answer."
  (org-lsp-bridge--send `((ask . ((prompt . ,(string-trim (substitute-command-keys (format "%s" prompt))))))))
  (let ((line (let ((org-lsp-bridge--reading t)) (read-from-minibuffer ""))))
    (eq (alist-get 'answer (json-read-from-string line)) t)))

(dolist (fn '(y-or-n-p yes-or-no-p))
  (advice-add fn :override #'org-lsp-bridge--ask))
(dolist (fn '(read-char-exclusive read-char read-key))
  (advice-add fn :override (lambda (&optional prompt &rest _) (org-lsp-bridge--prompt-error prompt))))

;;; Setting up

(defvar org-lsp-bridge--opened nil
  "The file `org-open-file' was asked to open during an export.")

(defun org-lsp-bridge-setup (params)
  "Load the user's configuration, then Org and its exporters."
  (let ((file (alist-get 'config_file params))
        (config (alist-get 'config params)))
    (setq enable-local-variables :safe
          create-lockfiles nil
          make-backup-files nil
          auto-save-default nil)
    (when (and file (not (string-empty-p file)))
      (load (expand-file-name file) nil t))
    (when (and config (not (string-empty-p config)))
      (with-temp-buffer
        (insert config)
        (eval-buffer)))
    ;; After the user's Lisp, so that these win over it.
    (org-lsp-bridge--set-options (append (alist-get 'options params) nil))
    (require 'org)
    (require 'ox)
    (require 'ox-publish)
    ;; Opening is the editor's: the file is reported instead.
    (advice-add 'org-open-file :override #'org-lsp-bridge--report-open)
    `((org . ,(org-version)) (emacs . ,emacs-version))))

;;; Org's options (`emacs_options')

;; Each option comes with the library it is in and its value as JSON,
;; which Emacs reads here: true is t, false nil, a list a list and a table
;; an alist with strings for keys. What the option's type (`custom-type')
;; wants decides the rest: a string where it wants a symbol is that
;; symbol, a table where it wants a list is a list of its pairs. The
;; value is set once the library is loaded, as Customize sets it (with
;; the option's `:set'), if the type matches it; if not, the user is
;; told.

(defun org-lsp-bridge--notice (format &rest args)
  "Tell the user what FORMAT and ARGS say."
  (org-lsp-bridge--send `((notice . ,(apply #'format format args)))))

(defun org-lsp-bridge--set-options (options)
  "Set each of OPTIONS once its library is loaded: by `name' to the value of
`json', after `feature' is."
  (dolist (option options)
    (let ((name (alist-get 'name option))
          (json (alist-get 'json option)))
      (with-eval-after-load (intern (alist-get 'feature option))
        ;; Whatever goes wrong, the library loads all the same.
        (condition-case err
            (org-lsp-bridge--set-option name json)
          (error (org-lsp-bridge--notice "`emacs_options` could not set `%s`: %s"
                                         name (error-message-string err))))))))

(defun org-lsp-bridge--set-option (name json)
  "Set the option NAME to JSON's value, as Customize would."
  (require 'cl-lib)
  (require 'seq)
  (require 'wid-edit)
  (let* ((symbol (intern name))
         (type (get symbol 'custom-type))
         (value (let ((json-object-type 'alist)
                      (json-array-type 'vector)
                      (json-key-type 'string)
                      (json-false :false)
                      (json-null nil))
                  (json-read-from-string json))))
    (if (not type)
        (org-lsp-bridge--notice "`emacs_options` leaves out `%s`: it is not an option of Org %s"
                                name (if (fboundp 'org-version) (org-version) ""))
      (let ((lisp (org-lsp-bridge--option-value type value)))
        (if (not (org-lsp-bridge--type-matches type lisp))
            (let ((fault (org-lsp-bridge--fault type value)))
              (org-lsp-bridge--notice "`emacs_options` leaves out `%s`: %s is not %s"
                                      name (org-lsp-bridge--json (car fault))
                                      (org-lsp-bridge--type-text (cdr fault))))
          (condition-case err
              (funcall (or (get symbol 'custom-set) #'set-default) symbol lisp)
            (error (org-lsp-bridge--notice "`emacs_options` could not set `%s`: %s"
                                           name (error-message-string err)))))))))

(defun org-lsp-bridge--type-matches (type value)
  "Whether VALUE is of the custom TYPE, as Customize checks it."
  (condition-case nil
      (widget-apply (widget-convert type) :match value)
    (error nil)))

(defun org-lsp-bridge--type-keywords (type)
  "The keywords of the custom TYPE (`:tag', `:key-type'…), as a plist."
  (let ((rest (and (consp type) (cdr type)))
        keywords)
    ;; A keyword with nothing after it is an argument: the value of
    ;; `(const :utf-8)', as `widget-convert' reads it.
    (while (and (keywordp (car rest)) (cdr rest))
      (setq keywords (append keywords (list (car rest) (cadr rest)))
            rest (cddr rest)))
    keywords))

(defun org-lsp-bridge--type-args (type)
  "The arguments of the custom TYPE after its keywords."
  (let ((rest (and (consp type) (cdr type))))
    (while (and (keywordp (car rest)) (cdr rest))
      (setq rest (cddr rest)))
    rest))

(defun org-lsp-bridge--plain-value (value)
  "VALUE as JSON gave it, made Lisp without a type to say more: false is
nil, a list a list, a table an alist."
  (cond ((eq value :false) nil)
        ((vectorp value) (mapcar #'org-lsp-bridge--plain-value value))
        ((consp value)
         (mapcar (lambda (pair) (cons (car pair) (org-lsp-bridge--plain-value (cdr pair)))) value))
        (t value)))

(defun org-lsp-bridge--pairs (value)
  "The pairs of the table VALUE as lists of two, for a type that wants a
list of them; any other VALUE as it is."
  (if (consp value)
      (vconcat (mapcar (lambda (pair) (vector (car pair) (cdr pair))) value))
    value))

(defun org-lsp-bridge--names-p (type value)
  "Whether the custom TYPE names VALUE: a constant whose name VALUE is,
or a list whose first item is such a constant, as `(not \"LOGBOOK\")'
is for `(list (const not) …)'."
  (let ((head (if (consp type) (car type) type))
        (keywords (org-lsp-bridge--type-keywords type))
        (args (org-lsp-bridge--type-args type)))
    (pcase head
      ((or 'const 'other)
       (let ((constant (if (plist-member keywords :value) (plist-get keywords :value) (car args))))
         (and (stringp value) constant (symbolp constant) (not (eq constant t))
              (string= value (symbol-name constant)))))
      ((or 'choice 'radio)
       (seq-some (lambda (type) (org-lsp-bridge--names-p type value)) args))
      ((or 'list 'group 'vector 'cons)
       (and (vectorp value) (> (length value) 0) args
            (memq (if (consp (car args)) (caar args) (car args)) '(const other))
            (org-lsp-bridge--names-p (car args) (aref value 0)))))))

(defun org-lsp-bridge--string-type-p (type)
  "Whether the custom TYPE takes strings."
  (let ((head (if (consp type) (car type) type)))
    (or (memq head '(string regexp file directory color))
        (and (memq head '(choice radio))
             (seq-some #'org-lsp-bridge--string-type-p (org-lsp-bridge--type-args type))))))

(defun org-lsp-bridge--inline-p (type)
  "Whether the custom TYPE, an argument of a list, stands for several of
its items (`(repeat :inline t …)')."
  (and (consp type) (memq (car type) '(repeat set))
       (plist-get (org-lsp-bridge--type-keywords type) :inline)))

(defun org-lsp-bridge--list-items (args items)
  "ITEMS, of a list, as the arguments ARGS of its custom type want them:
one item each, and the rest for an inline repeat (`org-latex-classes''s
sectioning). The symbol `none' when the count does not fit."
  (let ((inline (seq-find #'org-lsp-bridge--inline-p args)))
    (cond
     ((and (not inline) (= (length items) (length args)))
      (cl-mapcar #'org-lsp-bridge--option-value args items))
     ((and inline (>= (length items) (1- (length args))))
      (let* ((before (seq-take-while (lambda (type) (not (eq type inline))) args))
             (after (cdr (memq inline args)))
             (middle (- (length items) (length before) (length after))))
        (append (cl-mapcar #'org-lsp-bridge--option-value before items)
                (org-lsp-bridge--option-value inline (vconcat (seq-subseq items (length before)
                                                                          (+ (length before) middle))))
                (cl-mapcar #'org-lsp-bridge--option-value after (nthcdr (+ (length before) middle) items)))))
     (t 'none))))

(defun org-lsp-bridge--option-value (type value)
  "VALUE, read from JSON, as the custom TYPE wants it."
  (let* ((head (if (consp type) (car type) type))
         (keywords (org-lsp-bridge--type-keywords type))
         (args (org-lsp-bridge--type-args type)))
    (pcase head
      ((or 'const 'other)
       (let ((constant (if (plist-member keywords :value) (plist-get keywords :value) (car args))))
         (if (and (stringp value) (symbolp constant) (string= value (symbol-name constant)))
             constant
           (org-lsp-bridge--plain-value value))))
      ((or 'symbol 'variable 'face 'coding-system)
       (if (stringp value) (intern value) (org-lsp-bridge--plain-value value)))
      ('character
       (if (and (stringp value) (= (length value) 1)) (aref value 0) value))
      ('float (if (integerp value) (float value) value))
      ((or 'choice 'radio)
       ;; Where Customize's user picks the alternative, the value alone
       ;; says it here: first an alternative that names the value (a
       ;; constant `auto' for "auto", a list that starts with the
       ;; constant `not' for ["not", …]), then for a string one that
       ;; takes strings (unless it is written as a keyword, `:with-toc',
       ;; Org's way to name a property), then the first the value fits.
       (let ((found nil) (done nil))
         (dolist (fits (list (lambda (type) (org-lsp-bridge--names-p type value))
                             (lambda (type) (and (stringp value) (not (string-prefix-p ":" value))
                                                 (org-lsp-bridge--string-type-p type)))
                             (lambda (_) t)))
           (let ((rest args))
             (while (and rest (not done))
               (when (funcall fits (car rest))
                 (let ((lisp (org-lsp-bridge--option-value (car rest) value)))
                   (when (org-lsp-bridge--type-matches (car rest) lisp)
                     (setq found lisp done t))))
               (setq rest (cdr rest)))))
         (if done found (org-lsp-bridge--plain-value value))))
      ((or 'repeat 'set)
       (let ((value (org-lsp-bridge--pairs value)))
         (if (not (vectorp value))
             (org-lsp-bridge--plain-value value)
           (mapcar (lambda (item)
                     (org-lsp-bridge--option-value
                      (if (eq head 'repeat) (car args) `(choice ,@args))
                      item))
                   value))))
      ((or 'list 'group 'vector)
       (let ((value (org-lsp-bridge--pairs value)))
         (if (not (vectorp value))
             (org-lsp-bridge--plain-value value)
           (let ((items (org-lsp-bridge--list-items args (append value nil))))
             (cond ((eq items 'none) (org-lsp-bridge--plain-value value))
                   ((eq head 'vector) (vconcat items))
                   (t items))))))
      ((or 'function 'command)
       ;; A function's name, as Customize takes it: one defined by then,
       ;; in `emacs_config' or a library.
       (let ((symbol (and (stringp value) (intern-soft value))))
         (if (and symbol (fboundp symbol)) symbol (org-lsp-bridge--plain-value value))))
      ('cons
       (if (and (vectorp value) (= (length value) 2))
           (cons (org-lsp-bridge--option-value (nth 0 args) (aref value 0))
                 (org-lsp-bridge--option-value (nth 1 args) (aref value 1)))
         (org-lsp-bridge--plain-value value)))
      ((or 'alist 'plist)
       (let ((key-type (or (plist-get keywords :key-type) (if (eq head 'plist) 'symbol 'sexp)))
             (value-type (or (plist-get keywords :value-type) 'sexp))
             (pairs (cond ((consp value) (mapcar (lambda (pair) (vector (car pair) (cdr pair))) value))
                          ((vectorp value) (append value nil))
                          (t 'none))))
         (if (or (eq pairs 'none) (not (seq-every-p (lambda (pair) (and (vectorp pair) (= (length pair) 2))) pairs)))
             (org-lsp-bridge--plain-value value)
           (let ((key (lambda (key)
                        ;; A property list's keys are keywords: `scale' is
                        ;; `:scale', for a symbol as for a choice of
                        ;; keywords.
                        (if (and (eq head 'plist) (stringp key) (not (string-prefix-p ":" key)))
                            (let ((keyword (org-lsp-bridge--option-value key-type (concat ":" key))))
                              (if (org-lsp-bridge--type-matches key-type keyword)
                                  keyword
                                (org-lsp-bridge--option-value key-type key)))
                          (org-lsp-bridge--option-value key-type key)))))
             (if (eq head 'plist)
                 (mapcan (lambda (pair)
                           (list (funcall key (aref pair 0))
                                 (org-lsp-bridge--option-value value-type (aref pair 1))))
                         pairs)
               (mapcar (lambda (pair)
                         (cons (funcall key (aref pair 0))
                               (org-lsp-bridge--option-value value-type (aref pair 1))))
                       pairs))))))
      (_
       ;; A type of its own (`define-widget'), which may stand for another.
       (let ((other (and (symbolp head) (get head 'widget-type)
                         (condition-case nil (widget-get (widget-convert type) :type) (error nil)))))
         (if other
             (org-lsp-bridge--option-value other value)
           (org-lsp-bridge--plain-value value)))))))

(defun org-lsp-bridge--fault (type value)
  "The part of VALUE, read from JSON, that the custom TYPE does not take,
with the type it should have been: an item of a list, a key or a value of
a table, or VALUE itself."
  (let* ((head (if (consp type) (car type) type))
         (keywords (org-lsp-bridge--type-keywords type))
         (args (org-lsp-bridge--type-args type))
         (inner
          (unless (org-lsp-bridge--type-matches type (org-lsp-bridge--option-value type value))
            (pcase head
              ((or 'repeat 'set)
               (let ((items (org-lsp-bridge--pairs value)))
                 (and (vectorp items)
                      (seq-some (lambda (item)
                                  (org-lsp-bridge--fault
                                   (if (eq head 'repeat) (car args) `(choice ,@args)) item))
                                items))))
              ((or 'list 'group 'vector 'cons)
               (let ((items (org-lsp-bridge--pairs value)))
                 (and (vectorp items) (= (length items) (length args))
                      (not (seq-some #'org-lsp-bridge--inline-p args))
                      (seq-some #'identity (cl-mapcar #'org-lsp-bridge--fault args (append items nil))))))
              ((or 'alist 'plist)
               (and (consp value)
                    (let ((key-type (or (plist-get keywords :key-type) (if (eq head 'plist) 'symbol 'sexp)))
                          (value-type (or (plist-get keywords :value-type) 'sexp)))
                      (seq-some (lambda (pair)
                                  (or (and (not (eq head 'plist))
                                           (org-lsp-bridge--fault key-type (car pair)))
                                      (org-lsp-bridge--fault value-type (cdr pair))))
                                value))))))))
    (cond ((org-lsp-bridge--type-matches type (org-lsp-bridge--option-value type value)) nil)
          (inner)
          (t (cons value type)))))

(defun org-lsp-bridge--json (value)
  "VALUE, read from JSON, as JSON again."
  (let ((json-false :false) (json-null nil))
    (json-encode value)))

(defun org-lsp-bridge--json-text (value)
  "VALUE, a constant of a custom type, as it is written in the options."
  (cond ((eq value t) "true")
        ((null value) "false")
        ((symbolp value) (json-encode-string (symbol-name value)))
        ((stringp value) (json-encode-string value))
        (t (format "%S" value))))

(defun org-lsp-bridge--type-text (type)
  "What the custom TYPE takes, in a few words."
  (let* ((head (if (consp type) (car type) type))
         (keywords (org-lsp-bridge--type-keywords type))
         (args (org-lsp-bridge--type-args type)))
    (pcase head
      ((or 'const 'other)
       (org-lsp-bridge--json-text (if (plist-member keywords :value) (plist-get keywords :value) (car args))))
      ((or 'choice 'radio)
       (let ((texts (delete-dups (mapcar #'org-lsp-bridge--type-text args))))
         (if (cdr texts)
             (concat (string-join (butlast texts) ", ") " or " (car (last texts)))
           (car texts))))
      ('boolean "true or false")
      ((or 'string 'regexp 'file 'directory 'color 'key) "a string")
      ((or 'integer 'natnum) "an integer")
      ((or 'number 'float) "a number")
      ((or 'symbol 'variable 'face 'coding-system) "a name")
      ('character "a character")
      ((or 'function 'command) "the name of a defined function")
      ((or 'repeat 'set) "a list")
      ((or 'list 'group 'vector 'cons) (format "a list of %d" (if (eq head 'cons) 2 (length args))))
      ((or 'alist 'plist) "a table")
      (_ "what Customize allows"))))

;;; Export

(defun org-lsp-bridge--entries ()
  "The menu entries of the registered backends, as `org-export-dispatch'
sorts them: (KEY LABEL SUB-MENU), several of them with the same KEY when a
backend adds to another's menu."
  (sort (sort (delq nil (mapcar #'org-export-backend-menu org-export-registered-backends))
              (lambda (a b)
                (let ((key-a (nth 1 a))
                      (key-b (nth 1 b)))
                  (cond ((and (numberp key-a) (numberp key-b)) (< key-a key-b))
                        ((numberp key-b) t)))))
        #'car-less-than-car))

(defun org-lsp-bridge-export-menu (_params)
  "The dispatcher's menu: each first key with its label and its entries."
  (let (menu)
    (dolist (entry (org-lsp-bridge--entries))
      (let* ((key (char-to-string (car entry)))
             (top (assoc key menu))
             (sub (nth 2 entry)))
        (unless top
          (setq top (list key (nth 1 entry) nil))
          (push top menu))
        (if (functionp sub)
            (setf (nth 2 top) 'direct)
          (dolist (item sub)
            (setf (nth 2 top)
                  (append (nth 2 top)
                          (list `((key . ,(char-to-string (car item)))
                                  (label . ,(nth 1 item))))))))))
    (vconcat
     (mapcar (lambda (top)
               `((key . ,(nth 0 top))
                 (label . ,(nth 1 top))
                 (entries . ,(if (eq (nth 2 top) 'direct) :json-false (vconcat (nth 2 top))))))
             (nreverse menu)))))

(defun org-lsp-bridge--action (first second)
  "The function of the menu entry FIRST (and SECOND within it)."
  (let (found)
    (dolist (entry (org-lsp-bridge--entries))
      (when (and (not found) (eq (car entry) first))
        (let ((sub (nth 2 entry)))
          (if (functionp sub)
              (setq found sub)
            (setq found (nth 2 (assq second sub)))))))
    found))

(defun org-lsp-bridge--mode-extension (mode)
  (pcase (symbol-name mode)
    ((rx "html") "html")
    ((rx "latex") "tex")
    ((rx "markdown") "md")
    ((rx "org") "org")
    ((rx "texinfo") "texi")
    (_ "txt")))

(defmacro org-lsp-bridge--in-document (params &rest body)
  "Run BODY in a buffer holding the document of PARAMS, as Emacs would
have it visiting the file: its text as the editor has it, the point where
the cursor is."
  (declare (indent 1))
  `(let* ((file (expand-file-name (alist-get 'file ,params)))
          (buffer (generate-new-buffer (file-name-nondirectory file))))
     (unwind-protect
         (with-current-buffer buffer
           (insert (alist-get 'text ,params))
           (setq buffer-file-name file
                 buffer-file-truename (abbreviate-file-name (file-truename file))
                 default-directory (file-name-directory file))
           (set-buffer-modified-p nil)
           (delay-mode-hooks (org-mode))
           (run-mode-hooks)
           (hack-local-variables)
           (goto-char (min (point-max) (max (point-min) (or (alist-get 'point ,params) 1))))
           ,@body)
       (with-current-buffer buffer
         (set-buffer-modified-p nil)
         (setq buffer-file-name nil))
       (kill-buffer buffer))))

(defun org-lsp-bridge-export (params)
  "Run the menu entry PARAMS names on its document: a file and whether it
was to be opened, or the text of an export to a buffer."
  (let* ((first (string-to-char (alist-get 'first params)))
         (second (let ((key (alist-get 'second params))) (and key (string-to-char key))))
         (options (append (alist-get 'options params) nil))
         (subtree (and (member "subtree" options) t))
         (body (and (member "body" options) t))
         (force (and (member "force" options) t))
         (visible (and (member "visible" options) t))
         (action (unless (eq first ?P) (org-lsp-bridge--action first second)))
         (org-agenda-files (or (append (alist-get 'agenda_files params) nil) org-agenda-files)))
    (setq org-lsp-bridge--opened nil)
    (unless (or action (eq first ?P))
      (error "No export command under %s %s" (string first) (if second (string second) "")))
    (org-lsp-bridge--in-document params
      ;; Timestamps in custom formats, as the file shows them
      ;; (`org-display-custom-times').
      (let ((formats (alist-get 'custom_times params)))
        (when formats
          (setq-local org-display-custom-times t
                      org-timestamp-custom-formats (cons (aref formats 0) (aref formats 1)))))
      ;; Visible only: what the editor has folded, folded here too.
      (when visible
        (dolist (fold (append (alist-get 'folded params) nil))
          (save-excursion
            (goto-char (point-min))
            (forward-line (aref fold 0))
            (let ((start (line-end-position)))
              (forward-line (- (aref fold 1) (aref fold 0)))
              (org-fold-region start (line-end-position) t 'outline)))))
      (let ((result
             (if (eq first ?P)
                 (pcase second
                   (?f (org-publish-current-file force) 'published)
                   (?p (org-publish-current-project force) 'published)
                   (?x (org-publish (assoc (alist-get 'project params) org-publish-project-alist) force)
                       'published)
                   (?a (org-publish-all force) 'published)
                   (_ (error "No publishing command under %s" (string second))))
               (funcall action nil subtree visible body))))
        (cond
         (org-lsp-bridge--opened
          `((file . ,org-lsp-bridge--opened) (open . t)))
         ((eq result 'published) '((published . t)))
         ((bufferp result)
          (with-current-buffer result
            (prog1 `((text . ,(buffer-substring-no-properties (point-min) (point-max)))
                     (name . ,(buffer-name))
                     (extension . ,(org-lsp-bridge--mode-extension major-mode)))
              (kill-buffer result))))
         ((stringp result)
          `((file . ,(expand-file-name result))))
         (t '((done . t))))))))

(defun org-lsp-bridge-publish-projects (_params)
  "The names of the projects `org-publish' knows."
  (vconcat (mapcar #'car org-publish-project-alist)))

;;; Commands that change the document (Babel)

(defconst org-lsp-bridge--commands
  '(org-ctrl-c-ctrl-c org-babel-execute-maybe org-babel-execute-buffer
    org-babel-execute-subtree org-babel-remove-result-one-or-many
    org-footnote-action org-footnote-sort org-footnote-renumber-fn:N
    org-footnote-normalize org-footnote-delete
    org-toggle-item org-toggle-heading org-toggle-comment org-emphasize
    org-insert-structure-template org-insert-drawer
    org-clone-subtree-with-time-shift org-lsp-bridge-footnote-renumber-and-sort
    org-table-recalculate org-table-iterate org-table-calc-current-TBLFM
    org-lsp-bridge-debug-formulas
    org-babel-tangle org-lsp-bridge-detangle org-table-copy-down org-lsp-bridge-table-sum org-table-field-info
    org-lsp-bridge-table-create org-babel-next-src-block
    org-babel-previous-src-block org-babel-goto-named-src-block
    org-next-block org-previous-block org-timer-start org-timer org-timer-item
    org-timer-pause-or-continue org-timer-stop org-timer-set-timer
    org-lsp-bridge-timer-set org-table-move-cell-up org-table-move-cell-down
    org-table-move-cell-left org-table-move-cell-right org-babel-lob-ingest
    org-lsp-bridge-export-template org-metaleft org-metaright org-metaup
    org-metadown org-shiftmetaleft org-shiftmetaright org-toggle-checkbox
    org-table-create-or-convert-from-region org-table-cut-region
    org-table-copy-region org-table-paste-rectangle org-lsp-bridge-timer-offset
    org-lsp-bridge-timer-shift org-table-import org-table-export org-dblock-update
    org-update-all-dblocks org-lsp-bridge-columns-insert-dblock org-lsp-bridge-attach
    org-lsp-bridge-edit-special-save org-lsp-bridge-formulas-save
    org-lsp-bridge-field-save org-encrypt-entry org-decrypt-entry org-encrypt-entries
    org-decrypt-entries org-lsp-bridge-columns-command org-plot/gnuplot
    orgtbl-ascii-plot)
  "The commands the server may run on a document.")

(defvar org-lsp-bridge--extra nil
  "More a command run by the server gives back.")

(defvar org-lsp-bridge--message nil
  "What Emacs said last while a command ran.")

(defun org-lsp-bridge--record-message (old format &rest args)
  (let ((said (apply old format args)))
    (when (and format said (not (string-empty-p said)))
      (setq org-lsp-bridge--message said))
    said))

;; `org-table-formula-constants' and its relatives, as org-lsp's settings
;; have them: the formulas org-lsp leaves to Emacs follow them too.
(defun org-lsp-bridge--bind (variables)
  "Give Org's table options and the indentation of source blocks
\(`org-src-content-indentation', `org-src-preserve-indentation') among
VARIABLES, pairs of a name and a value in Lisp, those values in the
current buffer."
  (dolist (pair (append variables nil))
    (let ((name (elt pair 0)))
      (when (string-match-p
             "\\`org-\\(?:table-\\|calc-default-modes\\'\\|src-\\(?:content\\|preserve\\)-indentation\\'\\)" name)
        (set (make-local-variable (intern name))
             (eval (car (read-from-string (elt pair 1))) t))))))

(defun org-lsp-bridge-run (params)
  "Run the command PARAMS names in its document, at its point: the text it
leaves, where the point is then, and what Emacs said last. With `args' it
is called with them (a number stands for a character where the command
wants one), else interactively."
  (let ((command (intern (alist-get 'command params)))
        (args (append (alist-get 'args params) nil)))
    (unless (memq command org-lsp-bridge--commands)
      (error "org-lsp does not run %s" command))
    (setq org-lsp-bridge--message nil
          org-lsp-bridge--opened nil
          org-lsp-bridge--visited nil
          org-lsp-bridge--extra nil)
    (org-lsp-bridge--in-document params
      (org-lsp-bridge--bind (alist-get 'variables params))
      ;; The selection, as the active region.
      (when-let* ((mark (alist-get 'mark params)))
        (transient-mark-mode 1)
        (set-mark (min (point-max) (max (point-min) mark)))
        (activate-mark))
      (advice-add 'message :around #'org-lsp-bridge--record-message)
      (unwind-protect
          (let ((current-prefix-arg nil))
            (if (or args (not (commandp command)))
                (apply command args)
              (call-interactively command)))
        (advice-remove 'message #'org-lsp-bridge--record-message))
      `((text . ,(buffer-substring-no-properties (point-min) (point-max)))
        (point . ,(point))
        (message . ,org-lsp-bridge--message)
        (opened . ,org-lsp-bridge--opened)
        (visited . ,org-lsp-bridge--visited)
        (extra . ,org-lsp-bridge--extra)
        (countdown . ,(org-lsp-bridge--countdown))
        (timer . ,(org-lsp-bridge--timer))))))

(defun org-lsp-bridge--report-open (path &rest _)
  "`org-open-file', for the editor: PATH reported to open."
  (setq org-lsp-bridge--opened (expand-file-name path))
  path)

(defun org-lsp-bridge-detangle (source)
  "`org-babel-detangle' run in SOURCE, a file tangled from the document:
the blocks its links lead to in the document get its code back. Other
files it leads to are left as they are."
  (require 'ob-tangle)
  (let ((before (buffer-list))
        (document (current-buffer)))
    ;; The links lead back to the Org files: visited, not reported.
    (advice-remove 'org-open-file #'org-lsp-bridge--report-open)
    (unwind-protect
        (with-current-buffer (find-file-noselect source)
          (org-babel-detangle))
      (advice-add 'org-open-file :override #'org-lsp-bridge--report-open)
      (dolist (buffer (buffer-list))
        (unless (or (memq buffer before) (eq buffer document))
          (with-current-buffer buffer (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

(defun org-lsp-bridge-debug-formulas (command &rest args)
  "COMMAND, one org-lsp runs, called with ARGS with the formula debugger
on (`org-table-formula-debug'): after each formula the editor is asked
whether to go on, the formula's substitution history with the question.
Answered no, what was computed so far stays, as in Emacs."
  (let ((command (intern command))
        (ask (symbol-function 'y-or-n-p))
        (org-table-formula-debug t))
    (unless (memq command org-lsp-bridge--commands)
      (error "org-lsp does not run %s" command))
    (condition-case err
        (cl-letf (((symbol-function 'y-or-n-p)
                   (lambda (prompt &rest _)
                     (funcall ask (concat (org-lsp-bridge--history) "\n\n" prompt))))
                  ((symbol-function 'org-fit-window-to-buffer) #'ignore)
                  ((symbol-function 'delete-window) #'ignore)
                  ((symbol-function 'set-window-configuration) #'ignore))
          (apply command args))
      (user-error (setq org-lsp-bridge--message (error-message-string err))))
    ;; A history shown without a question: what is said last.
    (when-let* ((history (org-lsp-bridge--history)))
      (setq org-lsp-bridge--message history))))

(defun org-lsp-bridge--history ()
  "What the formula debugger showed last (`*Substitution History*'), and
none after that."
  (when-let* ((buffer (get-buffer "*Substitution History*")))
    (prog1 (with-current-buffer buffer (string-trim (buffer-string)))
      (kill-buffer buffer))))

(defun org-lsp-bridge-footnote-renumber-and-sort ()
  "`S' in `org-footnote-action''s menu."
  (org-footnote-renumber-fn:N)
  (org-footnote-sort))

(defun org-lsp-bridge-timer-offset (offset default)
  "`C-u C-c C-x 0': the timer started with OFFSET, DEFAULT when it is blank."
  (org-timer-start
   (org-timer-fix-incomplete (if (string-match "\\S-" offset) offset default))))

(defun org-lsp-bridge-timer-shift (delta)
  "`C-u C-u C-c C-x 0': the timer values of the region shifted by DELTA."
  (org-timer-change-times-in-region (region-beginning) (region-end) delta))

(defun org-lsp-bridge-columns-insert-dblock (id)
  "`org-columns-insert-dblock', the columns of ID: `global', `local' (or
empty) or an entry's ID."
  (org-create-dblock
   (list :name "columnview"
         :hlines 1
         :id (cond ((string= id "global") 'global)
                   ((member id '("" "local")) 'local)
                   (id))))
  (org-update-dblock))

(defun org-lsp-bridge-table-sum ()
  "`org-table-sum', saying the sum as it does when called by a key."
  (cl-letf (((symbol-function 'called-interactively-p) (lambda (&rest _) t)))
    (org-table-sum)))

(defun org-lsp-bridge-table-create (size)
  "`org-table-create' of SIZE, `org-table-default-size' when it is empty."
  (org-table-create (if (string-empty-p size) org-table-default-size size)))

;;; Export templates

(defun org-lsp-bridge-export-backends (_params)
  "The options categories `org-export-insert-default-template' offers."
  (vconcat (cons "default"
                 (mapcar (lambda (backend) (symbol-name (org-export-backend-name backend)))
                         org-export-registered-backends))))

(defun org-lsp-bridge-export-template (name subtree)
  "`org-export-insert-default-template' for the backend NAME (`default' for
none), for the subtree when SUBTREE is t."
  (org-export-insert-default-template (if (equal name "default") 'default (intern name))
                                      (eq subtree t)))

;;; Timers

(defun org-lsp-bridge-timer-set (minutes)
  "`org-timer-set-timer' for MINUTES (`10', `1:30:00'), the effort aside."
  (cl-letf (((symbol-function 'read-from-minibuffer) (lambda (&rest _) minutes)))
    (org-timer-set-timer '(64))))

(defun org-lsp-bridge--countdown ()
  "The seconds before the countdown ends, or nil when none runs (or it
is paused)."
  (and (boundp 'org-timer-countdown-timer)
       (timerp org-timer-countdown-timer)
       (not org-timer-pause-time)
       (max 0 (float-time (time-subtract (timer--time org-timer-countdown-timer) nil)))))

(defun org-lsp-bridge--timer ()
  "The relative timer or the countdown as the mode line shows it
(`org-timer-update-mode-line'): its value in seconds, whether it counts
down, whether it is paused. Nil when none is on, or none shows
(`org-timer-display')."
  (when (and (boundp 'org-timer-start-time) org-timer-start-time org-timer-display)
    (let ((time (- (float-time org-timer-pause-time) (float-time org-timer-start-time))))
      `((seconds . ,(abs (floor (if org-timer-countdown-timer (- time) time))))
        (countdown . ,(if org-timer-countdown-timer t :json-false))
        (paused . ,(if org-timer-pause-time t :json-false))))))

(defun org-lsp-bridge-timer-fire (_params)
  "End the countdown, as its timer would: what Org says then."
  (require 'org-timer)
  (let ((timer org-timer-countdown-timer)
        (org-show-notification-handler (lambda (text) (message "%s" text))))
    (setq org-lsp-bridge--message nil)
    (when (timerp timer)
      (cancel-timer timer)
      (advice-add 'message :around #'org-lsp-bridge--record-message)
      (unwind-protect (apply (timer--function timer) (timer--args timer))
        (advice-remove 'message #'org-lsp-bridge--record-message)))
    `((message . ,org-lsp-bridge--message)
      (timer . ,(org-lsp-bridge--timer)))))

(defun org-lsp-bridge-timer-default (params)
  "Whether `org-timer-set-timer' at the point of PARAMS takes the effort,
and the duration it would offer."
  (require 'org-timer)
  (org-lsp-bridge--in-document params
    (let ((effort (org-entry-get nil org-effort-property)))
      `((effort . ,(if (org-string-nw-p effort) t :json-false))
        (default . ,(let ((default (format "%s" org-timer-default-timer)))
                      (if (string-equal default "0") "" default)))))))

;;; Capture

(defun org-lsp-bridge-eval (params)
  "What the `%(sexp)' of a capture template, the `sexp' of PARAMS, puts in:
a string or nil, as `org-capture-expand-embedded-elisp' takes it (an error
in it becomes `%![Error: …]')."
  (let* ((sexp (alist-get 'sexp params))
         (directory (alist-get 'directory params))
         (default-directory (if directory (file-name-as-directory directory) default-directory))
         (result (with-temp-buffer (org-mode) (org-eval (read sexp)))))
    (cond ((null result) nil)
          ((stringp result) result)
          (t (error "Capture template sexp `%s' must evaluate to string or nil" sexp)))))

;;; Links

(defun org-lsp-bridge-expand-abbrev (params)
  "`org-link-expand-abbrev' of the `link' of PARAMS, with the
`abbreviations' of PARAMS in force (each [NAME REPLACEMENT], the file's
first). Where Org would warn and leave the link as it is, the warning is
the error."
  (let ((org-link-abbrev-alist-local
         (mapcar (lambda (pair) (cons (aref pair 0) (aref pair 1)))
                 (alist-get 'abbreviations params)))
        (org-link-abbrev-alist nil))
    (cl-letf (((symbol-function 'org-display-warning)
               (lambda (message) (error "%s" message))))
      (org-link-expand-abbrev (alist-get 'link params)))))

(defun org-lsp-bridge-bibtex-autokey (params)
  "The key `bibtex-generate-autokey' makes of the BibTeX `entry' of
PARAMS, as `org-bibtex-autokey' asks it: in a buffer of its own, with
the regexps of the dialect, which only `bibtex-mode' sets otherwise."
  (require 'bibtex)
  (unless bibtex-entry-head
    (bibtex-set-dialect))
  (with-temp-buffer
    (insert (alist-get 'entry params))
    (bibtex-generate-autokey)))

;;; What the server asks about

(defun org-lsp-bridge-diary-sexp (params)
  "What the diary sexp of PARAMS, with its entry, gives on its date
\(`org-diary-sexp-entry'): the lines it shows, or nil."
  (require 'diary-lib)
  (let* ((date (append (alist-get 'date params) nil))
         (result (cl-letf (((symbol-function 'sleep-for) #'ignore)
                           ((symbol-function 'beep) #'ignore))
                   (org-diary-sexp-entry (alist-get 'sexp params) (alist-get 'entry params) date))))
    (cond ((null result) nil)
          ((stringp result) (vector result))
          (t (vconcat (seq-filter #'stringp result))))))

(defun org-lsp-bridge-agenda-prefix (params)
  "The `%(…)' of `org-agenda-prefix-format' on the lines of an agenda,
each of the `forms' of PARAMS evaluated as `org-agenda-format-item' does:
in a buffer holding its `file' (one of the `files', each its `file' and
`text' as the editor has them), point at its `point' (its headline, a
character from 1), the variables of the format bound to its `category',
`extra', `time', `level', `tag', `effort' and `breadcrumbs'. A line with
no file, of the time grid, is evaluated in no Org buffer. What `format'
makes of each, or the `error' it signals.

As in Org's compiled format, `(org-eval FORM)', the form is evaluated
before `org-eval' is called, which evaluates its value again: an error
in the form is not caught there, and stops Org's agenda."
  (require 'org-agenda)
  (let (buffers)
    (unwind-protect
        (progn
          (dolist (file (append (alist-get 'files params) nil))
            (let* ((path (expand-file-name (alist-get 'file file)))
                   (buffer (generate-new-buffer (file-name-nondirectory path))))
              (with-current-buffer buffer
                (insert (alist-get 'text file))
                (setq buffer-file-name path
                      buffer-file-truename (abbreviate-file-name (file-truename path))
                      default-directory (file-name-directory path))
                (set-buffer-modified-p nil)
                (delay-mode-hooks (org-mode)))
              (push (cons path buffer) buffers)))
          (with-temp-buffer
            (let ((outside (current-buffer)))
              (vconcat
               (mapcar
                (lambda (form)
                  (let ((file (alist-get 'file form)))
                    (with-current-buffer (if file (cdr (assoc (expand-file-name file) buffers)) outside)
                      (save-excursion
                        (when-let* ((point (alist-get 'point form)))
                          (goto-char point))
                        ;; The variables `org-agenda-format-item' binds
                        ;; around the format, dynamically.
                        (cl-progv '(category category-icon extra time level tag effort breadcrumbs)
                            (list (alist-get 'category form) ""
                                  (alist-get 'extra form) (alist-get 'time form)
                                  (alist-get 'level form) (alist-get 'tag form)
                                  (alist-get 'effort form) (alist-get 'breadcrumbs form))
                          (condition-case err
                              (format "%s" (org-eval (eval (read (alist-get 'form form)) t)))
                            (error `((error . ,(error-message-string err))))))))))
                (alist-get 'forms params))))))
      (dolist (buffer buffers)
        (with-current-buffer (cdr buffer)
          (set-buffer-modified-p nil)
          (setq buffer-file-name nil))
        (kill-buffer (cdr buffer))))))

(defun org-lsp-bridge-capture-target (params)
  "Where the function of a `function' or `file+function' capture target
leaves the point: the file and the point. For `file+function', in a
buffer holding the file of PARAMS as the editor has it."
  (let ((fn (intern-soft (alist-get 'function params)))
        (buffers (buffer-list)))
    (unless (functionp fn)
      (error "Invalid capture target specification: %s" (alist-get 'function params)))
    (unwind-protect
        (if (alist-get 'file params)
            (org-lsp-bridge--in-document params
              (funcall fn)
              `((file . ,buffer-file-name) (point . ,(point))))
          (save-window-excursion
            (save-excursion
              (funcall fn)
              (unless buffer-file-name
                (error "The capture target function visited no file"))
              `((file . ,buffer-file-name) (point . ,(point))))))
      ;; The files it visited are read again next time.
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers)
          (with-current-buffer buffer (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

(defun org-lsp-bridge-refile-verify (params)
  "`org-refile-target-verify-function' at the headlines of PARAMS (their
points, in order), in its document: whether each is a target. One before
where the function left the point after another is skipped, as
`org-refile-get-targets' goes on from there."
  (let ((fn (intern-soft (alist-get 'function params)))
        (points (append (alist-get 'points params) nil)))
    (unless (functionp fn)
      (error "No function %s" (alist-get 'function params)))
    (org-lsp-bridge--in-document params
      (let ((cursor (point-min)) verdicts)
        (dolist (start points)
          (if (< start cursor)
              (push :json-false verdicts)
            (goto-char start)
            (push (if (funcall fn) t :json-false) verdicts)
            (setq cursor (if (= (point) start) (line-end-position) (point)))))
        (vconcat (nreverse verdicts))))))

(defun org-lsp-bridge-attach-expand (params)
  "`org-attach-expand' of the name in PARAMS, at its point of its document:
the file in the entry's attachment directory, or nil without one."
  (require 'org-attach)
  (org-lsp-bridge--in-document params
    (when (org-attach-dir)
      (org-attach-expand (alist-get 'name params)))))

(defun org-lsp-bridge-attach-files (params)
  "The attachments of the entry at the point of PARAMS's document."
  (require 'org-attach)
  (org-lsp-bridge--in-document params
    (let ((dir (org-attach-dir)))
      (vconcat (and dir (org-attach-file-list dir))))))

(defvar org-lsp-bridge--visited nil
  "The file an attachment command visited, for the editor to open.")

(defun org-lsp-bridge-attach (key answer)
  "The command of KEY in `org-attach-commands', with ANSWER for what it
asks: a file, a URL, a name, a directory or an attachment."
  (require 'org-attach)
  (setq org-lsp-bridge--visited nil)
  (cl-letf (((symbol-function 'read-file-name) (lambda (&rest _) answer))
            ((symbol-function 'read-directory-name) (lambda (&rest _) answer))
            ((symbol-function 'completing-read) (lambda (&rest _) answer))
            ((symbol-function 'find-file)
             (lambda (file &rest _) (setq org-lsp-bridge--visited (expand-file-name file))))
            ((symbol-function 'dired)
             (lambda (dir &rest _) (setq org-lsp-bridge--opened (expand-file-name dir)))))
    (pcase key
      ("a" (org-attach-attach (expand-file-name answer)))
      ("c" (org-attach-attach (expand-file-name answer) nil 'cp))
      ("m" (org-attach-attach (expand-file-name answer) nil 'mv))
      ("l" (org-attach-attach (expand-file-name answer) nil 'ln))
      ("y" (org-attach-attach (expand-file-name answer) nil 'lns))
      ("u" (org-attach-url answer))
      ("n" (org-attach-new answer))
      ("z" (org-attach-sync))
      ("o" (org-attach-open))
      ("f" (org-attach-reveal))
      ("d" (org-attach-delete-one answer))
      ("D" (org-attach-delete-all))
      ("s" (org-attach-set-directory))
      ("S" (org-attach-unset-directory))
      (_ (error "No attachment command under %s" key)))))

(defun org-lsp-bridge-src-block-names (params)
  "The names of the code blocks of the document of PARAMS, in its order."
  (org-lsp-bridge--in-document params
    (vconcat (reverse (org-babel-src-block-names)))))

(defun org-lsp-bridge-structure-templates (_params)
  "`org-structure-template-alist': each key with the type it inserts."
  (vconcat (mapcar (lambda (entry) (vector (car entry) (cdr entry)))
                   org-structure-template-alist)))

;;; Editing in a buffer of its own

(defconst org-lsp-bridge--languages
  '(("rust" . "rs") ("go" . "go") ("typescript" . "ts") ("ts" . "ts") ("json" . "json")
    ("yaml" . "yaml") ("toml" . "toml") ("kotlin" . "kt") ("swift" . "swift")
    ("dockerfile" . "Dockerfile") ("nix" . "nix") ("zig" . "zig") ("elixir" . "ex")
    ("php" . "php") ("csharp" . "cs") ("fish" . "fish") ("zsh" . "zsh"))
  "Extensions of languages Org's own Babel does not know.")

(defun org-lsp-bridge--language-extension (language)
  "The extension of a file of LANGUAGE, as tangling names it."
  (require 'ob-tangle)
  (let ((language (downcase language)))
    (or (progn (require (intern (concat "ob-" language)) nil t)
               (cdr (assoc language org-babel-tangle-lang-exts)))
        (cdr (assoc language org-lsp-bridge--languages))
        (and (string-match-p "\\`[a-z0-9]+\\'" language) language)
        "txt")))

(defun org-lsp-bridge--read-only-ends ()
  "The read-only text at the start and the end of the edit buffer
(a footnote's label, `Cannot edit footnote label')."
  (let ((start (or (text-property-not-all (point-min) (point-max) 'read-only nil) (point-max)))
        (end (point-max)))
    (while (and (> end (point-min)) (get-text-property (1- end) 'read-only))
      (setq end (1- end)))
    (list (if (= start (point-min))
              (buffer-substring-no-properties (point-min)
                                              (or (next-single-property-change (point-min) 'read-only) (point-max)))
            "")
          (buffer-substring-no-properties end (point-max)))))

(defun org-lsp-bridge-edit-special (params)
  "What `org-edit-special' does at the point of PARAMS. In an edit buffer
of `org-src' (a block, a footnote, a LaTeX fragment…): its text, an
extension for its file, and the part of the document it edits, from
`begin' to `end'. At `#+INCLUDE:' and the like: the file to visit. Else
what the element is, for the server to do as Org does there."
  (require 'org-src)
  (org-lsp-bridge--in-document params
    (org-lsp-bridge--bind (alist-get 'variables params))
    (let* ((element (org-element-at-point))
           (context (org-element-context element)))
      (pcase (org-element-type element)
        ((or 'table 'table-row)
         (if (eq (org-element-property :type element) 'table.el)
             (org-lsp-bridge--edit-buffer element)
           (org-lsp-bridge--formulas)))
        ('planning `((other . ,(if (org-element-property :deadline element) "deadline" "schedule"))))
        ('keyword
         (let ((value (org-element-property :value element)))
           (unless (member (org-element-property :key element) '("BIBLIOGRAPHY" "INCLUDE" "SETUPFILE"))
             (user-error "No special environment to edit here"))
           (unless (org-string-nw-p value) (user-error "No file to edit"))
           (let ((file (and (string-match "\\`\"\\(.*?\\)\"\\|\\S-+" value)
                            (or (match-string 1 value) (match-string 0 value)))))
             (when (org-url-p file)
               (user-error "Files located with a URL cannot be edited"))
             `((visit . ,(expand-file-name file))))))
        ((guard (eq (org-element-type context) 'timestamp))
         `((other . ,(if (eq (org-element-property :type context) 'inactive) "timestamp-inactive" "timestamp"))))
        ((guard (eq (org-element-type context) 'link)) '((other . "link")))
        (_ (org-lsp-bridge--edit-buffer element))))))

(defun org-lsp-bridge--edit-buffer (element)
  "`org-edit-special' on ELEMENT at point, which `org-src' edits in a
buffer of its own: what the buffer shows (see `org-lsp-bridge-edit-special')."
  (let ((org-src-window-setup 'current-window)
        (language (pcase (org-element-type element)
                    ('src-block (org-element-property :language element))
                    (_ (let ((context (org-element-context element)))
                         (and (eq (org-element-type context) 'inline-src-block)
                              (org-element-property :language context)))))))
    (org-edit-special)
    (unless (org-src-edit-buffer-p)
      (user-error "No special environment to edit here"))
    (let ((text (buffer-substring-no-properties (point-min) (point-max)))
          (extension (if (org-string-nw-p language)
                         (org-lsp-bridge--language-extension language)
                       (org-lsp-bridge--mode-extension major-mode)))
          (begin (marker-position org-src--beg-marker))
          (end (marker-position org-src--end-marker))
          (name (buffer-name)))
      (org-edit-src-abort)
      `((edit . ((text . ,text)
                 (extension . ,extension)
                 (name . ,name)
                 (begin . ,begin)
                 (end . ,end)
                 (region . ,(buffer-substring-no-properties begin end))))))))

(defun org-lsp-bridge--table-bounds ()
  "Where the table at point starts, and where it ends with its `#+TBLFM:'
lines."
  (list (org-table-begin)
        (save-excursion
          (goto-char (org-table-end))
          (while (looking-at-p "[ \t]*#\\+TBLFM:")
            (forward-line))
          (point))))

(defun org-lsp-bridge--formulas ()
  "`org-table-edit-formulas' at point: what its buffer shows (see
`org-lsp-bridge-edit-special'), the table with its formulas the part it
edits."
  (pcase-let ((`(,begin ,end) (org-lsp-bridge--table-bounds))
              (region nil)
              (text nil))
    (setq region (buffer-substring-no-properties begin end))
    (switch-to-buffer (current-buffer))
    (save-window-excursion
      (org-table-edit-formulas)
      (setq text (buffer-substring-no-properties (point-min) (point-max)))
      (kill-buffer))
    `((edit . ((text . ,text)
               (extension . "txt")
               (name . "*Edit Formulas*")
               (kind . "formulas")
               (begin . ,begin)
               (end . ,end)
               (region . ,region))))))

(defun org-lsp-bridge-edit-field (params)
  "`org-table-edit-field' at the point of PARAMS: what its buffer shows
(see `org-lsp-bridge-edit-special'), the table the part it edits."
  (org-lsp-bridge--in-document params
    (unless (org-at-table-p) (user-error "Not at a table"))
    (pcase-let* ((`(,begin ,end) (org-lsp-bridge--table-bounds))
                 (region (buffer-substring-no-properties begin end))
                 (text nil))
      (switch-to-buffer (current-buffer))
      (save-window-excursion
        (org-table-edit-field nil)
        (setq text (buffer-substring-no-properties (point-min) (point-max)))
        (kill-buffer))
      `((edit . ((text . ,text)
                 (extension . "org")
                 (name . "*Org Table Edit Field*")
                 (kind . "field")
                 (begin . ,begin)
                 (end . ,end)
                 (region . ,region)))))))

(defun org-lsp-bridge-field-save (content)
  "The field at point edited (`org-table-edit-field'), CONTENT its
buffer's text, and put back (`org-table-finish-edit-field'): the table
then, in `org-lsp-bridge--extra'."
  (switch-to-buffer (current-buffer))
  (save-window-excursion
    (org-table-edit-field nil)
    (erase-buffer)
    (insert content)
    (org-table-finish-edit-field))
  (pcase-let ((`(,begin ,end) (org-lsp-bridge--table-bounds)))
    (setq org-lsp-bridge--extra `((begin . ,begin) (end . ,end)))))

(defun org-lsp-bridge-formulas-save (content)
  "The formulas of the table at point edited (`org-table-edit-formulas'),
CONTENT their buffer's text, and installed (`org-table-fedit-finish'): the
table then, in `org-lsp-bridge--extra'."
  (switch-to-buffer (current-buffer))
  (save-window-excursion
    (org-table-edit-formulas)
    (erase-buffer)
    (insert content)
    (org-table-fedit-finish))
  (pcase-let ((`(,begin ,end) (org-lsp-bridge--table-bounds)))
    (setq org-lsp-bridge--extra `((begin . ,begin) (end . ,end)))))

(defun org-lsp-bridge-edit-special-save (content)
  "Edit the element at point in its buffer (`org-edit-special'), its text
now CONTENT, and back (`org-edit-src-exit'): the part of the document it
edits then, in `org-lsp-bridge--extra'."
  (require 'org-src)
  (let ((org-src-window-setup 'current-window))
    (org-edit-special)
    (unless (org-src-edit-buffer-p)
      (user-error "No special environment to edit here"))
    (pcase-let ((`(,head ,tail) (org-lsp-bridge--read-only-ends)))
      (unless (and (string-prefix-p head content) (string-suffix-p tail content)
                   (<= (+ (length head) (length tail)) (length content)))
        (org-edit-src-abort)
        (user-error "%s" (if (string-prefix-p head content)
                             "Cannot edit past footnote reference"
                           "Cannot edit footnote label"))))
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert content))
    (let ((begin (copy-marker org-src--beg-marker))
          (end (copy-marker org-src--end-marker t)))
      (org-edit-src-exit)
      (setq org-lsp-bridge--extra `((begin . ,(marker-position begin))
                                    (end . ,(marker-position end))))
      (set-marker begin nil)
      (set-marker end nil))))

;;; Encryption

(defun org-lsp-bridge-crypt-before-save (params)
  "Whether Org files encrypt their entries before they are saved
(`org-crypt-use-before-save-magic'), and if so the document of PARAMS
with its entries tagged `crypt' encrypted, as the hook does."
  (org-lsp-bridge--in-document params
    (if (not (memq 'org-encrypt-entries before-save-hook))
        '((magic . :json-false))
      (let ((inhibit-message t))
        (org-encrypt-entries))
      `((magic . t)
        (text . ,(buffer-substring-no-properties (point-min) (point-max)))))))

;;; Column view

(defun org-lsp-bridge--columns-view ()
  "What column view shows in the current buffer: its columns (key, title,
width), its header line, and for each headline it covers its line (from
0), the text the line shows and the value of each column."
  (let ((rows (make-hash-table)))
    (dolist (ov (overlays-in (point-min) (point-max)))
      (when (overlay-get ov 'org-columns-key)
        (push ov (gethash (1- (line-number-at-pos (overlay-start ov) t)) rows))))
    (let (lines)
      (maphash
       (lambda (line ovs)
         (let ((ovs (sort ovs (lambda (a b) (< (overlay-start a) (overlay-start b))))))
           (push (vector line
                         (mapconcat (lambda (ov) (substring-no-properties (or (overlay-get ov 'display) ""))) ovs "")
                         (vconcat (mapcar (lambda (ov) (format "%s" (or (overlay-get ov 'org-columns-value) ""))) ovs)))
                 lines)))
       rows)
      (let ((i -1))
        `((columns . ,(vconcat
                       (mapcar (lambda (column)
                                 (setq i (1+ i))
                                 `((key . ,(nth 0 column))
                                   (title . ,(or (nth 1 column) (nth 0 column)))
                                   (width . ,(aref org-columns-current-maxwidths i))))
                               org-columns-current-fmt-compiled)))
          (header . ,(substring-no-properties (or org-columns-full-header-line-format " ") 1))
          (lines . ,(vconcat (sort lines (lambda (a b) (< (aref a 0) (aref b 0)))))))))))

(defun org-lsp-bridge-columns (params)
  "Column view of the document of PARAMS, turned on where its point is
\(`org-columns'; for the whole file with `global'): what it shows."
  (require 'org-colview)
  (org-lsp-bridge--in-document params
    (switch-to-buffer (current-buffer))
    (let ((inhibit-message t))
      (org-columns (eq (alist-get 'global params) t)))
    (prog1 (org-lsp-bridge--columns-view)
      (org-columns-quit))))

;;; Column view in the agenda

(defmacro org-lsp-bridge--with-agenda (params &rest body)
  "Run BODY in an agenda buffer made of PARAMS, column view on there
\(`org-agenda-columns'): its `lines', each its `text' and, for an entry,
its `file' and the `point' of its headline (a character, from 1), or
`date' for a date line and `structure' for a block's header; the `start'
line (from 0) it is turned on at, whose entry has the format; the `files'
as the editor has them, each its `file' and `text', in buffers visiting
them. The buffers go afterwards."
  (declare (indent 1))
  `(let (org-lsp-bridge--agenda-files)
     (unwind-protect
         (progn
           (dolist (file (append (alist-get 'files ,params) nil))
             (let* ((path (expand-file-name (alist-get 'file file)))
                    (buffer (generate-new-buffer (file-name-nondirectory path))))
               (with-current-buffer buffer
                 (insert (alist-get 'text file))
                 (setq buffer-file-name path
                       buffer-file-truename (abbreviate-file-name (file-truename path))
                       default-directory (file-name-directory path))
                 (set-buffer-modified-p nil)
                 (delay-mode-hooks (org-mode)))
               (push (list path buffer (alist-get 'text file)) org-lsp-bridge--agenda-files)))
           (with-current-buffer (get-buffer-create " *org-lsp agenda*")
             (org-agenda-mode)
             (let ((inhibit-read-only t))
               (erase-buffer)
               (dolist (line (append (alist-get 'lines ,params) nil))
                 (let ((text (copy-sequence (alist-get 'text line)))
                       (file (alist-get 'file line)))
                   (cond
                    (file
                     (let ((marker (with-current-buffer
                                       (nth 1 (assoc (expand-file-name file) org-lsp-bridge--agenda-files))
                                     (copy-marker (alist-get 'point line)))))
                       (add-text-properties 0 (length text) `(org-hd-marker ,marker org-marker ,marker) text)))
                    ((eq (alist-get 'date line) t)
                     (add-text-properties 0 (length text) '(org-date-line t) text))
                    ((eq (alist-get 'structure line) t)
                     (add-text-properties 0 (length text) '(face org-agenda-structure) text)))
                   (insert text "\n"))))
             (setq-local org-agenda-contributing-files (mapcar #'car org-lsp-bridge--agenda-files))
             (setq-local org-done-keywords-for-agenda
                         (delete-dups
                          (apply #'append
                                 (mapcar (lambda (file) (with-current-buffer (nth 1 file) org-done-keywords))
                                         org-lsp-bridge--agenda-files))))
             (switch-to-buffer (current-buffer))
             (goto-char (point-min))
             (forward-line (or (alist-get 'start ,params) 0))
             (let ((inhibit-message t)) (org-agenda-columns))
             ,@body))
       (when-let* ((agenda (get-buffer " *org-lsp agenda*")))
         (with-current-buffer agenda (org-columns-remove-overlays))
         (kill-buffer agenda))
       (dolist (file org-lsp-bridge--agenda-files)
         (with-current-buffer (nth 1 file)
           (set-buffer-modified-p nil)
           (setq buffer-file-name nil))
         (kill-buffer (nth 1 file))))))

(defvar org-lsp-bridge--agenda-files nil
  "The files of the agenda being shown in column view: each its name, its
buffer and its text before.")

(defun org-lsp-bridge-agenda-columns (params)
  "Column view of the agenda of PARAMS (see `org-lsp-bridge--with-agenda'):
what it shows, as for a file."
  (require 'org-colview)
  (require 'org-agenda)
  (org-lsp-bridge--with-agenda params
    (org-lsp-bridge--columns-view)))

(defun org-lsp-bridge--call-answered (command answer)
  "Call COMMAND, one of column view's, interactively; what it asks is
ANSWER, without fast selection or a calendar, when there is one."
  (unless (string-prefix-p "org-columns-" (format "%s" command))
    (error "org-lsp does not run %s" command))
  (let ((org-use-fast-todo-selection nil)
        (org-use-fast-tag-selection nil)
        (org-read-date-popup-calendar nil)
        (command (intern (format "%s" command))))
    (if (not answer)
        (call-interactively command)
      (cl-letf* ((answered (lambda (&rest _) answer))
                 ((symbol-function 'read-string) answered)
                 ((symbol-function 'read-from-minibuffer) answered)
                 ((symbol-function 'completing-read) answered)
                 ((symbol-function 'org-completing-read) answered)
                 ((symbol-function 'read-char-exclusive)
                  (lambda (&rest _) (if (string-empty-p answer) ?\s (string-to-char answer)))))
        (call-interactively command)))))

(defun org-lsp-bridge-agenda-columns-command (params)
  "Column view of the agenda of PARAMS (see `org-lsp-bridge--with-agenda'),
its `command' run on the `column'th column (from 0) of its `line' (from
0), `answer' what it asks: the files it changed, each its `file' and
`text', what the view shows then and what Emacs said."
  (require 'org-colview)
  (require 'org-agenda)
  (setq org-lsp-bridge--message nil)
  (org-lsp-bridge--with-agenda params
    (goto-char (point-min))
    (forward-line (alist-get 'line params))
    (goto-char (min (+ (point) (alist-get 'column params)) (line-end-position)))
    (advice-add 'message :around #'org-lsp-bridge--record-message)
    (unwind-protect
        (org-lsp-bridge--call-answered (alist-get 'command params) (alist-get 'answer params))
      (advice-remove 'message #'org-lsp-bridge--record-message))
    (when (and org-lsp-bridge--message (string-prefix-p "Recomputing columns" org-lsp-bridge--message))
      (setq org-lsp-bridge--message nil))
    `((files . ,(vconcat
                 (delq nil
                       (mapcar (lambda (file)
                                 (with-current-buffer (nth 1 file)
                                   (let ((text (buffer-substring-no-properties (point-min) (point-max))))
                                     (unless (equal text (nth 2 file))
                                       `((file . ,(nth 0 file)) (text . ,text))))))
                               org-lsp-bridge--agenda-files))))
      (view . ,(and (bound-and-true-p org-columns-overlays) (org-lsp-bridge--columns-view)))
      (message . ,org-lsp-bridge--message))))

(defun org-lsp-bridge-columns-command (start global column command &optional answer)
  "Column view turned on at START (for the whole file with GLOBAL), COMMAND
run on the COLUMNth column (from 0) of the line at point, ANSWER what it
asks: what the view shows then, in `org-lsp-bridge--extra'."
  (require 'org-colview)
  (switch-to-buffer (current-buffer))
  (let ((line (line-beginning-position)))
    (save-excursion
      (goto-char start)
      (let ((inhibit-message t))
        (org-columns (eq global t))))
    (goto-char (min (+ line column) (line-end-position)))
    ;; What the command asks, the editor answered first; without fast
    ;; selection or a calendar, a question is one line of text.
    (org-lsp-bridge--call-answered command answer)
    (when (and org-lsp-bridge--message (string-prefix-p "Recomputing columns" org-lsp-bridge--message))
      (setq org-lsp-bridge--message nil))
    ;; Still on unless the command turned it off (`q').
    (setq org-lsp-bridge--extra
          (and (bound-and-true-p org-columns-overlays) (org-lsp-bridge--columns-view)))
    (when (bound-and-true-p org-columns-overlays)
      (org-columns-quit))))

;;; Appointments

(defvar org-lsp-bridge--appt-shown nil
  "What `appt.el' showed in its window while it ran, newest first.")

(defun org-lsp-bridge--appt-window (mins _time strings)
  "`appt-disp-window', for the editor: the appointments due in MINS
minutes, said as the mode line counts them down, with their text."
  (let ((mins (if (listp mins) mins (list mins)))
        (strings (if (listp strings) strings (list strings))))
    (push (format "%s: %s" (appt-mode-line (copy-sequence mins)) (string-join strings "; "))
          org-lsp-bridge--appt-shown)))

(defun org-lsp-bridge--appt (fn)
  "Call FN as `appt.el' would run it in Emacs: what Emacs said last, what
the appointment window showed, and how many appointments are left."
  (require 'appt)
  (let ((appt-audible nil)
        (appt-display-diary nil)
        (appt-disp-window-function (if (eq appt-disp-window-function #'appt-disp-window)
                                       #'org-lsp-bridge--appt-window
                                     appt-disp-window-function))
        (appt-delete-window-function #'ignore))
    (setq org-lsp-bridge--message nil
          org-lsp-bridge--appt-shown nil)
    (advice-add 'message :around #'org-lsp-bridge--record-message)
    (unwind-protect (funcall fn)
      (advice-remove 'message #'org-lsp-bridge--record-message))
    ;; The server checks each minute: Emacs's timer, which would run while
    ;; a command waits for a process, never does.
    (when (timerp appt-timer)
      (cancel-timer appt-timer))
    `((message . ,org-lsp-bridge--message)
      (shown . ,(vconcat (reverse org-lsp-bridge--appt-shown)))
      (appointments . ,(length appt-time-msg-list)))))

(defun org-lsp-bridge-appt (params)
  "`org-agenda-to-appt' on the agenda files of PARAMS (`files'), those the
editor has open as it has them (`texts', each a file and its text), with
`C-u' when `refresh'."
  (require 'org-agenda)
  (let ((org-agenda-files (append (alist-get 'files params) nil))
        (buffers nil))
    (unwind-protect
        (progn
          (dolist (open (append (alist-get 'texts params) nil))
            (let ((buffer (find-file-noselect (aref open 0) t)))
              (with-current-buffer buffer
                (let ((inhibit-read-only t))
                  (erase-buffer)
                  (insert (aref open 1))))
              (push buffer buffers)))
          (org-lsp-bridge--appt
           (lambda () (org-agenda-to-appt (and (eq (alist-get 'refresh params) t) '(4))))))
      (dolist (buffer buffers)
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

(defun org-lsp-bridge-appt-check (_params)
  "`appt-check', as the timer of `appt-activate' runs it each minute."
  (org-lsp-bridge--appt #'appt-check))

;;; LaTeX previews

(defun org-lsp-bridge-latex-preview (params)
  "Make the images of the LaTeX fragments between `from' and `to' (characters,
from 1) of the document PARAMS names, as `org-latex-preview' makes them
(`org--latex-preview-region'), in the colors `foreground' and `background':
where each fragment is and the file of its image, what Emacs shows over it."
  (let ((found nil))
    (org-lsp-bridge--in-document params
      (let ((org-format-latex-options
             (org-combine-plists
              org-format-latex-options
              (list :foreground (or (alist-get 'foreground params) "Black")
                    :background (or (alist-get 'background params) "Transparent")))))
        (cl-letf* ((place (lambda (beg end file)
                            (push `((start . ,beg) (end . ,end) (file . ,file)) found)))
                   ((symbol-function 'org-place-formula-image)
                    (lambda (_link _block-type beg end _value _overlays file _type)
                      (funcall place beg end file)
                      (goto-char end)))
                   ;; Before Org 9.7, `org-format-latex' makes the overlay
                   ;; itself.
                   ((symbol-function 'org--make-preview-overlay)
                    (lambda (beg end image &optional _imagetype)
                      (funcall place beg end image)))
                   ;; Without a window system there is no display to measure
                   ;; ("Window system frame should be used" before Org 9.8):
                   ;; the DPI Org takes for other uses.
                   ((symbol-function 'org--get-display-dpi) (lambda () 140))
                   ;; Nor an image cache to clear, which an Emacs with image
                   ;; support looks for on the display, and fails.
                   ((symbol-function 'clear-image-cache) #'ignore)
                   ((symbol-function 'image-flush) #'ignore)
                   ;; A frame of a terminal rounds `#rrggbb' to one of its few
                   ;; colors: the editor's colors as they are.
                   (color-values (symbol-function 'color-values))
                   ((symbol-function 'color-values)
                    (lambda (color &optional frame)
                      (if (and (stringp color)
                               (string-match "\\`#\\([0-9a-fA-F]\\{6\\}\\)\\'" color))
                          (let ((hex (match-string 1 color)))
                            (mapcar (lambda (at) (* 257 (string-to-number (substring hex at (+ at 2)) 16)))
                                    '(0 2 4)))
                        (funcall color-values color frame)))))
          (org--latex-preview-region (alist-get 'from params) (alist-get 'to params)))))
    `((previews . ,(vconcat (nreverse found))))))

;;; The loop

(defun org-lsp-bridge-main ()
  (let ((coding-system-for-read 'utf-8)
        (coding-system-for-write 'utf-8))
    (while t
      (let* ((line (condition-case nil
                       (let ((org-lsp-bridge--reading t)) (read-from-minibuffer ""))
                     (error (kill-emacs 0))))
             (request (condition-case nil (json-read-from-string line) (error nil)))
             (id (alist-get 'id request))
             (method (alist-get 'method request))
             (params (alist-get 'params request))
             (fn (and method (intern-soft (concat "org-lsp-bridge-" method)))))
        (when request
          (org-lsp-bridge--send
           (condition-case err
               (if (functionp fn)
                   `((id . ,id) (result . ,(let ((inhibit-message t)) (funcall fn params))))
                 `((id . ,id) (error . ,(format "Unknown method %s" method))))
             (error `((id . ,id) (error . ,(error-message-string err)))))))))))

(org-lsp-bridge-main)
