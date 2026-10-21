#!/usr/bin/env python3
"""Standard LSP features in a profile: tokens, folding, symbols,
completion, hover, definition, references, rename, formatting, code
actions, diagnostics, inlay hints, document links, workspace symbols.

    python3 features.py plain|full [utf-8|utf-16|utf-32]
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import BARE, FULL, PLAIN, SCRATCH, Client, ResponseError, decode_tokens, offset_of  # noqa
import workspace  # noqa

profile = sys.argv[1] if len(sys.argv) > 1 else PLAIN
enc = sys.argv[2] if len(sys.argv) > 2 else None
ws, options = workspace.make(os.path.join(SCRATCH, f"ws-feat-{profile}-{enc}"))
c = Client(profile=profile, root=ws, options=options, encodings=[enc] if enc else None, name=f"feat-{profile}")
print("encoding", c.encoding)
g = c.open(os.path.join(ws, "gtd.org"))
t = c.open(os.path.join(ws, "tables.org"))
c.settle(0.5, 10)
problems = []


def check(cond, what):
    if not cond:
        problems.append(what)
        print("PROBLEM:", what)


def req(method, params):
    try:
        return c.request(method, params)
    except ResponseError as err:
        print(f"  {method} error {err}")
        problems.append(f"{method} error {err}")
        return None


doc = {"uri": g}
text = c.text(g)
lines = text.split("\n")

# ----- semantic tokens
toks = req("textDocument/semanticTokens/full", {"textDocument": doc})
dec = decode_tokens(toks["data"], c.legend, text, c.encoding)
print("tokens:", len(dec))
# No overlaps, no spanning lines, inside the line.
prev = None
for tk in dec:
    line, ch, ln, ty, mods, snip = tk
    check(line < len(lines), f"token past last line {tk}")
    # line length in units
    from client import _units

    units = sum(_units(x, c.encoding) for x in lines[line])
    check(ch + ln <= units, f"token past end of line {tk} (line units {units})")
    if prev and prev[0] == line:
        check(prev[1] + prev[2] <= ch, f"overlapping tokens {prev} {tk}")
    prev = tk
# Show tokens of interesting lines.
for i, l in enumerate(lines):
    if any(k in l for k in ("Write the report", "日本語", "卵", "CLOSED", "[cite", "Links:", "<<target>>", "#+TITLE", "CLOCK:")):
        print(f"  L{i}: {l!r}")
        for tk in dec:
            if tk[0] == i:
                print(f"     {tk[3]}{'.' + '.'.join(tk[4]) if tk[4] else ''} {tk[5]!r}")
if profile != FULL:
    std = {"namespace", "type", "class", "enum", "interface", "struct", "typeParameter", "parameter", "variable",
           "property", "enumMember", "event", "function", "method", "macro", "keyword", "modifier", "comment",
           "string", "number", "regexp", "operator", "decorator"}
    check(set(c.legend["tokenTypes"]) <= std, f"non-standard token types for a plain client: {set(c.legend['tokenTypes']) - std}")
    stdm = {"declaration", "definition", "readonly", "static", "deprecated", "abstract", "async", "modification",
            "documentation", "defaultLibrary"}
    check(set(c.legend["tokenModifiers"]) <= stdm, f"non-standard modifiers: {set(c.legend['tokenModifiers']) - stdm}")
    print("  plain legend modifiers", c.legend["tokenModifiers"])
# Range
rng = req("textDocument/semanticTokens/range",
          {"textDocument": doc, "range": {"start": {"line": 10, "character": 0}, "end": {"line": 20, "character": 0}}})
decr = decode_tokens(rng["data"], c.legend, text, c.encoding)
full_in = [tk for tk in dec if 10 <= tk[0] < 20]
check(decr == full_in, f"range tokens differ from full ones in the range: {len(decr)} vs {len(full_in)}")
# Delta is not offered: the client may still ask.
try:
    d = c.request("textDocument/semanticTokens/full/delta", {"textDocument": doc, "previousResultId": "x"})
    print("  delta answered:", str(d)[:100])
except ResponseError as err:
    print("  delta:", err)

# ----- folding
folds = req("textDocument/foldingRange", {"textDocument": doc})
print("folds:", len(folds), folds[:4])
for f in folds:
    check(f["startLine"] < f["endLine"], f"empty fold {f}")
    check(f["endLine"] < len(lines), f"fold past end {f}")

# ----- symbols
syms = req("textDocument/documentSymbol", {"textDocument": doc})
print("symbols:", [s["name"] for s in syms], "children of first:", [s["name"] for s in syms[0].get("children", [])][:5])
ws_syms = req("workspace/symbol", {"query": "report"})
print("workspace symbols 'report':", [(s["name"], s["location"]["uri"].rsplit("/", 1)[-1]) for s in ws_syms or []])
ws_syms = req("workspace/symbol", {"query": ""})
print("workspace symbols '':", len(ws_syms or []))


# ----- completion
def complete(path, needle, delta=0, insert=None, trigger=None):
    """Complete at needle+delta; with `insert`, type it there first."""
    p = c.find(path, needle, delta=delta)
    if insert:
        c.type_at(path, p["line"], p["character"], insert)
        p = {"line": p["line"], "character": p["character"] + sum(_units(x, c.encoding) for x in insert)}
    ctx = {"triggerKind": 2, "triggerCharacter": trigger} if trigger else {"triggerKind": 1}
    r = req("textDocument/completion", {"textDocument": {"uri": c.uri(path)}, "position": p, "context": ctx})
    items = r.get("items", r) if isinstance(r, dict) else (r or [])
    return [i["label"] for i in items], items


def undo(path, original):
    c.change(path, text=original)


orig = c.text(g)
labels, _ = complete(g, "** WAIT Reply", 3)
print("complete keyword after stars:", labels[:10])
undo(g, orig)
labels, _ = complete(g, "** Meeting with Bob", len("** Meeting with Bob"), insert=" :", trigger=":")
print("complete tags:", labels[:12])
undo(g, orig)
labels, items = complete(g, "[cite:@knuth1984]", len("[cite:@"), trigger="@")
print("complete cite keys:", labels, [i.get("detail") for i in items][:2])
labels, _ = complete(g, "A paragraph.", 0, insert="#+", trigger="+")
print("complete keywords #+:", labels[:15], len(labels))
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="#+begin_", trigger=None)
print("complete blocks #+begin_:", labels[:15])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert=":", trigger=":")
print("complete drawers ':' at bol:", labels[:15])
undo(g, orig)
labels, _ = complete(g, "   :Effort:", 3, insert="", trigger=None)
print("complete in property drawer:", labels[:15])
p = c.find(g, "   :END:")
c.type_at(g, p["line"], 0, "   :\n")
r = req("textDocument/completion", {"textDocument": doc, "position": {"line": p["line"], "character": 4}, "context": {"triggerKind": 2, "triggerCharacter": ":"}})
items = r.get("items", r) if isinstance(r, dict) else (r or [])
print("complete property keys:", [i["label"] for i in items][:15])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="[[", trigger="[")
print("complete links [[:", labels[:20], len(labels))
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="[[id:", trigger=None)
print("complete links [[id::", labels[:10])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="[[*", trigger="*")
print("complete links [[*:", labels[:10])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="[[file:", trigger=None)
print("complete links [[file::", labels[:10])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="\\alp", trigger=None)
print("complete entity \\alp:", labels[:10])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="[[#", trigger="#")
print("complete custom ids [[#:", labels[:10])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="[[roam:", trigger=None)
print("complete roam:", labels[:10])
undo(g, orig)
labels, _ = complete(g, "A paragraph.", 0, insert="#+begin_src py", trigger=None)
print("complete src lang:", labels[:10])
undo(g, orig)

# ----- hover
for needle, d in (("@knuth1984]", 2), ("@knuht1984", 2), ("Write the report", 0), ("[[https", 4), ("[fn:1]", 2)):
    pos = c.find(g, needle, delta=d)
    h = req("textDocument/hover", {"textDocument": doc, "position": pos})
    print(f"hover {needle!r}:", json.dumps(h)[:300] if h else None)


# ----- definition
for needle, d in (("[[#custom]", 4), ("[[*Tasks]", 4), ("[[target]]", 3), ("[fn:1]", 2), ("[[file:tables.org]", 5),
                  ("[[id:0a1b2c3d-0000-4000-8000-000000000002]", 6), ("@knuth1984]", 2), ("[[file:notes.org::*Meeting", 8),
                  ("[[roam:Beta]]", 4), ("[cite/t:@lamport1994; @knuth1984]", 2)):
    pos = c.find(g, needle, delta=d)
    r = req("textDocument/definition", {"textDocument": doc, "position": pos})
    locs = r if isinstance(r, list) else ([r] if r else [])
    print(f"definition {needle!r}:", [(l.get("uri", l.get("targetUri", "")).rsplit("/", 1)[-1], (l.get("range") or l.get("targetSelectionRange"))["start"]) for l in locs])

# ----- references
for needle, d in (("<<target>>", 3), ("Write the report", 0), (":work:", 2), ("CUSTOM_ID: custom", 12), ("@knuth1984]", 2),
                  ("* Tasks", 3)):
    pos = c.find(g, needle, delta=d)
    r = req("textDocument/references", {"textDocument": doc, "position": pos, "context": {"includeDeclaration": True}})
    print(f"references {needle!r}:", [(l["uri"].rsplit("/", 1)[-1], l["range"]["start"]["line"]) for l in r or []])

# ----- document links
links = req("textDocument/documentLink", {"textDocument": doc})
print("document links:", len(links or []))
for l in links or []:
    s = l["range"]["start"]
    tgt = l.get("target")
    if not tgt and l.get("data") is not None:
        try:
            tgt = "resolved:" + str(c.request("documentLink/resolve", l).get("target"))
        except ResponseError as err:
            tgt = f"resolve error {err}"
    print("   ", s, lines[s["line"]][:60].strip()[:40], "->", str(tgt)[:110])

# ----- inlay hints
ih = req("textDocument/inlayHint", {"textDocument": doc, "range": {"start": {"line": 0, "character": 0}, "end": {"line": len(lines), "character": 0}}})
print("inlay hints:", len(ih or []), [(h["position"], h["label"]) for h in (ih or [])[:8]])

# ----- diagnostics
diags = c.diagnostics.get(g, [])
for d in diags:
    check(d.get("severity") in (3, 4), f"diagnostic with severity {d.get('severity')}: {d['message']}")
print("diagnostics:", [(d["range"]["start"]["line"], d["severity"], d["message"]) for d in diags])

# quick fixes for unknown cite key
dk = [d for d in diags if "citation" in d["message"]]
if dk:
    s = dk[0]["range"]["start"]
    acts = c.code_actions(g, s["line"], s["character"] + 2, diagnostics=dk)
    print("quick fixes:", [(a.get("title"), a.get("kind")) for a in acts if a.get("kind") == "quickfix"])

# ----- code actions at places
for needle, d in (("Write the report", 0), ("<2026-10-22 Thu 14:00", 3), ("[ ] bread", 1), ("|   3 |", 3), ("A paragraph.", 0),
                  ("2. second", 0), ("CLOCK:", 0), ("#+TBLFM", 0)):
    path = t if needle in ("|   3 |", "#+TBLFM") else g
    pos = c.find(path, needle, delta=d)
    acts = c.code_actions(path, pos["line"], pos["character"])
    print(f"code actions at {needle!r}: {len(acts)}:", [a.get("title") for a in acts][:40])

# ----- formatting
for path in (t, g):
    r = req("textDocument/formatting", {"textDocument": {"uri": path}, "options": {"tabSize": 8, "insertSpaces": True}})
    print("formatting", path.rsplit("/", 1)[-1], "edits:", len(r or []))
    if r:
        newt = __import__("client").apply_text_edits(c.text(path), r, c.encoding)
        import difflib
        print("".join(list(difflib.unified_diff(c.text(path).splitlines(True), newt.splitlines(True), n=0))[2:60]))
        # idempotence
        c.change(path, text=newt)
        r2 = req("textDocument/formatting", {"textDocument": {"uri": path}, "options": {"tabSize": 8, "insertSpaces": True}})
        check(not r2, f"formatting not idempotent on {path}: {r2}")

# ----- rename: heading, tag
pos = c.find(g, "Project Alpha", delta=2)
prep = req("textDocument/prepareRename", {"textDocument": doc, "position": pos})
print("prepareRename on heading:", prep)
r = req("textDocument/rename", {"textDocument": doc, "position": pos, "newName": "Project Omega"})
print("rename heading:", json.dumps(r)[:600])
pos = c.find(g, ":work:", delta=2)
prep = req("textDocument/prepareRename", {"textDocument": doc, "position": pos})
print("prepareRename on tag:", prep)
r = req("textDocument/rename", {"textDocument": doc, "position": pos, "newName": "job"})
if r:
    ch = r.get("changes") or {}
    dc = r.get("documentChanges") or []
    print("rename tag edits:", {u.rsplit('/', 1)[-1]: len(e) for u, e in ch.items()}, [(x["textDocument"]["uri"].rsplit('/', 1)[-1], len(x["edits"])) for x in dc if "edits" in x])
pos = c.find(g, "A paragraph.", delta=2)
try:
    prep = c.request("textDocument/prepareRename", {"textDocument": doc, "position": pos})
    print("prepareRename on text:", prep)
except ResponseError as err:
    print("prepareRename on text: error", err)

# ----- on type formatting
p = c.find(g, "A paragraph.", delta=len("A paragraph."))
c.type_at(g, p["line"], p["character"], "\n")
r = req("textDocument/onTypeFormatting", {"textDocument": doc, "position": {"line": p["line"] + 1, "character": 0}, "ch": "\n", "options": {"tabSize": 8, "insertSpaces": True}})
print("onTypeFormatting after RET in body:", r)
undo(g, orig)

print("stderr:", c.stderr()[-2000:])
print("exit", c.shutdown())
print("PROBLEMS:", problems)
