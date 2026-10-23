import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from client import *
import workspace
for prof in (PLAIN, FULL):
    ws, opts = workspace.make(os.path.join(SCRATCH, "ws-smoke-"+prof))
    c = Client(profile=prof, root=ws, options=opts, name="smoke-"+prof)
    caps = c.init_result["capabilities"]
    print(prof, "encoding", c.encoding, "keys", sorted(caps.keys()))
    print(" legend types", c.legend["tokenTypes"][:40])
    print(" exp keys", sorted((caps.get("experimental") or {}).keys()))
    g = os.path.join(ws, "gtd.org")
    c.open(g)
    c.settle(1, 10)
    print(" diags", json.dumps(c.diagnostics.get(c.uri(g)))[:600])
    print(" msgs", c.messages, "notes", sorted(set(m for _,m,_ in c.notifications)))
    print(" exit", c.shutdown())
    print(" stderr", c.stderr()[:500])
