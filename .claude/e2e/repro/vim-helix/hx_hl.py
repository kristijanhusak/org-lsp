"""Helix: the scope of every run of text of a file (tree-sitter queries of editors/helix/runtime), page by page."""
import re, sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
path = sys.argv[1]
text = open(path).read()
name = os.path.basename(path)
h = Helix("hl", {"files": {name: text}})
scopes = hx.scope_colors(h.config)
h.environment["COLORTERM"] = "truecolor"
try:
    h.start(name)
    total = len(text.splitlines()); top = 1
    seen = set()
    while top <= total:
        h.command_line(f"goto {min(top + 44, total)}")
        for row, colors in h.colored_rows()[:-2]:
            m = re.match(r"\s*(\d+) ", row)
            if not m or int(m.group(1)) in seen: continue
            seen.add(int(m.group(1)))
            out, cur, buf = [], None, ""
            for ch, c in zip(row, colors):
                s = scopes.get(c)
                if s != cur:
                    if buf.strip(): out.append(f"{buf}<{cur}>" if cur else buf)
                    elif buf: out.append(buf)
                    cur, buf = s, ""
                buf += ch
            if buf.strip(): out.append(f"{buf}<{cur}>" if cur else buf)
            print("".join(out).rstrip())
        top += 40
finally:
    h.stop()
import re
