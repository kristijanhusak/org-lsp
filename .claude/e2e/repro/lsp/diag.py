import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from client import *
import workspace
ws, opts = workspace.make(os.path.join(SCRATCH, "ws-diag"))
c = Client(profile=PLAIN, root=ws, options=opts, name="diag")
for f in ("gtd.org","notes.org","tables.org"):
    g = os.path.join(ws, f)
    c.open(g)
c.settle(1, 10)
for u,ds in c.diagnostics.items():
    print(u)
    for d in ds: print("  ", d["range"]["start"], d.get("severity"), d.get("code"), d.get("source"), d["message"])
c.shutdown()
