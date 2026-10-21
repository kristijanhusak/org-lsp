"""Agenda edits to files the client does not have open: applyEdit for an
unopened file (the client opens it, as editors load a buffer), auto_save on
and off (org-lsp/save), typing in a file while the agenda is open."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for prof in (FULL, PLAIN):
    for auto in (True, False):
        ws, options = workspace.make(os.path.join(SCRATCH, f"ws-notopen-{prof}"))
        options["auto_save"] = auto
        c = Client(profile=prof, root=ws, options=options, name="notopen")
        c.command("org.agenda.list", {}); c.settle(0.5, 5); c.run_fetches()
        u = [x for x in c.docs if "gtd.org" not in x][0]
        lines = c.docs[u]["text"].split("\n")
        i = next(i for i, l in enumerate(lines) if "Write the report" in l)
        c.command("org.todo.next", c.at(u, i, 2) ) if prof == FULL else c.command("org.priority.down", c.at(u, i, 2))
        c.settle(1, 5); c.run_fetches()
        g = os.path.join(ws, "gtd.org")
        disk = open(g).read()
        ln = [l for l in disk.split("\n") if "Write the report" in l][0]
        print(f"{prof} auto_save={auto}: applyEdits {[(p['edit'].get('documentChanges') or [{}])[0].get('textDocument') for p, ok, why in c.applied]} | saves {c.notes('org-lsp/save')} | on disk: {ln.strip()[:40]} | buffer: {[l for l in c.text(g).split(chr(10)) if 'Write the report' in l][0][:40]}")
        # Type in the file: the agenda follows
        c.type_at(g, 20, 3, "")
        p = c.find(g, "Call the plumber")
        c.type_at(g, p["line"], p["character"], "URGENTLY ")
        c.settle(1.5, 5); c.run_fetches()
        print("   agenda follows typing:", any("URGENTLY" in l for l in c.docs[u]["text"].split("\n")), "| failures", c.edit_failures)
        c.shutdown()
