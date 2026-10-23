import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, f"ws-compl2"))
c = Client(profile=PLAIN, root=ws, options=options, name=f"compl2")
def comp(path, s, line=None):
    u = c.open(path)
    orig = c.text(u)
    ln = line if line is not None else len(orig.split("\n")) - 1
    c.type_at(u, ln, 0, s + "\n")
    r = c.request("textDocument/completion", {"textDocument": {"uri": u}, "position": {"line": ln, "character": len(s)}, "context": {"triggerKind": 1}})
    c.change(u, text=orig)
    items = r.get("items", r) if isinstance(r, dict) else (r or [])
    return [i["label"] for i in items], items
l,_ = comp(os.path.join(ws,"gtd.org"), "#+begin_", 74)
print([x for x in l if 'egin' in x.lower() or 'END' in x.upper()][:20], len(l))
print(l)
a = os.path.join(ws, "roam/alpha.org")
for s in ["[[roam:", "[[roam:Be", "Bet", "[[id:", "[[id:0a1b"]:
    l, items = comp(a, s, 7)
    print(repr(s), l[:10], items[:1])
c.shutdown()
