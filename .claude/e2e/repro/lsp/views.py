"""Agenda menu views against Emacs: tags match (m, M), search (s), stuck (#),
TODO list of a keyword; bulk action."""
import json, os, sys, re, difflib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace, emacs_agenda_lib as E
def norm(t):
    t = re.sub(r"^ +\d+:\d+\.+ now .*$", "NOW", t, flags=re.M)
    t = re.sub(r" +(:[^ ]+:)$", r" \1", t, flags=re.M)
    t = re.sub(r"^Press .*\n( {20,}.*\n)?", "", t, flags=re.M)
    return t.rstrip("\n") + "\n"
ws, options = workspace.make(os.path.join(SCRATCH, "ws-views"))
cases = [("m work", ["m", "work"], '(org-tags-view nil "work")'),
         ("M work", ["M", "work"], '(org-tags-view t "work")'),
         ("m TODO=\"NEXT\"|urgent", ["m", 'TODO="NEXT"|urgent'], '(org-tags-view nil "TODO=\\"NEXT\\"|urgent")'),
         ("m Effort>0:30", ["m", "Effort>\"0:30\""], '(org-tags-view nil "Effort>\\"0:30\\"")'),
         ("s report", ["s", "report"], '(org-search-view nil "report")'),
         ("s +plumber -call", ["s", "+plumber -call"], '(org-search-view nil "+plumber -call")'),
         ("S report", ["S", "report"], '(org-search-view t "report")'),
         ("# stuck", ["#"], '(org-agenda-list-stuck-projects)'),
         ("t todo list", ["t"], '(org-todo-list)')]
c = Client(profile=FULL, root=ws, options=options, name="views")
for name, keys, form in cases:
    r, steps = c.run("org.agenda.dispatch", {}, answers=keys)
    c.settle(0.5, 5); c.run_fetches()
    u = [x for x in c.docs if x.startswith("org-agenda")][0]
    s = norm(c.docs[u]["text"]); e = norm(E.agenda(ws, form))
    d = "".join(difflib.unified_diff(e.splitlines(True), s.splitlines(True), "emacs", "server", n=1))
    print(f"== {name}: {'SAME' if not d else 'DIFF'}")
    if d: print(d)
c.shutdown()
