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
;; question is an error.

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
    (require 'org)
    (require 'ox)
    (require 'ox-publish)
    ;; Opening is the editor's: the file is reported instead.
    (advice-add 'org-open-file :override
                (lambda (path &rest _) (setq org-lsp-bridge--opened (expand-file-name path)) path))
    `((org . ,(org-version)) (emacs . ,emacs-version))))

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
         (action (unless (eq first ?P) (org-lsp-bridge--action first second)))
         (org-agenda-files (or (append (alist-get 'agenda_files params) nil) org-agenda-files)))
    (setq org-lsp-bridge--opened nil)
    (unless (or action (eq first ?P))
      (error "No export command under %s %s" (string first) (if second (string second) "")))
    (org-lsp-bridge--in-document params
      (let ((result
             (if (eq first ?P)
                 (pcase second
                   (?f (org-publish-current-file force) 'published)
                   (?p (org-publish-current-project force) 'published)
                   (?x (org-publish (assoc (alist-get 'project params) org-publish-project-alist) force)
                       'published)
                   (?a (org-publish-all force) 'published)
                   (_ (error "No publishing command under %s" (string second))))
               (funcall action nil subtree nil body))))
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
    org-babel-execute-subtree org-babel-remove-result-one-or-many)
  "The commands the server may run on a document.")

(defvar org-lsp-bridge--message nil
  "What Emacs said last while a command ran.")

(defun org-lsp-bridge--record-message (old format &rest args)
  (let ((said (apply old format args)))
    (when (and format said (not (string-empty-p said)))
      (setq org-lsp-bridge--message said))
    said))

(defun org-lsp-bridge-run (params)
  "Run the command PARAMS names in its document, at its point: the text it
leaves, where the point is then, and what Emacs said last."
  (let ((command (intern (alist-get 'command params))))
    (unless (memq command org-lsp-bridge--commands)
      (error "org-lsp does not run %s" command))
    (setq org-lsp-bridge--message nil)
    (org-lsp-bridge--in-document params
      (advice-add 'message :around #'org-lsp-bridge--record-message)
      (unwind-protect
          (let ((current-prefix-arg nil))
            (call-interactively command))
        (advice-remove 'message #'org-lsp-bridge--record-message))
      `((text . ,(buffer-substring-no-properties (point-min) (point-max)))
        (point . ,(point))
        (message . ,org-lsp-bridge--message)))))

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
