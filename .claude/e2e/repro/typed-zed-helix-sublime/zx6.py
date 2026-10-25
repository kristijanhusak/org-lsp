"""Zed: whether it announces willSaveWaitUntil (org-crypt's encryption on
save), what it sends as it saves (ctrl-s), and a completion menu."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402


def capabilities(s):
    with open(s.zed.log.path, encoding="utf-8") as file:
        for line in file:
            entry = json.loads(line)
            message = entry["message"] or {}
            if entry["from"] == "zed" and message.get("method") == "initialize":
                return message["params"]["capabilities"]["textDocument"].get("synchronization")
    return None


def saved(s):
    s.zed.press("ctrl-s")
    s.zed.settle()
    methods = []
    with open(s.zed.log.path, encoding="utf-8") as file:
        for line in file:
            entry = json.loads(line)
            method = (entry["message"] or {}).get("method", "")
            if entry["from"] == "zed" and "Save" in method:
                methods.append(method)
    return methods


steps = [
    {"open": "a.org", "check": capabilities},
    {"type": "x", "check": saved},
]
zxlib.run("zx6", {"a.org": "* Secret :crypt:\nText\n"}, {}, steps)
