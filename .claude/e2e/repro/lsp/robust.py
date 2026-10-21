#!/usr/bin/env python3
"""Robustness: invalid params, unknown commands and methods, documents not
open, didChange storms, refused edits, files changed on disk, CRLF,
malformed JSON, cancel, requests after shutdown, exit without shutdown."""
import json
import os
import random
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import FULL, PLAIN, SCRATCH, Client, ResponseError, Timeout, apply_text_edits, decode_tokens, uri_of  # noqa
import workspace  # noqa


def try_req(c, method, params, timeout=10):
    try:
        r = c.request(method, params, timeout=timeout)
        return "ok", json.dumps(r)[:200]
    except ResponseError as e:
        return "error", f"{e.code} {e.message}"[:200]
    except Timeout as e:
        return "TIMEOUT", str(e)


def section(t):
    print("\n#####", t)


ws, options = workspace.make(os.path.join(SCRATCH, "ws-robust"))
c = Client(profile=FULL, root=ws, options=options, name="robust")
g = c.open(os.path.join(ws, "gtd.org"))
c.settle(0.3, 5)

section("invalid params and unknown things")
for method, params in [
    ("workspace/executeCommand", {"command": "org.nonexistent", "arguments": []}),
    ("workspace/executeCommand", {"command": "org.todo.next", "arguments": []}),
    ("workspace/executeCommand", {"command": "org.todo.next", "arguments": [42]}),
    ("workspace/executeCommand", {"command": "org.todo.next", "arguments": [{"textDocument": {"uri": "file:///nope.org"}, "position": {"line": 0, "character": 0}}]}),
    ("workspace/executeCommand", {"command": "org.todo.next", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 9999, "character": 0}}]}),
    ("workspace/executeCommand", {"command": "org.todo.next", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 10, "character": 9999}}]}),
    ("workspace/executeCommand", {"command": "org.todo.next", "arguments": [{"textDocument": {"uri": g}, "position": {"line": -1, "character": 0}}]}),
    ("workspace/executeCommand", {"command": "org.schedule", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 10, "character": 0}, "input": "not a date at all ???"}]}),
    ("workspace/executeCommand", {"command": "org.schedule", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 10, "character": 0}, "input": 12}]}),
    ("workspace/executeCommand", {"command": "org.todo", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 10, "character": 0}, "input": "Z"}]}),
    ("workspace/executeCommand", {"command": "org.priority", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 10, "character": 0}, "input": "Q"}]}),
    ("workspace/executeCommand", {"command": "org.table.create", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 73, "character": 0}, "input": "0x0"}]}),
    ("workspace/executeCommand", {"command": "org.subtree.clone", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 52, "character": 0}, "input": "-3"}]}),
    ("workspace/executeCommand", {"command": "org.columns.edit", "arguments": [{"textDocument": {"uri": g}, "position": {"line": 10, "character": 0}, "column": 99}]}),
    ("workspace/executeCommand", None),
    ("textDocument/hover", {"textDocument": {"uri": "file:///nope.org"}, "position": {"line": 0, "character": 0}}),
    ("textDocument/semanticTokens/full", {"textDocument": {"uri": "file:///nope.org"}}),
    ("textDocument/semanticTokens/range", {"textDocument": {"uri": g}, "range": {"start": {"line": 50, "character": 0}, "end": {"line": 10, "character": 0}}}),
    ("textDocument/semanticTokens/range", {"textDocument": {"uri": g}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 99999, "character": 0}}}),
    ("textDocument/completion", {"textDocument": {"uri": g}, "position": {"line": 99999, "character": 5}}),
    ("textDocument/definition", {"textDocument": {"uri": g}, "position": {"line": 10, "character": 99999}}),
    ("textDocument/foldingRange", {"textDocument": {"uri": "untitled:Untitled-1"}}),
    ("textDocument/codeAction", {"textDocument": {"uri": g}, "range": {"start": {"line": 99999, "character": 0}, "end": {"line": 99999, "character": 0}}, "context": {"diagnostics": []}}),
    ("textDocument/rename", {"textDocument": {"uri": g}, "position": {"line": 10, "character": 72}, "newName": ""}),
    ("textDocument/rename", {"textDocument": {"uri": g}, "position": {"line": 10, "character": 72}, "newName": "has space"}),
    ("textDocument/formatting", {"textDocument": {"uri": g}}),
    ("workspace/symbol", {"query": "\\(\\["}),
    ("workspace/textDocumentContent", {"uri": "org-agenda:/Nothing"}),
    ("org-lsp/preview", {"textDocument": {"uri": g}, "position": {"line": 0, "character": 0}}),
    ("org-lsp/startupFolds", {"textDocument": {"uri": g}}),
    ("org-lsp/ellipsis", None),
    ("org-lsp/journal", None),
    ("org-lsp/faces", None),
    ("textDocument/unknownThing", {}),
    ("$/unknownRequest", {}),
]:
    print(f"  {method:40} {json.dumps(params)[:90] if params else params!s:90} -> {try_req(c, method, params)}")
