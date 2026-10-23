import emacs, sys, os
W = sys.argv[1]
cmd = sys.argv[2] if len(sys.argv) > 2 else "(org-agenda-list)"
prelude = f'''(require 'org-agenda) (require 'org-habit)
(setq org-agenda-files (list "{W}/gtd.org" "{W}/notes.org"))
(setq org-todo-keywords (quote ((sequence "TODO" "NEXT" "WAIT(w@)" "|" "DONE(d!)" "CANCELLED(c@)"))))
(setq org-log-done 'time org-log-into-drawer t)
(setq org-cite-global-bibliography (list "{W}/refs.bib"))'''
o, e = emacs.run(f'''{cmd} (princ (buffer-substring-no-properties (point-min) (point-max)))''', None, prelude=prelude)
print(o); print(e[-1500:], file=sys.stderr)
