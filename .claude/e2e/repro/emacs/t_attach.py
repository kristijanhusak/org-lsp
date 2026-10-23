"""The attachment dispatcher with emacs_options (org-attach-method,
org-attach-id-dir), key by key."""
import sys, json, os, shutil, time, subprocess; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
D = SCRATCH + '/attach'
shutil.rmtree(D, ignore_errors=True); os.makedirs(D)
for n in ['a.txt', 'c.txt', 'l.txt', 'y.txt', 'm.txt']:
    open(f'{D}/{n}', 'w').write(n + '\n')
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-attach-method': 'mv', 'org-attach-id-dir': 'data/'}},
           request_answers=[])
u = c.open(D + '/t.org', '* Entry\nbody\n* Other\n')

def step(keys, label, pos=(0, 2), answers_for_requests=None):
    if answers_for_requests is not None:
        c.request_answers[:] = answers_for_requests
    n, m, k = len(c.requests), len(c.messages), len(c.requests)
    r = c.cmd('org.attach', u, pos, answers=keys)
    c.settle(1.5)
    print(f'== {label} {keys!r}')
    for t in c.trail:
        print('   ->', json.dumps(t, ensure_ascii=False)[:250])
    for x in c.messages[m:]: print('   msg', x)
    for meth, p in c.requests[k:]:
        if meth != 'workspace/applyEdit': print('   req', meth, json.dumps(p)[:200])
    return r

step([], 'menu')
step(['a', 'a.txt'], 'attach (method mv)')
print(c.text(u))
print('a.txt still in dir:', os.path.exists(D + '/a.txt'))
print(subprocess.run(['find', D + '/data'], capture_output=True, text=True).stdout)
step(['c', 'c.txt'], 'copy')
step(['l', 'l.txt'], 'hard link')
step(['y', 'y.txt'], 'symlink')
step(['m', 'm.txt'], 'move')
print(subprocess.run(['find', D + '/data', '-printf', '%p %y %n\n'], capture_output=True, text=True).stdout)
step(['n', 'new.txt'], 'new')
step(['u', 'file:///etc/hostname'], 'url')
step(['z'], 'sync')
step(['o'], 'open')
step(['o', 'c.txt'], 'open one')
step(['f'], 'reveal')
step(['d'], 'delete one (menu)')
step(['d', 'c.txt'], 'delete one')
step(['D'], 'delete all, answered No', answers_for_requests=['No'])
step(['D'], 'delete all, answered Yes', answers_for_requests=['Yes'])
print(c.text(u))
step(['s', D + '/mydir'], 'set dir', pos=(2, 2))
step(['S'], 'unset dir', pos=(2, 2), answers_for_requests=['Yes'])
step(['q'], 'quit')
step(['x'], 'bad key')
print(c.text(u))
c.close()
