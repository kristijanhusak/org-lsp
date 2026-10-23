import sys, json, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': {'org-timer-default-timer': '0:00:03'}})
u = c.open(SCRATCH + '/timer/cd.org', '* Task\n')
r = c.cmd('org.timer.set', u, (0, 3), answers=['']); c.settle(1)
n = lambda: [x.get('params') for x in c.notifications if x['method'] == 'org-lsp/timer']
print('set ->', r, n(), c.messages)
time.sleep(7)
print('after 7s', n(), c.messages)
r = c.cmd('org.timer.set', u, (0, 3), answers=['1']); c.settle(1)
print('set 1 min ->', r, n()[-1], c.messages[-1:])
r = c.cmd('org.timer.set', u, (0, 3), answers=['2'], ); c.settle(1)
print('set again ->', json.dumps(c.trail)[:200], c.messages[-2:])
c.close()
