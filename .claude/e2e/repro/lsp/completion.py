"""Completion in every context, on a line of its own."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
profile = sys.argv[1] if len(sys.argv) > 1 else PLAIN
ws, options = workspace.make(os.path.join(SCRATCH, f"ws-compl-{profile}"))
c = Client(profile=profile, root=ws, options=options, name=f"compl-{profile}")
g = c.open(os.path.join(ws, "gtd.org"))
orig = c.text(g)
base = c.find(g, "   A paragraph.")["line"] + 1
def at(s, trigger=None, line_prefix=""):
    c.change(g, text=orig)
    c.type_at(g, base, 0, line_prefix + s + "\n")
    col = len((line_prefix + s).encode("utf-16-le")) // 2
    ctx = {"triggerKind": 2, "triggerCharacter": trigger} if trigger else {"triggerKind": 1}
    r = c.request("textDocument/completion", {"textDocument": {"uri": g}, "position": {"line": base, "character": col}, "context": ctx})
    items = r.get("items", r) if isinstance(r, dict) else (r or [])
    return items
cases = [("#+", "+"), ("#+TI", None), ("#+begin_", None), ("#+BEGIN_", None), ("#+begin_src ", None), ("#+begin_src py", None),
         (":", ":"), (":LOG", None), ("[[", "["), ("[[id:", None), ("[[id:0a1b", None), ("[[file:", None), ("[[file:roam/", "/"),
         ("[[*", "*"), ("[[*Wri", None), ("[[#", "#"), ("[[roam:", None), ("[[https:", None), ("\\alp", None), ("\\", "\\"),
         ("[cite:@", None), ("[cite:@kn", None), ("[[file:notes.org::*", "*"), ("<", None), ("[fn:", None), ("#+STARTUP: ", None),
         ("#+STARTUP: ov", None), ("#+OPTIONS: ", None), ("src_", None), ("call_", None), ("#+CALL: ", None), ("#+INCLUDE: ", None),
         ("#+FILETAGS: :", ":"), ("{{{", None), ("#+PROPERTY: ", None), ("#+TODO: ", None)]
for s, trig in cases:
    try:
        items = at(s, trig)
        labels = [i["label"] for i in items]
        print(f"{s!r:24} {len(labels):4} {labels[:12]}")
        if items and items[0].get("textEdit"):
            print("      textEdit:", items[0]["textEdit"])
    except ResponseError as err:
        print(f"{s!r:24} ERROR {err}")
# Headline contexts
for s, trig in [("** ", None), ("** T", None), ("** TODO Something :", ":"), ("** TODO Something :wo", None), ("** TODO [#", "#")]:
    items = at(s, trig)
    print(f"{s!r:24} {len(items):4} {[i['label'] for i in items][:12]}")
# In a property drawer
c.change(g, text=orig)
p = c.find(g, "   :Effort:")
c.type_at(g, p["line"], 0, "   :\n")
r = c.request("textDocument/completion", {"textDocument": {"uri": g}, "position": {"line": p["line"], "character": 4}, "context": {"triggerKind": 2, "triggerCharacter": ":"}})
items = r.get("items", r) if isinstance(r, dict) else (r or [])
print("property key   ", [i["label"] for i in items])
c.change(g, text=orig)
p = c.find(g, "   :Effort:   1:00")
r = c.request("textDocument/completion", {"textDocument": {"uri": g}, "position": {"line": p["line"], "character": 14}, "context": {"triggerKind": 1}})
items = r.get("items", r) if isinstance(r, dict) else (r or [])
print("property value ", [i["label"] for i in items])
print(c.stderr()[-1000:])
c.shutdown()
