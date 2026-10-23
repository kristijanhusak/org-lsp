import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from client import *
import workspace
ws, opts = workspace.make(os.path.join(SCRATCH, "ws-caps"))
for prof in (PLAIN, FULL, BARE):
    c = Client(profile=prof, root=ws, options=opts, name="caps")
    caps = dict(c.init_result["capabilities"])
    exp = caps.pop("experimental", {})
    caps["semanticTokensProvider"]["legend"] = "..."
    caps["executeCommandProvider"] = len(caps["executeCommandProvider"]["commands"])
    print(prof, json.dumps(caps, indent=None))
    print(" exp", {k: (v if k != "org" else "manifest") for k, v in exp.items()})
    c.shutdown()
