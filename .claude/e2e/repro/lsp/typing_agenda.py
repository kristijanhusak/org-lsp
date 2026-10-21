"""Does the agenda follow a change typed in a file? With and without an
edit made from the agenda first; file open before or after the agenda."""
import json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for prof in (FULL, PLAIN):
    for edit_first in (False, True):
        for open_first in (True, False):
            ws, options = workspace.make(os.path.join(SCRATCH, f"ws-typag-{prof}"))
            c = Client(profile=prof, root=ws, options=options, name="typag")
            g = os.path.join(ws, "gtd.org")
            if open_first: c.open(g)
            c.command("org.agenda.list", {}); c.settle(0.5, 5); c.run_fetches()
            u = [x for x in c.docs if "gtd.org" not in x][0]
            if edit_first:
                lines = c.docs[u]["text"].split("\n")
                i = next(i for i, l in enumerate(lines) if "Write the report" in l)
                c.command("org.priority.down", c.at(u, i, 2)); c.settle(1, 5); c.run_fetches()
            if not open_first: c.open(g); c.settle(0.3, 3)
            p = c.find(g, "Call the plumber")
            c.type_at(g, p["line"], p["character"], "URGENTLY ")
            c.settle(2, 8); c.run_fetches(); c.settle(1, 5); c.run_fetches()
            ok = "URGENTLY" in c.docs[u]["text"]
            c.save(g); c.settle(2, 8); c.run_fetches(); c.settle(0.5, 5); c.run_fetches()
            ok2 = "URGENTLY" in c.docs[u]["text"]
            print(f"{prof:5} edit_first={edit_first!s:5} open_first={open_first!s:5}: follows typing {ok}, after save {ok2}")
            c.shutdown()
