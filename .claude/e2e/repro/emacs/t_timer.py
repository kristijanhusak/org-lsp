"""Timers through Emacs: relative timer, countdown, org-timer-display / org-timer-default-timer via emacs_options."""
import sys, json, os, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-timer-default-timer': '0:00:03', 'org-timer-format': '[%s] '}})
u = c.open(SCRATCH + '/timer/t.org', '* Notes\n\n')
def timer():
    n = [x for x in c.notifications if x['method'] == 'org-lsp/timer']
    return n[-1].get('params') if n else 'none'
for cmd, ans in [('org.timer.start', []), ('org.timer.insert', []), ('org.timer.pause', []), ('org.timer.pause', []),
                 ('org.timer.item', []), ('org.timer.stop', []), ('org.timer.insert', [])]:
    m = len(c.messages)
    r = c.cmd(cmd, u, (1, 0), answers=ans); c.settle(1.2)
    print(cmd, '->', json.dumps(r)[:150], '| timer', timer(), '| msgs', c.messages[m:])
print(repr(c.text(u)))
m = len(c.messages)
r = c.cmd('org.timer.set', u, (1, 0)); print('timer.set ->', json.dumps(r)[:300])
r = c.cmd('org.timer.set', u, (1, 0), answers=['']); c.settle(1)
print('timer.set default ->', json.dumps(c.trail)[:300], timer(), c.messages[m:])
time.sleep(6)
print('after 6s', timer(), c.messages[m:])
c.close()
