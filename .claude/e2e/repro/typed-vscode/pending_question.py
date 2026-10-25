"""While a window/showMessageRequest is not answered yet (a VS Code toast
the user has not clicked), does the server answer other requests?
python3 pending_question.py [SERVER]"""
import json
import os
import subprocess
import sys
import tempfile
import threading
import time

server = sys.argv[1] if len(sys.argv) > 1 else '/home/user/org-lsp/target/debug/org-lsp'
root = tempfile.mkdtemp(prefix='pending-')
path = os.path.join(root, 'fn.org')
text = '* A\nText[fn:1] and [fn:2].\n* Footnotes\n[fn:1] The first.\n'
open(path, 'w').write(text)
uri = 'file://' + path
env = dict(os.environ, ORG_LSP_CONFIG='/dev/null', XDG_STATE_HOME=root)
proc = subprocess.Popen([server], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, cwd=root)
messages = []


def send(message):
    body = json.dumps(message).encode()
    proc.stdin.write(b'Content-Length: %d\r\n\r\n' % len(body) + body)
    proc.stdin.flush()


def read():
    while True:
        length = None
        while True:
            line = proc.stdout.readline()
            if not line:
                return
            if line in (b'\r\n', b'\n'):
                break
            if line.lower().startswith(b'content-length:'):
                length = int(line.split(b':')[1])
        message = json.loads(proc.stdout.read(length))
        messages.append((time.time(), message))
        # Answer what an editor answers at once, except the question.
        if 'id' in message and 'method' in message and message['method'] != 'window/showMessageRequest':
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': None})


threading.Thread(target=read, daemon=True).start()


def wait(probe, timeout):
    end = time.time() + timeout
    while time.time() < end:
        for _, m in messages:
            if probe(m):
                return m
        time.sleep(0.05)
    return None


send({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
    'processId': None, 'rootUri': 'file://' + root,
    'capabilities': {'window': {'showMessage': {'messageActionItem': {'additionalPropertiesSupport': True}}, 'showDocument': {'support': True}},
                     'workspace': {'applyEdit': True}},
    'initializationOptions': {'config_file': False}}})
wait(lambda m: m.get('id') == 1, 10)
send({'jsonrpc': '2.0', 'method': 'initialized', 'params': {}})
send({'jsonrpc': '2.0', 'method': 'textDocument/didOpen', 'params': {'textDocument': {'uri': uri, 'languageId': 'org', 'version': 1, 'text': text}}})
send({'jsonrpc': '2.0', 'id': 2, 'method': 'workspace/executeCommand', 'params': {
    'command': 'org.footnote.action', 'arguments': [{'textDocument': {'uri': uri}, 'position': {'line': 1, 'character': 17}}]}})
asked = wait(lambda m: m.get('method') == 'window/showMessageRequest', 20)
print('asked:', asked and asked['params']['message'])
start = time.time()
send({'jsonrpc': '2.0', 'id': 3, 'method': 'textDocument/documentSymbol', 'params': {'textDocument': {'uri': uri}}})
answered = wait(lambda m: m.get('id') == 3, 10)
print('documentSymbol while the question waits:', 'answered in %.2f s' % (time.time() - start) if answered else 'no answer in 10 s')
start = time.time()
send({'jsonrpc': '2.0', 'id': 4, 'method': 'workspace/executeCommand', 'params': {
    'command': 'org.todo', 'arguments': [{'textDocument': {'uri': uri}, 'position': {'line': 0, 'character': 2}}]}})
answered = wait(lambda m: m.get('id') == 4, 10)
print('another command while the question waits:', 'answered in %.2f s' % (time.time() - start) if answered else 'no answer in 10 s')
if asked:
    send({'jsonrpc': '2.0', 'id': asked['id'], 'result': {'title': 'Yes'}})
    answered = wait(lambda m: m.get('id') == 4, 10)
    print('after the question is answered:', 'answered' if answered else 'still no answer')
proc.kill()
