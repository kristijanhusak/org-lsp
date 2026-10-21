import sys, json, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
for ans in ['No', 'Yes']:
    c = Client(settings={'message_prompts': False, 'emacs_config': LOAD}, request_answers=[ans])
    u = c.open(SCRATCH + '/timer/cd2.org', '* Task\n')
    c.cmd('org.timer.set', u, (0, 3), answers=['1']); c.settle(1)
    c.cmd('org.timer.set', u, (0, 3), answers=['2']); c.settle(2)
    print(ans, c.messages, [x.get('params') for x in c.notifications if x['method'] == 'org-lsp/timer'][-1])
    c.close()
