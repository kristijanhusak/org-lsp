import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for delta in (0, 1, 4, -1, -5):
    ws, options = workspace.make(os.path.join(SCRATCH, "ws-cookie"))
    # A wrong cookie, to see whether it is recounted.
    p = os.path.join(ws, "gtd.org"); t = open(p).read().replace("Groceries [33%]", "Groceries [10%]"); open(p, "w").write(t)
    c = Client(profile=FULL, root=ws, options=options, name="cookie")
    g = c.open(p)
    arg = c.at_text(g, "[10%]", delta=delta)
    try:
        r = c.command("org.checkbox.toggle", arg)
        c.apply_result(arg, r); c.settle(1, 10)
    except ResponseError as e:
        r = str(e)
    lines = c.text(g).split("\n")
    i = next(i for i, l in enumerate(lines) if "Groceries" in l)
    print(delta, "->", r, lines[i], "|", lines[i+1].strip(), "|", lines[i+2].strip())
    c.shutdown()
