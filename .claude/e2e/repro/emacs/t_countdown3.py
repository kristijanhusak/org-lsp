import sys, json, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD})
u = c.open(SCRATCH + '/timer/cd3.org', '* Task\n')
c.cmd('org.timer.set', u, (0, 3), answers=['0:00:03']); c.settle(1)
print([x.get('params') for x in c.notifications if x['method'] == 'org-lsp/timer'], c.messages)
time.sleep(6)
print([x.get('params') for x in c.notifications if x['method'] == 'org-lsp/timer'], c.messages)
c.close()
