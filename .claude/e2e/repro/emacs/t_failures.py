"""Emacs failure modes: a Lisp error in emacs_config, a slow emacs_config,
Emacs killed mid-command, a stuck synchronous call; and whether the server
answers other requests meanwhile."""
import sys, json, os, time, threading, signal; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
which = sys.argv[1:] or ['error', 'slow', 'kill', 'stuck']
DOC = '* H\n#+begin_src emacs-lisp\n(sleep-for 20)\n#+end_src\n'
TPL = [{'key': 'l', 'description': 'loop', 'type': 'plain', 'target': ['file', SCRATCH + '/fail-target.org'],
        'template': 'x %(while t)'},
       {'key': 'o', 'description': 'ok', 'type': 'plain', 'target': ['file', SCRATCH + '/fail-target.org'],
        'template': 'x %(+ 1 2)'}]

def symbols_latency(c, u):
    t = time.time(); c.request('textDocument/documentSymbol', {'textDocument': {'uri': u}}, timeout=300)
    return round(time.time() - t, 2)

def concurrently(c, u, fn):
    out = {}
    th = threading.Thread(target=lambda: out.setdefault('r', fn()))
    t0 = time.time(); th.start(); time.sleep(0.5)
    out['symbols_latency'] = symbols_latency(c, u)
    th.join(400); out['took'] = round(time.time() - t0, 1)
    return out

if 'error' in which:
    c = Client(settings={'message_prompts': False, 'emacs_config': LOAD + '\n(car 1)\n',
                         'org_capture_templates': TPL})
    u = c.open(SCRATCH + '/fail.org', DOC)
    r = c.cmd('org.export.dispatch', u, (0, 0)); print('error config, export menu ->', json.dumps(r)[:400])
    r = c.cmd('org.capture', u, (0, 0), answers=['o']); print('error config, capture ->', json.dumps(r)[:300])
    print('   msgs', c.messages); c.close()

if 'slow' in which:
    for secs in [40, 130]:
        c = Client(settings={'message_prompts': False, 'emacs_config': LOAD + f'\n(sleep-for {secs})\n',
                             'org_capture_templates': TPL})
        u = c.open(SCRATCH + '/fail.org', DOC)
        out = concurrently(c, u, lambda: c.cmd('org.export.dispatch', u, (0, 0), timeout=400))
        print(f'slow config {secs}s: export menu ->', json.dumps(out['r'])[:200], '| took', out['took'],
              's | documentSymbol meanwhile took', out['symbols_latency'], 's')
        print('   msgs', c.messages); c.close()

if 'kill' in which:
    c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                         'emacs_options': {'org-confirm-babel-evaluate': False}, 'org_capture_templates': TPL})
    u = c.open(SCRATCH + '/fail.org', DOC)
    c.cmd('org.babel.execute', u, (2, 0)); time.sleep(4)
    kids = c.emacs_children(); print('emacs children', [k[0] for k in kids])
    for pid, _ in kids: os.kill(pid, signal.SIGKILL)
    time.sleep(2); print('after kill msgs', c.messages)
    r = c.cmd('org.capture', u, (0, 0), answers=['o']); print('next command ->', json.dumps(r)[:200])
    print('emacs children now', [k[0] for k in c.emacs_children()], 'msgs', c.messages[-2:])
    c.close()

if 'stuck' in which:
    c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'org_capture_templates': TPL})
    u = c.open(SCRATCH + '/fail.org', DOC)
    c.cmd('org.capture', u, (0, 0), answers=['o'])  # Emacs is up
    first = [k[0] for k in c.emacs_children()]
    out = concurrently(c, u, lambda: c.cmd('org.capture', u, (0, 0), answers=['l'], timeout=200))
    print('stuck %(while t): ->', json.dumps(out['r'])[:300], '| took', out['took'], 's | documentSymbol meanwhile', out['symbols_latency'], 's')
    time.sleep(1)
    print('emacs before', first, 'after', [k[0] for k in c.emacs_children()])
    r = c.cmd('org.capture', u, (0, 0), answers=['o']); print('next ->', json.dumps(r)[:200], [k[0] for k in c.emacs_children()])
    print('   msgs', c.messages); c.close()
