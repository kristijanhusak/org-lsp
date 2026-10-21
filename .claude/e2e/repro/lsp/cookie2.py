import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-cookie2"))
p = os.path.join(ws, "gtd.org"); t = open(p).read().replace("Project Alpha [1/3]", "Project Alpha [0/3]").replace("Some text with", "Items [5/9]\n   - [X] a\n   - [ ] b\n   Some text with"); open(p, "w").write(t)
c = Client(profile=FULL, root=ws, options=options, name="cookie2")
g = c.open(p)
for needle in ("[0/3]", "[5/9]"):
    arg = c.at_text(g, needle, delta=1)
    r = c.command("org.checkbox.toggle", arg); c.apply_result(arg, r); c.settle(1, 10)
    l = c.text(g).split("\n")[arg["position"]["line"]:arg["position"]["line"]+3]
    print(needle, r, l)
c.shutdown()
