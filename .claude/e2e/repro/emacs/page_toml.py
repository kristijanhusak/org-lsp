"""Every TOML example on docs/emacs-options.org parses, and equals the `json` of emacs-options.json."""
import re, json, tomllib
t = open('/home/user/org-lsp/docs/emacs-options.org').read()
E = {n: v for d in json.load(open('/home/user/org-lsp/scripts/emacs-options.json')).values() for n, v in d.items()}
blocks = re.findall(r'#\+begin_src toml\n(.*?)#\+end_src', t, re.S)
seen, bad = set(), 0
for b in blocks:
    try:
        d = tomllib.loads(b)
    except Exception as e:
        print('PARSE', e, b[:200]); bad += 1; continue
    for n, v in d.get('emacs_options', {}).items():
        seen.add(n)
        if n not in E: print('UNKNOWN', n); bad += 1
        elif E[n].get('json') != v: print('DIFF', n, v, E[n].get('json')); bad += 1
with_json = {n for n, v in E.items() if 'json' in v}
print('blocks', len(blocks), 'options in them', len(seen), 'bad', bad, 'json without example on page:', sorted(with_json - seen)[:10])
