#!/bin/bash
# em.sh FILE LINE COL ELISP : run ELISP at LINE/COL of FILE in Emacs Org, print buffer and messages
f=$1 l=$2 c=$3 e=$4
timeout 60 emacs --batch -Q -L /root/org-mode/lisp --eval "(progn (require 'org) (setq org-log-done 'time) (find-file \"$f\") (org-mode) (goto-char (point-min)) (forward-line (1- $l)) (move-to-column (1- $c)) $e (princ (format \"%s\n== point %d:%d\n\" (buffer-string) (line-number-at-pos) (1+ (current-column)))))" 2>&1
