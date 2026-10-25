"""Zed held open on a file for a look by hand: zxhold.py [STEPS-AS-JSON]."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

steps = json.loads(sys.argv[1]) if len(sys.argv) > 1 else []
zxlib.run("zxhold", {"a.org": "* Second\n** b\n** a\n* Table\n| a | b |\n| 1 | 2 |\n"}, {},
          [{"open": "a.org"}] + steps + [{"hold": True}])
