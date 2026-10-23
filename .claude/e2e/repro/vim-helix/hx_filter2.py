"""Helix: the category filter in the week agenda and in the TODO list."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
h = Helix("filt", {"files": {"a.org": "* TODO Write report\nSCHEDULED: <2026-05-18 Mon 09:00>\n", "b.org": "* TODO Bee\nSCHEDULED: <2026-05-18 Mon>\n"}})
try:
    A = os.path.join(h.directory, "a.org"); h.start(A)
    ag = lambda: h.log.documents.get(h.agenda()) or ""
    for view, how in (("week", lambda: act(h, "Agenda for the current week or day")), ("todo", lambda: act(h, "List of all TODO entries"))):
        h.open(A); how()
        n = next(i for i, l in enumerate(ag().split("\n"), 1) if "Bee" in l); goto(h, n, 3)
        act(h, "Change the TODO state"); h.settle(); print(view, "after t:", [l for l in ag().split("\n") if "Write" in l or "Bee" in l])
        act(h, "Agenda: filter by the line's category"); time.sleep(1); h.settle()
        print(view, "text:", [l for l in ag().split("\n") if "Write" in l or "Bee" in l], "| msgs:", h.messages().strip().replace("\n", " | "))
        print(view, "screen:", [r.strip() for r in h.screen() if "Write" in r or "Bee" in r])
        act(h, "Agenda: remove all filters")
finally:
    h.stop()
