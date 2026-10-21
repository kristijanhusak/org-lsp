"""What else goes through Emacs: capture %(sexp) and function targets,
refile verify function, diary sexps, agenda prefix %(…), link abbrev functions."""
import sys, json, os, time, datetime; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
D = SCRATCH + '/lisp'; os.makedirs(D, exist_ok=True)
CONFIG = LOAD + r'''
(defun my-target () (goto-char (point-max)))
(defun my-visit () (find-file "%s/visited.org") (goto-char (point-max)))
(defun my-verify () (not (member "skip" (org-get-tags))))
(defun my-abbrev (tag) (concat "https://example.org/" (upcase tag)))
(put 'my-abbrev 'org-link-abbrev-safe t)
(defun my-diary (d) (and (= 21 (cadr d)) "Diary from Lisp"))
(defun my-prefix () (format "<%%s>" (or (org-entry-get nil "WHO") "-")))
''' % D
today = datetime.date.today()
AG = f"""* Meeting
SCHEDULED: <{today.isoformat()} {today.strftime('%a')} 10:00>
:PROPERTIES:
:WHO: bob
:END:
* Diary
%%(my-diary date)
%%(diary-float t 3 3) Third Wednesday
%%(error "boom")
* Target
* Skipped :skip:
* Other
"""
open(D + '/visited.org', 'w').write('* Visited\n')
c = Client(settings={'message_prompts': False, 'emacs_config': CONFIG, 'org_agenda_files': [D + '/ag.org'],
                     'org_link_abbrev_alist': {'ex': '%(my-abbrev)'},
                     'org_refile_target_verify_function': 'my-verify',
                     'org_refile_targets': [{'files': 'agenda', 'maxlevel': 2}],
                     'org_agenda_prefix_format': {'agenda': ' %(my-prefix) %-8:c%?-12t% s'},
                     'org_capture_templates': [
                         {'key': 'a', 'description': 'sexp', 'type': 'entry', 'target': ['file+function', D + '/ag.org', 'my-target'],
                          'template': '* %(concat "cap" "tured") %^{Name} %(upcase "%\\1")'},
                         {'key': 'b', 'description': 'function target', 'type': 'entry', 'target': ['function', 'my-visit'],
                          'template': '* via function'},
                         {'key': 'c', 'description': 'error', 'type': 'plain', 'target': ['file', D + '/x.org'],
                          'template': 'x %(car 1) y'},
                         {'key': 'd', 'description': 'bad fn', 'type': 'entry', 'target': ['function', 'no-such-fn'],
                          'template': '* x'}]})
u = c.open(D + '/ag.org', AG)
def shown(n): return [p for m, p in c.requests[n:] if m == 'window/showDocument']
for key, ans in [('a', ['a', 'zed']), ('b', ['b']), ('c', ['c']), ('d', ['d'])]:
    n = len(c.requests); m = len(c.messages)
    r = c.cmd('org.capture', u, (0, 0), answers=ans); c.settle(1)
    d = shown(n)
    body = open(d[-1]['uri'][7:]).read() if d else None
    print(f'capture {key}: ->', json.dumps(r)[:200], '| buffer', d[-1]['uri'][-40:] if d else None, repr(body), c.messages[m:])
    if d:
        r = c.cmd('org.capture.finish', d[-1]['uri'], (0, 0)); c.settle(1)
        print('    finish ->', json.dumps(r)[:150], c.messages[m:])
print('applyEdit to visited:', [json.dumps(p)[:300] for m2, p in c.requests if m2 == 'workspace/applyEdit' and 'visited' in json.dumps(p)])
print('ag.org now:', repr(c.text(u)), 'visited.org:', repr(open(D + '/visited.org').read()))
m = len(c.messages)
r = c.cmd('org.refile', u, (0, 2)); print('refile targets ->', [ch.get('label') for ch in (r or {}).get('input', {}).get('choices', [])] if isinstance(r, dict) else r, c.messages[m:])
# link abbrev
c.change(u, c.text(u) + '\n[[ex:foo]]\n')
line = c.text(u).split('\n').index('[[ex:foo]]')
n = len(c.requests); m = len(c.messages)
r = c.cmd('org.open', u, (line, 3)); c.settle(1)
print('open ex:foo ->', json.dumps(r)[:200], shown(n), c.messages[m:])
# agenda day view
n = len(c.requests); m = len(c.messages)
r = c.cmd('org.agenda.dispatch', u, (0, 0), answers=['a']); c.settle(4)
d = shown(n); au = d[-1]['uri'] if d else None
print('agenda ->', au)
if au:
    print(open(au[7:]).read())
print('msgs', c.messages[m:])
c.close()
