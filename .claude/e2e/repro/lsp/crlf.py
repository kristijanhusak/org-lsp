"""lsp-11: formatting a CRLF file: each edit's range ends one character past
the end of its line (taking the \\r) and its text ends with \\r, so a
client that reads positions as LSP says (\\r\\n ends a line; a character
past the end is the end) gets \\r\\r\\n."""
import os, sys, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
d = os.path.join(SCRATCH, "ws-crlf"); os.makedirs(d, exist_ok=True)
p = os.path.join(d, "crlf.org")
open(p, "w", newline="").write("* TODO A :x:\r\n|a|bb|\r\n|1|2|\r\n* B :y:\r\n   :PROPERTIES:\r\n   :Foo: 1\r\n   :END:\r\n")
c = Client(profile=PLAIN, root=d, options={}, name="crlf")
u = c.open(p)
text = c.text(p)
lines = text.split("\r\n")
for method, extra in (("textDocument/formatting", {}),
                      ("textDocument/rangeFormatting", {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 7, "character": 0}}})):
    f = c.request(method, dict({"textDocument": {"uri": u}, "options": {"tabSize": 8, "insertSpaces": True}}, **extra))
    print(method)
    for e in f or []:
        ln = e["range"]["start"]["line"]
        print(f"   line {ln} (length {len(lines[ln])}): range {e['range']['start']['character']}-{e['range']['end']['line']}:{e['range']['end']['character']} newText {e['newText']!r}")
    print("   applied as LSP says:", repr(apply_text_edits(text, f or [], c.encoding)))
# A table command through its result edits
r = c.command("org.table.next.field", c.at(p, 1, 1))
print("org.table.next.field:", r)
print("   applied:", repr(apply_text_edits(text, r["edits"], c.encoding)))
c.shutdown()
