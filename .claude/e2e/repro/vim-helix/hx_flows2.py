"""Helix: links, rename, symbols, footnotes, babel, tables, clock table, diagnostics."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
FILE = """* Alpha
See [[Beta]] and [fn:1].
* Beta
| x | y |
| 1 | 2 |
#+begin_src emacs-lisp :tangle out.el
(+ 1 2)
#+end_src
text ** heading in a paragraph
#+begin_src sh :foo 1
echo hi
#+end_src
[fn:1] The note.
"""
h = Helix("flows2", {"files": {"a.org": FILE, "b.org": "* Other\n[[file:a.org::*Beta][to beta]]\n"}, "emacs": True})
def step(name, fn):
    try:
        r = fn(); print(f"ok   {name}: {r if r is not None else ''}")
    except Exception as e:
        print(f"FAIL {name}: {str(e).splitlines()[0][:400]}")
        try: h.keys("Escape"); h.settle()
        except Exception: pass
A = None
def L(): return h.text(A).split("\n")
try:
    A = os.path.join(h.directory, "a.org"); h.start(A)
    def fn():
        goto(h, 2, 23); h.keys("g", "d"); h.settle(); r = h.status()[2:]
        h.keys("g", "d"); h.settle(); return [r, h.status()[2:]]
    step("gd on [fn:1] then back from definition", fn)
    def hover():
        goto(h, 2, 7); h.keys("Space", "k"); time.sleep(1.5); h.settle(); s = [r.strip() for r in h.screen() if "Beta" in r]; h.keys("Escape"); h.settle(); return s[:4]
    step("Space k on [[Beta]]", hover)
    def refs():
        goto(h, 3, 3); h.keys("g", "r"); time.sleep(1.5); h.settle(); s = [r.strip()[:100] for r in h.screen() if "a.org" in r or "b.org" in r]; h.keys("Escape"); h.settle(); return s[:6]
    step("gr (references) on * Beta", refs)
    def rename():
        goto(h, 3, 3); h.keys("Space", "r"); h.wait(lambda: any("rename-to:" in r for r in h.screen()), "rename"); h.keys("C-u"); h.type("Gamma"); h.keys("Enter"); h.settle()
        return [L()[1], L()[2], (h.text(os.path.join(h.directory, "b.org")) or "").split("\n")[1], h.messages().strip()]
    step("rename headline Beta→Gamma updates links", rename)
    def symbols():
        h.keys("Space", "S"); time.sleep(1.5); h.type("Oth"); time.sleep(1.5); h.settle(); s = [r.strip()[:80] for r in h.screen() if "Other" in r]; h.keys("Escape"); h.settle(); return s[:3]
    step("workspace symbols (Space S)", symbols)
    def diags():
        h.keys("Space", "d"); time.sleep(1.5); h.settle(); s = [r.strip()[:110] for r in h.screen() if "a.org" in r or "heading" in r.lower() or "foo" in r.lower()]; h.keys("Escape"); h.settle(); return s[:8]
    step("diagnostics picker", diags)
    def tangle():
        goto(h, 7, 2); act(h, "Tangle the file's code blocks"); return [h.messages().strip(), os.path.exists(os.path.join(h.directory, "out.el"))]
    step("tangle", tangle)
    def babel():
        goto(h, 7, 2); act(h, "Evaluate the code block", ["y"]); time.sleep(3); h.settle(); return [l for l in L() if "RESULTS" in l or l.startswith(": ")] + [h.messages().strip()]
    step("evaluate emacs-lisp block (y)", babel)
    def shrink():
        goto(h, 4, 3); act(h, "Shrink or expand the table column"); return [L()[3], L()[4], h.messages().strip()]
    step("shrink table column (no cookie)", shrink)
    def clocktable():
        goto(h, 1, 3); act(h, "Insert or update a clock table"); return [l for l in L() if l.startswith("#+")][:4]
    step("clock table", clocktable)
    def edit_special():
        n = L().index("#+begin_src sh :foo 1") + 1; goto(h, n + 1, 2); act(h, "Edit the block in a file of its own"); return str(h.status()[1:])
    step("edit special on sh block", edit_special)
finally:
    h.stop()
