"""How the time of opening and of the first requests grows with the size
of a file (headlines with a tag, a planning line, a link)."""
import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
d = os.path.join(SCRATCH, "ws-scale"); os.makedirs(d, exist_ok=True)
kind = sys.argv[1] if len(sys.argv) > 1 else "full"
for n in [int(x) for x in sys.argv[2:]] or (500, 1000, 2000, 4000):
    p = os.path.join(d, f"big{n}.org")
    with open(p, "w") as fh:
        for i in range(n):
            if kind == "full":
                fh.write(f"* TODO Heading {i} :t{i % 7}:\n  SCHEDULED: <2026-10-{(i % 28) + 1:02d} Wed>\n  Text {i} with [[https://e.com/{i}][link]].\n")
            elif kind == "plain":
                fh.write(f"* Heading {i}\n  Text {i}.\n")
            elif kind == "text":
                fh.write(f"Line {i} of plain text.\n")
    c = Client(profile=PLAIN, root=d, options={"org_agenda_files": []}, name="scale", timeout=300)
    t0 = time.time()
    u = c.open(p)
    try:
        c.request("textDocument/foldingRange", {"textDocument": {"uri": u}}, timeout=300)
        t1 = time.time()
        c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": u}}, timeout=300)
        t2 = time.time()
        c.type_at(u, 1, 0, "x")
        c.request("textDocument/hover", {"textDocument": {"uri": u}, "position": {"line": 0, "character": 0}}, timeout=300)
        t3 = time.time()
        print(f"{kind} {n} entries ({sum(1 for _ in open(p))} lines): open+folding {t1-t0:.2f}s tokens {t2-t1:.2f}s edit+hover {t3-t2:.2f}s", flush=True)
    except Timeout as e:
        print(kind, n, "TIMEOUT", e, flush=True)
        c.kill(); continue
    c.shutdown()
