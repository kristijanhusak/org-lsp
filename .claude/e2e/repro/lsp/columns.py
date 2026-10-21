"""Column view: org.columns, then its commands once the view has come."""
import json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-columns"))
c = Client(profile=FULL, root=ws, options=options, name="columns")
g = c.open(os.path.join(ws, "gtd.org"))
c.settle(0.3, 5)
arg = c.at_text(g, "Write the report")
# Right away, before Emacs answers.
r = c.command("org.columns", arg)
early = c.command("org.columns.edit", dict(arg, column=1))
print("edit right away (column 1, TODO):", early)
c.pump(lambda: c.notes("org-lsp/columns"), 20)
v = c.notes("org-lsp/columns")[-1]
print("columns:", v["columns"], "header:", v.get("header"))
print("lines:", v["lines"][:3])
for col, answers in ((1, []), (2, ["0:30"]), (0, ["New title"])):
    r, steps = c.run("org.columns.edit", dict(arg, column=col), answers=answers)
    c.settle(1.5, 15)
    print(f"edit column {col}:", json.dumps(steps)[:300])
    print("   line now:", c.text(g).split("\n")[arg["position"]["line"]][:80], "| effort:", [l for l in c.text(g).split("\n")[11:16] if "Effort" in l])
print(c.stderr()[-500:])
c.shutdown()
