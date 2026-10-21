"""A plain client's agenda (Agenda.org): diagnostics, tokens, code actions on
its lines, folding; running the actions."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-pag"))
c = Client(profile=PLAIN, root=ws, options=options, name="pag")
g = c.open(os.path.join(ws, "gtd.org"))
c.settle(0.3, 5)
acts = c.code_actions(g, 10, 5)
ag = [a for a in acts if a["title"] == "Agenda for the current week or day"][0]
print("agenda action:", json.dumps(ag)[:300])
c.run_action(ag)
u = [x for x in c.docs if x.endswith("Agenda.org")][0]
print("agenda uri:", u)
c.settle(1, 5)
print("diagnostics on Agenda.org:", [(d["range"]["start"]["line"], d["message"]) for d in c.diagnostics.get(u, [])])
lines = c.docs[u]["text"].split("\n")
toks = c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": u}})
dec = decode_tokens(toks["data"], c.legend, c.docs[u]["text"], c.encoding)
print("token types in agenda:", sorted({t[3] for t in dec}))
for i in (0, 1, 7):
    print("  ", repr(lines[i]), [(t[3], t[5]) for t in dec if t[0] == i])
fr = c.request("textDocument/foldingRange", {"textDocument": {"uri": u}})
print("folds in agenda:", (fr or [])[:5])
sy = c.request("textDocument/documentSymbol", {"textDocument": {"uri": u}})
print("symbols in agenda:", [s["name"] for s in sy or []][:5])
i = next(i for i, l in enumerate(lines) if "Scheduled:  TODO [#A] Write" in l)
acts = c.code_actions(u, i, 5)
print(f"code actions on entry line ({len(acts)}):", [a["title"] for a in acts])
i0 = next(i for i, l in enumerate(lines) if l.startswith("Tuesday"))
acts0 = c.code_actions(u, i0, 0)
print(f"code actions on a date line ({len(acts0)}):", [a["title"] for a in acts0][:60])
# Run "Cycle TODO state forward"
a = [a for a in acts if a["title"] == "Change the TODO state"][0]
c.picks = ["NEXT"]
print("action:", json.dumps(a)[:400])
r = c.run_action(a)
c.settle(1, 10)
print("after cycle, line:", c.docs[u]["text"].split("\n")[i], "| file:", [l for l in c.text(g).split("\n") if "Write the report" in l])
# Later
a = [a for a in c.code_actions(u, i, 5) if "go forward in time" in a["title"]][0]
c.run_action(a); c.settle(1, 10)
print("after later:", c.docs[u]["text"].split("\n")[:2], "version", c.docs[u]["version"])
# Save the agenda file? (it is a temp file the server rewrites)
print("edit failures", c.edit_failures, "messages", [m["message"] for m in c.messages])
# Type in the agenda (a user would): what happens?
c.type_at(u, 0, 0, "x")
c.settle(0.5, 5)
print("after typing into Agenda.org, diags:", c.diagnostics.get(u, [])[:2])
print("stderr", c.stderr()[-300:])
c.shutdown()
