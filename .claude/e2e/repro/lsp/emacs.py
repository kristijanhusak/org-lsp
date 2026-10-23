"""Run Org 9.8.10 in Emacs batch on a copy of a file."""
import subprocess, os, tempfile, shutil
ORG = "/root/org-mode/lisp"
def run(lisp, file=None, timeout=60, prelude=""):
    """Evaluate `lisp` in a buffer visiting `file` (org-mode), printing what it prints."""
    code = f"""(progn
 (add-to-list 'load-path "{ORG}")
 (require 'org) (require 'org-lint) (require 'org-habit nil t)
 (setq org-agenda-files nil org-id-locations-file "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/lsp/emacs-id-locations")
 {prelude}
 {f'(find-file "{file}") (org-mode)' if file else ''}
 {lisp})"""
    r = subprocess.run(["emacs", "--batch", "-Q", "--eval", code], capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
    return r.stdout, r.stderr