c.settle(0.5, 5)
print("  messages:", [m["message"] for m in c.messages][-6:])
print("  file line 10 now:", c.text(g).split("\n")[10])

section("notifications for unknown documents and bad versions")
c.notify("textDocument/didChange", {"textDocument": {"uri": "file:///nope.org", "version": 2}, "contentChanges": [{"text": "x"}]})
c.notify("textDocument/didClose", {"textDocument": {"uri": "file:///nope.org"}})
c.notify("textDocument/didSave", {"textDocument": {"uri": "file:///nope.org"}})
c.notify("$/cancelRequest", {"id": 99999})
c.notify("unknown/notification", {"x": 1})
c.notify("workspace/didChangeConfiguration", {"settings": {"org_todo_keywords": ["A", "|", "B"]}})
c.notify("workspace/didChangeWatchedFiles", {"changes": [{"uri": uri_of(os.path.join(ws, "notes.org")), "type": 2}]})
# Out-of-range incremental change
c.notify("textDocument/didChange", {"textDocument": {"uri": g, "version": c.docs[g]["version"] + 1},
                                    "contentChanges": [{"range": {"start": {"line": 9999, "character": 0}, "end": {"line": 9999, "character": 5}}, "text": "zzz"}]})
c.docs[g]["version"] += 1
print("  after bad change, hover works:", try_req(c, "textDocument/hover", {"textDocument": {"uri": g}, "position": {"line": 64, "character": 22}}))
srv_text = c.request("workspace/executeCommand", {"command": "org.outline.path", "arguments": [c.at(g, 10, 3)]})
print("  outline path still:", srv_text)

section("didChange storm (500 random incremental edits), then compare the server's view")
random.seed(7)
text = c.text(g)
from client import position_of
for i in range(500):
    cur = c.text(g)
    lines = cur.split("\n")
    ln = random.randrange(len(lines))
    ch = random.randrange(len(lines[ln]) + 1)
    start_off = sum(len(l) + 1 for l in lines[:ln]) + ch
    kind = random.random()
    p0 = position_of(cur, start_off, c.encoding)
    if kind < 0.6:
        s = random.choice(["a", "日", "🎉", "*", " ", "\n", ":tag:", "[ ] ", "TODO "])
        c.type_at(g, p0["line"], p0["character"], s)
    else:
        end_off = min(sum(len(l) + 1 for l in lines[:ln]) + len(lines[ln]), start_off + random.randrange(4))
        p1 = position_of(cur, end_off, c.encoding)
        c.change(g, [{"range": {"start": p0, "end": p1}, "newText": ""}])
t0 = time.time()
st = try_req(c, "textDocument/semanticTokens/full", {"textDocument": {"uri": g}}, timeout=30)
print("  tokens after storm:", st[0], round(time.time() - t0, 2), "s")
# The server's text: formatting the whole file with a no-op range edit is
# not a read; use the document symbols and a full-text roundtrip instead:
# replace the text with itself as one change and ask for folding ranges.
fr1 = c.request("textDocument/foldingRange", {"textDocument": {"uri": g}})
c.change(g, text=c.text(g))
fr2 = c.request("textDocument/foldingRange", {"textDocument": {"uri": g}})
print("  folding identical after a full resync (server kept in sync):", fr1 == fr2)
c.change(g, text=text)

