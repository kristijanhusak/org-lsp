#!/usr/bin/env python3
"""Type into the Vim of a tmux session what it asks to be typed, as the
user types it: the keys go through the terminal, not feedkeys().

    editors/vim/test/typist.py SESSION WORK

The Vim of the typed scenarios (scenarios.vim, ORG_LSP_TEST_TYPED) writes
WORK/keys, a JSON list of `["key", NAME]` (a key as tmux names it:
`Enter`, `C-c`, `M-S-Right`) and `["text", TEXT]` (typed as it is); this
types them in turn, then removes the file, which tells Vim they are on
their way. It ends once WORK/result.txt is there or the session is gone.
"""

import json
import os
import subprocess
import sys
import time


def tmux(*args):
    return subprocess.run(["tmux", *args], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode


def send(session, item):
    kind, value = item
    if kind == "key":
        tmux("send-keys", "-t", session, value)
    elif value.isascii():
        # By their codes: an argument ending in `;` would be a separator
        # to tmux.
        tmux("send-keys", "-t", session, "-H", *[format(ord(c), "x") for c in value])
    else:
        tmux("send-keys", "-t", session, "-l", value)


def main():
    session, work = sys.argv[1:3]
    keys = os.path.join(work, "keys")
    polls = 0
    while not os.path.exists(os.path.join(work, "result.txt")):
        if os.path.exists(keys):
            with open(keys, encoding="utf-8") as f:
                items = json.load(f)
            for item in items:
                send(session, item)
            os.remove(keys)
        # The session asked after once a second.
        polls += 1
        if polls % 50 == 0 and tmux("has-session", "-t", session) != 0:
            break
        time.sleep(0.02)


if __name__ == "__main__":
    main()
