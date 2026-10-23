import json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-roamdiag"))
c = Client(profile=PLAIN, root=ws, options=options, name="roamdiag")
g = c.open(os.path.join(ws, "gtd.org"))
t0=time.time()
c.settle(3, 10)
for (t, m, p) in c.notifications:
    if m == "textDocument/publishDiagnostics":
        print(round(t-t0,2), p.get("version"), [d["message"][:40] for d in p["diagnostics"]])
c.shutdown()
