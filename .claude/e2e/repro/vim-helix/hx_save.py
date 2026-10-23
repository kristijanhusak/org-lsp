"""Helix: does `Save all Org files` write a buffer Helix has open? Does the category filter hide other lines?"""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
h = Helix("save", {"files": {"a.org": "* TODO Write report\nSCHEDULED: <2026-05-18 Mon 09:00>\n", "b.org": "* TODO Bee\nSCHEDULED: <2026-05-18 Mon>\n"}})
try:
    A = os.path.join(h.directory, "a.org"); B = os.path.join(h.directory, "b.org"); h.start(A)
    act(h, "Agenda for the current week or day")
    ag = lambda: h.log.documents.get(h.agenda()) or ""
    for who in ("Write report", "Bee"):
        n = next(i for i, l in enumerate(ag().split("\n"), 1) if who in l); goto(h, n, 3)
        act(h, "Change the TODO state")
    print("buffers:", [l for l in ag().split("\n") if "DONE" in l])
    act(h, "Save all Org files"); time.sleep(2); h.settle()
    print("messages:", h.messages().strip().replace("\n", " | "))
    print("a.org disk:", open(A).read().split("\n")[0], "| b.org disk:", open(B).read().split("\n")[0])
    print("a.org in Helix:", (h.text(A) or "").split("\n")[0])
    print("screen status:", [r for r in h.screen() if "[+]" in r][:3])
    n = next(i for i, l in enumerate(ag().split("\n"), 1) if "Bee" in l); goto(h, n, 3)
    act(h, "Agenda: filter by the line's category"); time.sleep(1); h.settle()
    print("after filter b:", [l for l in ag().split("\n") if "Write" in l or "Bee" in l])
    print("screen:", [r.rstrip() for r in h.screen() if "Write" in r or "Bee" in r])
finally:
    h.stop()
