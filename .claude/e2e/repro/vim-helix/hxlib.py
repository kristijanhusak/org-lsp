"""Helpers over editors/helix/test/run.py's Helix class for by-hand checks."""
import importlib.util, json, os, sys, time
os.environ.setdefault("HX", "/root/helix/helix-25.07.1-x86_64-linux/hx")
os.environ.setdefault("ORG_LISP_DIR", "/root/org-mode/lisp")
spec = importlib.util.spec_from_file_location("hxrun", "/home/user/org-lsp/editors/helix/test/run.py")
hx = importlib.util.module_from_spec(spec); spec.loader.exec_module(hx)
Helix, Failure = hx.Helix, hx.Failure

def titles(h):
    acts = h.code_actions()
    h.keys("Escape"); h.settle()
    return [a["title"] for a in acts]

def goto(h, line, col=1):
    h.command_line(f"goto {line}")
    h.keys("g", "h")
    if col > 1:
        h.keys(str(col - 1), "l")
    h.settle()

def act(h, title_part, answers=()):
    """Pick the code action whose title contains title_part, then answer."""
    acts = h.code_actions()
    idx = next((i for i, a in enumerate(acts) if title_part in a["title"]), None)
    if idx is None:
        h.keys("Escape"); h.settle()
        raise Failure(f"no action like {title_part!r}: {[a['title'] for a in acts]}")
    h.pick_action(acts, idx)
    for a in answers:
        h.answer(a, title_part)

def buf(h):
    _, path, line, col = h.status()
    return path, line, col, h.text(path)