section("edits refused by the client")
c.apply_ok = False
r = try_req(c, "workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(g, 10, 3)]})
c.settle(0.5, 5)
print("  todo.next with the client refusing edits:", r, "| messages:", [m["message"] for m in c.messages][-2:])
c.apply_ok = True
r = try_req(c, "workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(g, 10, 3)]})
c.settle(0.5, 5)
print("  then accepted:", r, c.text(g).split("\n")[10][:40])

section("stale version: the client changes the document while the server's edit is in flight")
# Send a change right before the command, without waiting
c.type_at(g, 0, 0, "# x\n")
r = try_req(c, "workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(g, 11, 3)]})
c.settle(0.5, 5)
print("  result:", r, "| edit failures:", c.edit_failures[-2:], "| line 11:", c.text(g).split("\n")[11][:40])

section("file changed on disk while not open (agenda)")
n = os.path.join(ws, "notes.org")
open(n, "a").write("* TODO Added on disk\n  SCHEDULED: <2026-10-21 Wed>\n")
c.notify("workspace/didChangeWatchedFiles", {"changes": [{"uri": uri_of(n), "type": 2}]})
c.command("org.agenda.list", {})
c.settle(0.5, 5); c.run_fetches()
ag = [u for u in c.docs if u.startswith("org-agenda")][0]
print("  agenda shows the entry added on disk:", "Added on disk" in c.docs[ag]["text"])
open(n, "a").write("* TODO Second on disk\n  SCHEDULED: <2026-10-21 Wed>\n")
c.notify("workspace/didChangeWatchedFiles", {"changes": [{"uri": uri_of(n), "type": 2}]})
c.command("org.agenda.redo", c.at(ag, 0, 0))
c.settle(0.5, 5); c.run_fetches()
print("  after redo, second entry:", "Second on disk" in c.docs[ag]["text"])
# A file deleted on disk that is an agenda file
os.rename(n, n + ".gone")
c.notify("workspace/didChangeWatchedFiles", {"changes": [{"uri": uri_of(n), "type": 3}]})
r = try_req(c, "workspace/executeCommand", {"command": "org.agenda.list", "arguments": []})
print("  agenda with a missing agenda file:", r)
os.rename(n + ".gone", n)

section("CRLF file")
crlf = os.path.join(ws, "crlf.org")
open(crlf, "w", newline="").write("* TODO A\r\n|a|bb|\r\n|1|2|\r\n* B :x:\r\n")
u = c.open(crlf)
r = c.request("workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(crlf, 0, 3)]})
c.settle(0.5, 5)
f = c.request("textDocument/formatting", {"textDocument": {"uri": u}, "options": {"tabSize": 8, "insertSpaces": True}})
print("  after todo.next:", repr(c.text(crlf)))
print("  formatting edits:", f)
if f:
    print("  formatted:", repr(apply_text_edits(c.text(crlf), f, c.encoding)))
r = c.request("workspace/executeCommand", {"command": "org.table.next.field", "arguments": [c.at(crlf, 1, 1)]})
print("  table next field:", r)
toks = c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": u}})
print("  tokens:", [(t[0], t[3], t[5]) for t in decode_tokens(toks["data"], c.legend, c.text(crlf).replace("\r\n", "\n"), c.encoding)])

section("cancel a request in flight (export through Emacs)")
c.next_id += 1
rid = c.next_id
c._send({"id": rid, "method": "workspace/executeCommand", "params": {"command": "org.babel.execute.buffer", "arguments": [c.at(g, 80, 0)]}})
c.notify("$/cancelRequest", {"id": rid})
ok = c.pump(lambda: rid in c.responses, 10)
print("  answered after cancel:", ok, c.responses.get(rid))

section("huge file")
big = os.path.join(ws, "big.org")
with open(big, "w") as fh:
    for i in range(20000):
        fh.write(f"* TODO Heading {i} :t{i % 7}:\n  SCHEDULED: <2026-10-{(i % 28) + 1:02d} Wed>\n  Text {i} with [[https://e.com/{i}][link]].\n")
t0 = time.time()
u = c.open(big)
st = try_req(c, "textDocument/semanticTokens/full", {"textDocument": {"uri": u}}, timeout=60)
print("  tokens on 60k lines:", st[0], round(time.time() - t0, 2), "s")
t0 = time.time()
st = try_req(c, "textDocument/foldingRange", {"textDocument": {"uri": u}}, timeout=60)
print("  folding:", st[0], round(time.time() - t0, 2), "s")
t0 = time.time()
c.type_at(u, 30000, 0, "x")
st = try_req(c, "textDocument/hover", {"textDocument": {"uri": u}, "position": {"line": 0, "character": 0}}, timeout=60)
print("  hover after an edit:", st[0], round(time.time() - t0, 2), "s")
t0 = time.time()
st = try_req(c, "workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(u, 59997, 3)]}, timeout=60)
c.settle(0.3, 10)
print("  todo.next at the end:", st[0], round(time.time() - t0, 2), "s")
c.close(u)

section("shutdown, then requests, then exit")
print("  shutdown:", try_req(c, "shutdown", None))
print("  hover after shutdown:", try_req(c, "textDocument/hover", {"textDocument": {"uri": g}, "position": {"line": 0, "character": 0}}, timeout=5))
c.notify("exit")
try:
    code = c.proc.wait(timeout=10)
except Exception:
    code = "still running"
print("  exit code:", code)
print("  stderr:", c.stderr()[-1500:])

section("exit without shutdown")
c2 = Client(profile=PLAIN, root=ws, options=options, name="robust2")
c2.open(os.path.join(ws, "gtd.org"))
c2.notify("exit")
try:
    print("  exit code:", c2.proc.wait(timeout=10))
except Exception:
    print("  still running")
    c2.kill()

section("malformed JSON and bad headers")
c3 = Client(profile=PLAIN, root=ws, options=options, name="robust3")
c3.send_raw(b"Content-Length: 10\r\n\r\n{not json}")
c3.send_raw(b"Content-Length: 2\r\n\r\n[]")
time.sleep(0.5)
print("  alive after malformed JSON:", c3.proc.poll() is None)
if c3.proc.poll() is None:
    print("  hover:", try_req(c3, "textDocument/hover", {"textDocument": {"uri": g}, "position": {"line": 0, "character": 0}}, timeout=5))
    c3.shutdown()
print("  stderr:", c3.stderr()[-500:])

section("closing the stdin (editor crash)")
c4 = Client(profile=PLAIN, root=ws, options=options, name="robust4")
c4.proc.stdin.close()
try:
    print("  exit code after stdin closed:", c4.proc.wait(timeout=10))
except Exception:
    print("  STILL RUNNING after stdin closed")
    c4.kill()
