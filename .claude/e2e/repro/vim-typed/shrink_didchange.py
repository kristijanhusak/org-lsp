"""didOpen of a `#+STARTUP: shrink` file, then (optionally) a full didChange
with the same text, then textDocument/inlayHint."""
import json, subprocess, sys, os

def run(change):
    p = subprocess.Popen(["/home/user/org-lsp/target/debug/org-lsp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         env=dict(os.environ, ORG_LSP_CONFIG="/dev/null"))
    n = [0]
    def send(msg):
        b = json.dumps(msg).encode()
        p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); p.stdin.flush()
    def read():
        headers = {}
        while True:
            line = p.stdout.readline().decode()
            if line in ("\r\n", "\n"):
                break
            k, v = line.split(":", 1); headers[k.lower()] = v.strip()
        return json.loads(p.stdout.read(int(headers["content-length"])))
    def request(method, params):
        n[0] += 1; send({"jsonrpc": "2.0", "id": n[0], "method": method, "params": params})
        while True:
            m = read()
            if m.get("id") == n[0] and "method" not in m:
                return m
            if "method" in m and "id" in m:
                send({"jsonrpc": "2.0", "id": m["id"], "result": None})
    uri = "file:///tmp/s.org"
    text = "#+STARTUP: shrink\n| <3> |\n| abcdef |\n"
    request("initialize", {"processId": None, "rootUri": None, "capabilities": {}})
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "org", "version": 1, "text": text}}})
    if change:
        send({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": text}]}})
    r = request("textDocument/inlayHint", {"textDocument": {"uri": uri}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 3, "character": 0}}})
    print("with didChange" if change else "without didChange", r.get("result"))
    p.kill()

run(False)
run(True)
