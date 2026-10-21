"""org-protocol: `org-lsp protocol URL` reaching a running server (plain
and full clients): store-link, capture."""
import json, os, sys, subprocess, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for prof in (FULL, PLAIN):
    ws, options = workspace.make(os.path.join(SCRATCH, f"ws-proto-{prof}"))
    c = Client(profile=prof, root=ws, options=options, name="proto")
    g = c.open(os.path.join(ws, "gtd.org")); c.settle(1.5, 5)
    env = dict(os.environ, XDG_STATE_HOME=os.path.join(SCRATCH, "xdg-state"), XDG_CONFIG_HOME=os.path.join(SCRATCH, "xdg-config"))
    def proto(url):
        r = subprocess.run([SERVER, "protocol", url], capture_output=True, text=True, env=env, timeout=20)
        c.settle(1, 5); c.run_fetches()
        return r.returncode, r.stdout.strip(), r.stderr.strip()[:200]
    print("=====", prof)
    print(" store-link:", proto("org-protocol://store-link?url=https%3A%2F%2Fexample.com%2Fp&title=Example%20Page"))
    n = len(c.shown)
    print(" capture:", proto("org-protocol://capture?template=t&url=https%3A%2F%2Fexample.com&title=Ex&body=Selected"))
    print("   shown:", [s["uri"].rsplit("/",1)[-1] for s in c.shown[n:]], "runCommand notes:", c.notes("org-lsp/runCommand")[-1:], "asked:", [p.get("message") for m, p in c.server_requests if m == "window/showMessageRequest"][-1:])
    print("   messages:", [m["message"] for m in c.messages][-3:])
    cap = [u for u in c.docs if "CAPTURE" in u]
    if cap: print("   capture buffer:", repr(c.docs[cap[-1]]["text"]))
    r = c.command("org.link.insert.last", c.at_text(g, "A paragraph.", delta=12)); c.settle(1, 5)
    print("   insert last stored link:", [l for l in c.text(g).split("\n") if "A paragraph" in l])
    c.shutdown()
