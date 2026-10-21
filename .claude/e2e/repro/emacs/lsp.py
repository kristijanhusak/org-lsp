"""A small LSP client for org-lsp, for the emacs tester.

    c = Client(settings={...}, env={...})
    uri = c.open('/path/file.org', text)
    r = c.cmd('org.export.dispatch', uri, (line, col), answers=['h', 'h'])
    c.messages  # window/showMessage (type, text), in order
    c.text(uri)  # the document after the edits the server sent
"""
import json, os, subprocess, threading, time, queue, sys

SERVER = os.environ.get('ORG_LSP', '/home/user/org-lsp/target/debug/org-lsp')
SCRATCH = '/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/emacs'


def uri_of(path):
    return 'file://' + os.path.abspath(path)


class Client:
    def __init__(self, settings=None, env=None, root=None, capabilities=None, home=None,
                 request_answers=None, verbose=False, init_settings_raw=False):
        self.home = home or os.path.join(SCRATCH, 'home')
        os.makedirs(self.home, exist_ok=True)
        e = dict(os.environ)
        e['HOME'] = self.home
        e['XDG_CONFIG_HOME'] = os.path.join(self.home, '.config')
        e.pop('ORG_LSP_CONFIG', None)
        if env:
            for k, v in env.items():
                if v is None:
                    e.pop(k, None)
                else:
                    e[k] = v
        self.proc = subprocess.Popen([SERVER], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=open(os.path.join(SCRATCH, 'server-stderr.log'), 'ab'), env=e)
        self.next_id = 0
        self.pending = {}
        self.messages = []
        self.requests = []  # (method, params) the server asked
        self.notifications = []
        self.docs = {}
        self.versions = {}
        self.request_answers = request_answers or []  # list of titles, for showMessageRequest
        self.default_answer = 'No'
        self.verbose = verbose
        self.lock = threading.Lock()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        root = root or SCRATCH
        caps = capabilities if capabilities is not None else {
            'workspace': {'applyEdit': True, 'workspaceEdit': {'documentChanges': True}},
            'window': {'showDocument': {'support': True}},
            'general': {'positionEncodings': ['utf-16']},
        }
        r = self.request('initialize', {
            'processId': os.getpid(), 'rootUri': uri_of(root),
            'workspaceFolders': [{'uri': uri_of(root), 'name': 'w'}],
            'capabilities': caps, 'initializationOptions': settings,
        }, timeout=60)
        self.init = r
        self.notify('initialized', {})

    # ----- framing
    def _send(self, msg):
        body = json.dumps(msg).encode()
        if self.verbose:
            print('>>', json.dumps(msg)[:400], file=sys.stderr)
        with self.lock:
            self.proc.stdin.write(b'Content-Length: %d\r\n\r\n' % len(body) + body)
            self.proc.stdin.flush()

    def _read(self):
        f = self.proc.stdout
        while True:
            headers = {}
            while True:
                line = f.readline()
                if not line:
                    return
                line = line.strip()
                if not line:
                    break
                k, v = line.split(b':', 1)
                headers[k.lower()] = v.strip()
            body = f.read(int(headers[b'content-length']))
            msg = json.loads(body)
            if self.verbose:
                print('<<', json.dumps(msg)[:400], file=sys.stderr)
            if 'id' in msg and 'method' in msg:
                self._server_request(msg)
            elif 'id' in msg:
                q = self.pending.get(msg['id'])
                if q:
                    q.put(msg)
            else:
                self.notifications.append(msg)
                if msg['method'] == 'window/showMessage':
                    self.messages.append((msg['params']['type'], msg['params']['message']))

    def _server_request(self, msg):
        m, p = msg['method'], msg.get('params')
        self.requests.append((m, p))
        result = None
        if m == 'workspace/applyEdit':
            self.apply(p['edit'])
            result = {'applied': True}
        elif m == 'window/showMessageRequest':
            self.messages.append(('request', p['message'], [a['title'] for a in p.get('actions') or []]))
            title = self.request_answers.pop(0) if self.request_answers else self.default_answer
            result = None if title is None else {'title': title}
        elif m == 'window/showDocument':
            result = {'success': True}
        elif m == 'workspace/configuration':
            result = [None for _ in p['items']]
        self._send({'jsonrpc': '2.0', 'id': msg['id'], 'result': result})

    def request(self, method, params, timeout=120):
        self.next_id += 1
        i = self.next_id
        q = queue.Queue()
        self.pending[i] = q
        self._send({'jsonrpc': '2.0', 'id': i, 'method': method, 'params': params})
        try:
            msg = q.get(timeout=timeout)
        except queue.Empty:
            raise TimeoutError(f'{method} did not answer in {timeout}s')
        if 'error' in msg:
            return {'__error__': msg['error']}
        return msg.get('result')

    def notify(self, method, params):
        self._send({'jsonrpc': '2.0', 'method': method, 'params': params})

    # ----- documents
    def open(self, path, text=None):
        if text is None:
            text = open(path).read()
        else:
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, 'w') as f:
                f.write(text)
        u = uri_of(path)
        self.docs[u] = text
        self.versions[u] = 1
        self.notify('textDocument/didOpen', {'textDocument': {'uri': u, 'languageId': 'org', 'version': 1, 'text': text}})
        return u

    def change(self, uri, text):
        self.docs[uri] = text
        self.versions[uri] += 1
        self.notify('textDocument/didChange', {'textDocument': {'uri': uri, 'version': self.versions[uri]},
                                                'contentChanges': [{'text': text}]})

    def text(self, uri):
        return self.docs.get(uri)

    @staticmethod
    def _offset(text, pos):
        lines = text.split('\n')
        off = sum(len(l) + 1 for l in lines[:pos['line']])
        # utf-16 columns
        line = lines[pos['line']] if pos['line'] < len(lines) else ''
        units = 0
        i = 0
        while i < len(line) and units < pos['character']:
            units += 2 if ord(line[i]) > 0xFFFF else 1
            i += 1
        return off + i

    def _apply_text_edits(self, uri, edits):
        text = self.docs.get(uri)
        if text is None:
            path = uri[len('file://'):]
            text = open(path).read() if os.path.exists(path) else ''
        spans = sorted(((self._offset(text, e['range']['start']), self._offset(text, e['range']['end']), e['newText'])
                        for e in edits), key=lambda s: (s[0], s[1]), reverse=True)
        for a, b, t in spans:
            text = text[:a] + t + text[b:]
        if uri in self.docs:
            self.docs[uri] = text
            self.versions[uri] += 1
            self.notify('textDocument/didChange', {'textDocument': {'uri': uri, 'version': self.versions[uri]},
                                                    'contentChanges': [{'text': text}]})
        else:
            self.docs[uri] = text
            self.versions[uri] = 0

    def apply(self, edit):
        for change in edit.get('documentChanges') or []:
            kind = change.get('kind')
            if kind == 'create':
                path = change['uri'][len('file://'):]
                os.makedirs(os.path.dirname(path), exist_ok=True)
                open(path, 'a').close()
            elif kind == 'rename':
                os.rename(change['oldUri'][7:], change['newUri'][7:])
            elif kind == 'delete':
                try:
                    os.remove(change['uri'][7:])
                except OSError:
                    pass
            else:
                self._apply_text_edits(change['textDocument']['uri'], change['edits'])
        for uri, edits in (edit.get('changes') or {}).items():
            self._apply_text_edits(uri, edits)

    # ----- commands
    def cmd(self, command, uri=None, pos=(0, 0), answers=None, extra=None, timeout=120, raw_args=None):
        answers = list(answers or [])
        if raw_args is not None:
            args = raw_args
        else:
            args = {'textDocument': {'uri': uri}, 'position': {'line': pos[0], 'character': pos[1]}}
            if extra:
                args.update(extra)
        trail = []
        for _ in range(40):
            r = self.request('workspace/executeCommand', {'command': command, 'arguments': [args]}, timeout=timeout)
            trail.append(r)
            if isinstance(r, dict) and 'input' in r and answers:
                inp = r['input']
                args = dict(args)
                for k, v in (inp.get('context') or {}).items():
                    args[k] = v
                args['input'] = answers.pop(0)
                continue
            break
        self.trail = trail
        return r

    def wait_messages(self, pred, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            for m in self.messages:
                if pred(m):
                    return m
            time.sleep(0.1)
        return None

    def settle(self, seconds=0.5):
        time.sleep(seconds)

    def close(self):
        try:
            self.request('shutdown', None, timeout=10)
            self.notify('exit', None)
        except Exception:
            pass
        try:
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()

    def emacs_children(self):
        out = subprocess.run(['ps', '-o', 'pid=,args=', '--ppid', str(self.proc.pid)], capture_output=True, text=True).stdout
        return [(int(l.split()[0]), l) for l in out.splitlines() if 'emacs' in l]


def probe_template(exprs, key='v'):
    """A capture template whose %(…) print EXPRS, one a line, for `probe`."""
    body = '\n'.join('@@%d=%%(format "%%S" %s)' % (i, e) for i, e in enumerate(exprs))
    return {'key': key, 'description': 'probe', 'type': 'plain', 'target': ['file', SCRATCH + '/probe-target.org'],
            'template': body}


def probe(client, uri, exprs, key='v'):
    """Run the probe template and read back each expression's printed value."""
    before = len(client.requests)
    r = client.cmd('org.capture', uri, (0, 0), answers=[key], timeout=120)
    shown = [p for m, p in client.requests[before:] if m == 'window/showDocument']
    if not shown:
        return {'__result__': r, '__messages__': client.messages[-5:]}
    path = shown[-1]['uri'][7:]
    text = open(path).read()
    out = {}
    import re
    for line in text.split('\n'):
        mm = re.match(r'@@(\d+)=(.*)', line)
        if mm:
            out[exprs[int(mm.group(1))]] = mm.group(2)
    client.cmd('org.capture.abort', shown[-1]['uri'], (0, 0))
    return out
