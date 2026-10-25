"""Helix: what it sends the server as it saves (willSave, willSaveWaitUntil,
didSave), for org-crypt's encryption before saving."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402


def save(x):
    x.keys("g", "e"); x.h.keys("o"); x.h.type("more"); x.h.escape()
    before = x.h.log.count
    x.cmd("write")
    x.h.settle()
    sent = [m.get("method") for i, m in x.h.log.sent if i > before]
    init = next((m for _, m in x.h.log.sent if m.get("method") == "initialize"), {})
    sync = init.get("params", {}).get("capabilities", {}).get("textDocument", {}).get("synchronization")
    return [sent, sync]


run("typed5", {"a.org": "* Secret :crypt:\nText\n"}, {}, [("save", save)], first="a.org")
