"""docs/config.schema.json and VS Code's org-lsp.settings schema for emacs_options:
each option's default validates, typos are refused, the two agree."""
import json, jsonschema
R = '/home/user/org-lsp/'
s = json.load(open(R + 'docs/config.schema.json'))
p = json.load(open(R + 'editors/vscode/package.json'))
cfg = p['contributes']['configuration']; cfg = cfg if isinstance(cfg, list) else [cfg]
vs = next(c['properties']['org-lsp.settings'] for c in cfg if 'org-lsp.settings' in c['properties'])
E = json.load(open(R + 'scripts/emacs-options.json'))
for label, root in [('config.schema.json', s), ('package.json', vs)]:
    V = jsonschema.Draft7Validator(root)
    bad = []
    n = 0
    for f, d in E.items():
        for name, v in d.items():
            if 'json' in v:
                n += 1
                errs = list(V.iter_errors({'emacs_options': {name: v['json']}}))
                if errs: bad.append((name, v['json'], errs[0].message[:120]))
    print(label, 'defaults checked', n, 'invalid', len(bad), bad[:5])
    for sample, want in [({'emacs_options': {'org-atach-method': 'mv'}}, False),
                         ({'emacs_options': {'org-attach-method': 'mv'}}, True),
                         ({'emacs_options': {'org-attach-method': 'move'}}, False),
                         ({'emacs_options': {'org-export-with-toc': 2}}, True),
                         ({'emacs_options': {'org-export-with-toc': 'yes'}}, False),
                         ({'emacs_options': {'org-babel-load-languages': {'python': True}}}, True),
                         ({'emacs_options': {'org-babel-load-languages': {'pyton': True}}}, None),
                         ({'emacs_options': {'org-export-backends': ['md', 'nosuch']}}, None),
                         ({'emacs_options': {'org-log-done': 'time'}}, None),
                         ({'emacs_options': []}, None),
                         ({'emacs_options': {'org-export-headline-levels': 2.5}}, False),
                         ({'emacs_options': {'org-odt-pixels-per-inch': 100}}, True),
                         ({'emacs_options': {'org-latex-classes': [['x', 'y']]}}, True),
                         ({'emacs_options': {'org-html-table-default-attributes': {'border': '1'}}}, True),
                         ]:
        ok = V.is_valid(sample)
        print('  ', 'valid' if ok else 'INVALID', json.dumps(sample), '' if want is None or ok == want else '<-- unexpected')
# do the two schemas agree on emacs_options?
a = s['properties']['emacs_options']; b = vs['properties']['emacs_options']
print('same emacs_options schema in both:', a == b)
if a != b:
    for k in set(a['properties']) | set(b['properties']):
        if a['properties'].get(k) != b['properties'].get(k): print('  differs', k); break
    print({k: v for k, v in a.items() if k != 'properties'}, {k: v for k, v in b.items() if k != 'properties'})
