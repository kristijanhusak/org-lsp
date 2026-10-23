import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-roamlink"))
with open(os.path.join(ws, "roam/beta.org"), "a") as f: f.write("See [[roam:Alpha]] and [[roam:Nope]].\n")
c = Client(profile=FULL, root=ws, options=options, name="roamlink")
g = c.open(os.path.join(ws, "gtd.org")); b = c.open(os.path.join(ws, "roam/beta.org"))
c.settle(0.5, 5)
for path, needle in ((g, "[[roam:Beta]]"), (b, "[[roam:Alpha]]"), (b, "[[roam:Nope]]")):
    pos = c.find(path, needle, delta=4)
    d = c.request("textDocument/definition", {"textDocument": {"uri": path}, "position": pos})
    print(path.rsplit('/',1)[-1], needle, "definition:", d)
    try:
        r, steps = c.run("org.open", c.at(path, pos["line"], pos["character"]), answers=[])
        print("  open:", json.dumps(steps)[:300], "shown:", [s["uri"].rsplit('/',1)[-1] for s in c.shown][-1:])
    except ResponseError as e:
        print("  open error", e)
print("diags beta", [d["message"] for d in c.diagnostics.get(b, [])])
print("diags gtd", [d["message"] for d in c.diagnostics.get(g, [])])
c.shutdown()
