"""Every TOML/JSON example (`json`) of scripts/emacs-options.json, put in
emacs_options: does Emacs end up with the option's standard value, with no
notice?"""
import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
E = json.load(open('/home/user/org-lsp/scripts/emacs-options.json'))
opts, feats = {}, {}
for f, d in E.items():
    for name, v in d.items():
        if 'json' in v:
            opts[name] = v['json']
            feats[name] = f[:-3]
exprs = []
for name, feat in feats.items():
    exprs.append(f"(condition-case e (progn (require (quote {feat})) "
                 f"(if (equal {name} (eval (car (get (quote {name}) (quote standard-value))) t)) \"same\" "
                 f"(replace-regexp-in-string \"\\n\" \"\\\\\\\\n\" (format \"%S\" {name})))) (error (format \"ERR %S\" e)))")
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': opts,
                     'org_capture_templates': [probe_template(exprs)]})
u = c.open(SCRATCH + '/rt.org', '* H\n')
res = probe(c, u, exprs)
c.settle(2)
names = list(feats)
bad = 0
for i, e in enumerate(exprs):
    v = res.get(e)
    if v != '"same"':
        bad += 1
        print('DIFF', names[i], 'example:', json.dumps(opts[names[i]])[:200], '-> emacs:', (v or 'MISSING')[:300])
print('checked', len(exprs), 'differ', bad)
print('messages:')
for m in c.messages:
    print(' ', m)
c.close()
