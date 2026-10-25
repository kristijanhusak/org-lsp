"""Helix: a #+TODO: line without keys, used instead of the settings."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402


def todo_line(x):
    x.at("* A first", 3)
    x.act("Change the TODO state")
    a = x.lines("b.org")[1]
    x.act("Change the TODO state")
    return [a, x.lines("b.org")[1], x.msgs()]


run("typed1c", {"b.org": "#+TODO: A B | C\n* A first\n"}, {"org_todo_keywords": ["TODO(t)", "|", "DONE(d)"]},
    [("#+TODO line", todo_line)], first="b.org")
