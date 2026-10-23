import subprocess, json
ORG = "/root/org-mode/lisp"
SCR = "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/lsp"
def agenda(ws, form):
    code = f'''(progn (add-to-list 'load-path "{ORG}") (require 'org) (require 'org-agenda) (require 'org-habit)
 (setq org-id-locations-file "{SCR}/emacs-id-locations")
 (setq org-agenda-files (list "{ws}/gtd.org" "{ws}/notes.org"))
 (setq org-todo-keywords (quote ((sequence "TODO" "NEXT" "WAIT(w@)" "|" "DONE(d!)" "CANCELLED(c@)"))))
 (setq org-agenda-window-setup 'current-window)
 {form}
 (princ (buffer-substring-no-properties (point-min) (point-max))))'''
    r = subprocess.run(["emacs", "--batch", "-Q", "--eval", code], capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
    return r.stdout
