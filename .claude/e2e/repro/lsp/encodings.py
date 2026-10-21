"""Positions on a line with CJK, accents and an emoji, in each encoding:
prepareRename on its tag, the edit of a TODO change, tag alignment
(formatting) against Emacs, a typed edit after the emoji."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace, emacs
res = {}
for enc in ("utf-16", "utf-8", "utf-32"):
    ws, options = workspace.make(os.path.join(SCRATCH, "ws-enc-" + enc))
    p = os.path.join(ws, "gtd.org")
    t = open(p).read().replace("** Ideas about 日本語 and émojis 🎉                                  :urgent:", "** Ideas about 日本語 and émojis 🎉 :urgent:")
    open(p, "w").write(t)
    c = Client(profile=PLAIN, root=ws, options=options, encodings=[enc], name="enc")
    g = c.open(p); c.settle(0.3, 5)
    print("==", enc, "negotiated", c.encoding)
    pos = c.find(g, ":urgent:", delta=1)
    prep = c.request("textDocument/prepareRename", {"textDocument": {"uri": g}, "position": pos})
    print("  tag position", pos, "prepareRename", prep)
    # TODO change edit via code action
    acts = c.code_actions(g, pos["line"], 3)
    a = [a for a in acts if a.get("kind") == "org.todo.next"][0]
    edits = a["edit"]["documentChanges"][0]["edits"]
    print("  todo.next edits:", edits)
    c.run_action(a)
    line = c.text(g).split("\n")[pos["line"]]
    print("  line after:", repr(line))
    f = c.request("textDocument/formatting", {"textDocument": {"uri": g}, "options": {"tabSize": 8, "insertSpaces": True}})
    print("  formatting edits on the line:", [e for e in f or [] if e["range"]["start"]["line"] == pos["line"]])
    res[enc] = line
    # Typing after the emoji, then a token request
    e = c.find(g, "🎉", delta=1)
    c.type_at(g, e["line"], e["character"], "!")
    toks = c.request("textDocument/semanticTokens/range", {"textDocument": {"uri": g}, "range": {"start": {"line": e["line"], "character": 0}, "end": {"line": e["line"] + 1, "character": 0}}})
    print("  tokens after typing:", [(t[3], t[5]) for t in decode_tokens(toks["data"], c.legend, c.text(g), c.encoding) if t[0] == e["line"]])
    print("  stderr:", c.stderr()[-200:])
    c.shutdown()
print("same line in all encodings:", len(set(res.values())) == 1)
W = os.path.join(SCRATCH, "ws-enc-emacs/"); workspace.make(W)
p = W + "gtd.org"; t = open(p).read().replace("** Ideas about 日本語 and émojis 🎉                                  :urgent:", "** Ideas about 日本語 and émojis 🎉 :urgent:"); open(p, "w").write(t)
o, e = emacs.run('(org-fold-show-all) (goto-char (point-min)) (search-forward "Ideas about") (org-todo (quote right)) (princ (buffer-substring-no-properties (line-beginning-position) (line-end-position)))', p)
print("emacs:   ", repr(o))
print("server:  ", repr(res["utf-16"]))
