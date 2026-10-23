"""emacs_options from a config_file (JSON) and from $ORG_LSP_CONFIG (TOML); a notice that comes mid-command (ob-python)."""
import sys, json, os; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
D = SCRATCH + '/cfg'; os.makedirs(D, exist_ok=True)
json.dump({'emacs_config': LOAD, 'emacs_options': {'org-attach-method': 'ln', 'org-babel-python-command': 7,
           'org-confirm-babel-evaluate': False, 'org-babel-load-languages': {'python': True}}}, open(D + '/c.json', 'w'))
open(D + '/c.toml', 'w').write(f"emacs_config = '{LOAD}'\n[emacs_options]\norg-attach-method = \"lns\"\n")
exprs = ['(progn (require (quote org-attach)) org-attach-method)']
for label, settings, env in [('config_file json', {'config_file': D + '/c.json'}, {}),
                             ('ORG_LSP_CONFIG toml', {}, {'ORG_LSP_CONFIG': D + '/c.toml'})]:
    s = dict(settings, message_prompts=False, org_capture_templates=[probe_template(exprs)])
    c = Client(settings=s, env=env)
    u = c.open(D + '/p.org', '* H\n#+begin_src python\nprint(1)\n#+end_src\n')
    print(label, probe(c, u, exprs), c.messages)
    m = len(c.messages)
    c.cmd('org.babel.execute', u, (2, 0)); c.settle(4)
    print('   babel python ->', c.messages[m:])
    c.close()
