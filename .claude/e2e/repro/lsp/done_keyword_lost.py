"""lsp-7: in the agenda, an entry marked done loses its keyword on its line
when the parent's statistics cookie is counted again and the parent's tags
move (they were not in their column)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
d = os.path.join(SCRATCH, "ws-lsp7"); os.makedirs(d, exist_ok=True)
p = os.path.join(d, "a.org")
for view, text in (("org.agenda.todos", "* TODO Project [0/2] :work:\n** TODO Build it\n** TODO Ship it\n"),
                   ("org.agenda.list", "* TODO Project [0/2] :work:\n** TODO Build it\n** TODO Ship it\n   SCHEDULED: <2026-10-21 Wed>\n")):
    open(p, "w").write(text)
    c = Client(profile=FULL, root=d, options={"org_agenda_files": [p], "org_agenda_span": "day"}, name="lsp7")
    c.open(p)
    c.command(view, {}); c.settle(0.5, 5); c.run_fetches()
    u = [x for x in c.docs if x.startswith("org-agenda")][0]
    lines = c.docs[u]["text"].split("\n")
    i = next(i for i, l in enumerate(lines) if "TODO Ship it" in l)
    c.command("org.todo.next", c.at(u, i, 2)); c.settle(1, 10); c.run_fetches()
    print(view, "before:", repr(lines[i]))
    print(view, "after: ", repr(c.docs[u]["text"].split("\n")[i]))
    print("   file:", c.text(p).split("\n")[:3])
    c.shutdown()
