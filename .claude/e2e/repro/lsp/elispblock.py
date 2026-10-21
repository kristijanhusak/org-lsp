import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-elisp"))
c = Client(profile=FULL, root=ws, options=options, name="elisp", timeout=60)
g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
c.pick = lambda params: next((a["title"] for a in params.get("actions", []) if a["title"].lower().startswith("yes")), None)
arg = c.at_text(g, "(+ 1 2)", nth=1)
r = c.command("org.babel.execute", arg); c.settle(3, 30)
print("result", r, "asked", [(m, p.get("message")) for m, p in c.server_requests], "msgs", [m["message"] for m in c.messages])
lines = c.text(g).split("\n"); i = arg["position"]["line"]
print("\n".join(lines[i-1:i+6]))
print("applied", len(c.applied), c.edit_failures)
c.shutdown()
