"""LSP requests on the agenda document (full and plain): definition, hover,
preview, tokens, agendaCursor echo, agendaWidth."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for prof in (FULL, PLAIN):
    ws, options = workspace.make(os.path.join(SCRATCH, f"ws-agf-{prof}"))
    c = Client(profile=prof, root=ws, options=options, name="agf")
    c.open(os.path.join(ws, "gtd.org"))
    c.command("org.agenda.list", {}); c.settle(0.5, 5); c.run_fetches()
    u = [x for x in c.docs if "gtd.org" not in x][0]
    lines = c.docs[u]["text"].split("\n")
    i = next(i for i, l in enumerate(lines) if "Scheduled:  TODO [#A] Write" in l)
    pos = {"line": i, "character": 30}
    print("=====", prof, u.rsplit("/",1)[-1])
    print(" definition:", c.request("textDocument/definition", {"textDocument": {"uri": u}, "position": pos}))
    h = c.request("textDocument/hover", {"textDocument": {"uri": u}, "position": pos})
    print(" hover:", json.dumps(h)[:400])
    print(" preview:", json.dumps(c.request("org-lsp/preview", {"textDocument": {"uri": u}, "position": pos}))[:200])
    print(" agendaCursor as request:", c.request("org-lsp/agendaCursor", {"textDocument": {"uri": u}, "position": pos, "width": 30}))
    toks = c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": u}})
    dec = decode_tokens(toks["data"], c.legend, c.docs[u]["text"], c.encoding)
    print(" token types:", sorted({(t[3], tuple(t[4])) for t in dec})[:30])
    c.notify("org-lsp/agendaWidth", {"width": 120}); c.settle(1, 5); c.run_fetches()
    print(" width 120 line:", repr(c.docs[u]["text"].split("\n")[i]), len(c.docs[u]["text"].split("\n")[i]))
    c.shutdown()
