"""message_requests = false (Helix): choices come first in the code actions
(kind org.answer); a rename with a choice's key answers too."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-answers"))
options["message_requests"] = False
c = Client(profile=PLAIN, root=ws, options=options, name="answers")
g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
arg = c.at_text(g, "Write the report")
c.command("org.todo", arg); c.settle(0.3, 5)
acts = c.code_actions(g, 20, 0)  # elsewhere in the file
print("actions elsewhere:", [(a["title"], a.get("kind")) for a in acts][:8])
ans = [a for a in acts if a.get("kind") == "org.answer"]
print("answer action:", json.dumps(ans[3])[:300] if len(ans) > 3 else None)
c.run_action(ans[3]); c.settle(0.5, 5)
print("->", [l for l in c.text(g).split("\n") if "Write the report" in l], [m["message"] for m in c.messages])
# Answer by rename with the key
c.command("org.todo", arg); c.settle(0.3, 5)
p = arg["position"]
prep = c.request("textDocument/prepareRename", {"textDocument": {"uri": g}, "position": p})
print("prepareRename while a key is asked:", prep)
c.rename_answer(g, p["line"], p["character"], "t")
print("-> rename 't':", [l for l in c.text(g).split("\n") if "Write the report" in l], [m["message"] for m in c.messages][-1:])
# Capture through the answers
c.command("org.capture", arg); c.settle(0.3, 5)
acts = c.code_actions(g, 0, 0)
ans = [a for a in acts if a.get("kind") == "org.answer"]
print("capture answers:", [a["title"] for a in ans])
c.run_action(ans[0]); c.settle(0.5, 5); c.run_fetches()
print("shown:", [s["uri"].rsplit('/',1)[-1] for s in c.shown])
# Agenda dispatch
c.command("org.agenda.dispatch", {}); c.settle(0.3, 5)
acts = c.code_actions(g, 0, 0)
ans = [a for a in acts if a.get("kind") == "org.answer"]
print("dispatch answers:", [a["title"] for a in ans][:6])
t = [a for a in ans if a["title"].endswith("[t]")][0]
c.run_action(t); c.settle(0.5, 5)
print("shown:", [s["uri"].rsplit('/',1)[-1] for s in c.shown], "edit failures", c.edit_failures)
c.shutdown()
