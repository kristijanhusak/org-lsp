import os, sys, json
sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/lsp")
from client import *
ws = "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/diagws"; os.makedirs(ws, exist_ok=True)
p = os.path.join(ws, "d.org")
open(p, "w").write("* Beta\ntext** heading in a paragraph\nSome text ** Misplaced heading\n[[nowhere]]\n#+begin_src sh :foo 1\necho\n#+end_src\n#+begin_quote\nopen\n")
c = Client(profile=PLAIN, root=ws, options={}, name="diag")
c.open(p); c.settle(1.0, 10)
for n in c.notes("textDocument/publishDiagnostics"):
    for d in n["diagnostics"]: print(d["range"]["start"]["line"], d.get("severity"), d.get("code"), d["message"][:90])
c.shutdown()
