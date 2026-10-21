"""didChange storm: random incremental edits (with CJK and emoji), then
check that the server's text is the client's: tokens of the server's view
against tokens after a full resync."""
import os, sys, random, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
seed = int(sys.argv[1]) if len(sys.argv) > 1 else 7
n = int(sys.argv[2]) if len(sys.argv) > 2 else 500
enc = sys.argv[3] if len(sys.argv) > 3 else "utf-16"
ws, options = workspace.make(os.path.join(SCRATCH, "ws-storm"))
c = Client(profile=PLAIN, root=ws, options=options, encodings=[enc], name="storm")
g = c.open(os.path.join(ws, os.environ.get("STORM_FILE", "tables.org"))); c.settle(0.3, 5)
random.seed(seed)
log = []
for i in range(n):
    cur = c.text(g)
    lines = cur.split("\n")
    ln = random.randrange(len(lines)); ch = random.randrange(len(lines[ln]) + 1)
    so = sum(len(l) + 1 for l in lines[:ln]) + ch
    p0 = position_of(cur, so, c.encoding)
    if random.random() < 0.6:
        s = random.choice(["a", "日", "🎉", "*", " ", "\n", ":tag:", "|", "é"])
        c.type_at(g, p0["line"], p0["character"], s); log.append(("ins", p0, s))
    else:
        eo = min(sum(len(l) + 1 for l in lines[:ln]) + len(lines[ln]), so + random.randrange(4))
        p1 = position_of(cur, eo, c.encoding)
        c.change(g, [{"range": {"start": p0, "end": p1}, "newText": ""}]); log.append(("del", p0, p1))
    if i % 25 == 24 or i == n - 1:
        a = c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": g}})
        b_text = c.text(g)
        c2 = Client(profile=PLAIN, root=ws, options=options, encodings=[enc], name="storm-ref")
        u2 = c2.open(g, text=b_text)
        b = c2.request("textDocument/semanticTokens/full", {"textDocument": {"uri": u2}})
        c2.shutdown()
        if a["data"] != b["data"]:
            print(f"DIVERGED by edit {i}; last edits:", log[-25:])
            da = decode_tokens(a["data"], c.legend, b_text, c.encoding); db = decode_tokens(b["data"], c.legend, b_text, c.encoding)
            for x, y in zip(da, db):
                if x != y:
                    print("  first difference: server", x, "fresh", y); break
            print("  text line:", repr(b_text.split("\n")[min(x[0], y[0])]))
            break
else:
    print("in sync after", n, "edits")
c.shutdown()
