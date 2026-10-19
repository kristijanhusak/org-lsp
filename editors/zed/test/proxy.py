#!/usr/bin/env python3
"""Stand between Zed and org-lsp, and write down what they say.

Zed starts this in place of the server (`lsp.org-lsp.binary` in the
settings run.py writes): it starts the server given on its command line
and passes the messages on both ways unchanged, and appends each to the
log named by `ORG_LSP_ZED_LOG`, one JSON object a line, `{"from": "zed" |
"server", "message": ...}`. run.py reads the log to know what Zed has open,
the text of each buffer, where the cursor is and what was said.
"""

import json
import os
import subprocess
import sys
import threading

log_lock = threading.Lock()
log = open(os.environ["ORG_LSP_ZED_LOG"], "a", encoding="utf-8")


def read_message(stream):
    """One message of the base protocol, its header and body, or None at the end."""
    header = b""
    length = None
    while True:
        line = stream.readline()
        if not line:
            return None
        header += line
        if line in (b"\r\n", b"\n"):
            break
        name, _, value = line.decode("ascii").partition(":")
        if name.strip().lower() == "content-length":
            length = int(value.strip())
    body = stream.read(length or 0)
    return header, body


def relay(source, target, who):
    while True:
        message = read_message(source)
        if message is None:
            break
        header, body = message
        try:
            parsed = json.loads(body)
        except ValueError:
            parsed = None
        # Written down before it is passed on: the answer to a request can
        # only come after the other side has it, so the log has every
        # request before its answer. Passed on first, the answer could be
        # written down first by the other thread (more often under load),
        # and run.py would wait for an answer that came already.
        with log_lock:
            log.write(json.dumps({"from": who, "message": parsed}) + "\n")
            log.flush()
        target.write(header + body)
        target.flush()
    try:
        target.close()
    except OSError:
        pass


def main():
    server = subprocess.Popen(sys.argv[1:], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    up = threading.Thread(target=relay, args=(sys.stdin.buffer, server.stdin, "zed"), daemon=True)
    up.start()
    relay(server.stdout, sys.stdout.buffer, "server")
    sys.exit(server.wait())


if __name__ == "__main__":
    main()
