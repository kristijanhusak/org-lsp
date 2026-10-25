"""Helix: org-protocol (capture and store-link from `org-lsp protocol`),
an attachment: link."""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

A = """* Task
:PROPERTIES:
:ID: att-1
:END:
[[attachment:note.txt]]
"""
opts = {
    "org_capture_templates": [
        {"key": "p", "description": "Protocol", "target": ["file", "$DIR/inbox.org"],
         "template": "* %:description\n%:link\n%i"},
    ],
    "org_protocol_default_template_key": "p",
    "org_attach_id_dir": "$DIR/data",
}


def protocol(x):
    state = os.path.join(x.h.work, "state")
    env = dict(os.environ, XDG_STATE_HOME=state, HOME=os.path.join(x.h.work, "home"))
    url = "org-protocol://capture?template=p&url=https%3A%2F%2Forgmode.org&title=Org%20mode&body=selected%20text"
    out = subprocess.run([os.environ["ORG_LSP_BIN"], "protocol", url], env=env, capture_output=True, text=True, timeout=30)
    time.sleep(2)
    x.h.settle()
    a = [out.returncode, out.stdout.strip()[:200], out.stderr.strip()[:200], x.where()]
    x.act("Capture: finish")
    return a + [x.lines("inbox.org"), x.msgs()]


def store_link(x):
    state = os.path.join(x.h.work, "state")
    env = dict(os.environ, XDG_STATE_HOME=state, HOME=os.path.join(x.h.work, "home"))
    url = "org-protocol://store-link?url=https%3A%2F%2Fexample.org&title=Example"
    out = subprocess.run([os.environ["ORG_LSP_BIN"], "protocol", url], env=env, capture_output=True, text=True, timeout=30)
    time.sleep(1)
    x.h.settle()
    return [out.returncode, x.msgs()]


def attachment(x):
    os.makedirs(os.path.join(x.h.directory, "data", "at", "t-1"), exist_ok=True)
    with open(os.path.join(x.h.directory, "data", "at", "t-1", "note.txt"), "w") as out:
        out.write("attached\n")
    x.h.open("a.org")
    x.at("[[attachment:note.txt]]", 3)
    x.keys("g", "d")
    for _ in range(20):
        if "note.txt" in x.where():
            break
        time.sleep(0.5)
        x.h.settle()
    return [x.where(), x.msgs()]


run("typed4", {"a.org": A, "inbox.org": ""}, opts, [
    ("org-protocol capture", protocol),
    ("org-protocol store-link", store_link),
    ("attachment link", attachment),
], first="a.org")
