"""Compare scripts/emacs-options.json with what Emacs says (dump_options.el)."""
import json, re, sys
S = '/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/emacs/'
E = {}
for f, d in json.load(open('/home/user/org-lsp/scripts/emacs-options.json')).items():
    for n, v in d.items(): E[n] = v
em = {}
for line in open(S + 'emacs-dump.jsonl'):
    o = json.loads(line); em[o['name']] = o
ws = lambda s: ' '.join(s.split())
cnt = {'doc': 0, 'default': 0, 'type': 0, 'version': 0}
for n, v in E.items():
    o = em[n]
    if ws(v['doc']) != ws(o['doc']):
        cnt['doc'] += 1
        if cnt['doc'] <= 6:
            a, b = ws(v['doc']), ws(o['doc'])
            i = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b)))
            print('DOC', n, '| page:', a[max(0, i-40):i+60], '| emacs:', b[max(0, i-40):i+60])
    d1 = ws(v.get('default', 'MISSING')); d2 = ws(o['default'].replace('\\n', '\n'))
    if d1 != d2 and ws(v.get('default','MISSING').replace('\n', '\\n')) != ws(o['default']):
        cnt['default'] += 1
        if cnt['default'] <= 8: print('DEFAULT', n, '| page:', d1[:150], '| emacs:', d2[:150])
    t1, t2 = ws(v['type']), ws(o['type'].replace('\\n', '\n'))
    if t1 != t2 and re.sub(r'\s', '', t1) != re.sub(r'\s', '', t2):
        cnt['type'] += 1
        if cnt['type'] <= 5: print('TYPE', n, '| page:', t1[:150], '| emacs:', t2[:150])
    ver = v.get('version')
    ev = o['version'] if o['version'] != 'nil' else None
    pk = o['pkg'] if o['pkg'] != 'nil' else None
    exp = None
    if pk:
        m = re.match(r'\((\S+) \. "(.*)"\)', pk)
        exp = f'{m.group(1)} {m.group(2)}' if m else pk
    elif ev:
        exp = 'Emacs ' + ev.strip('"')
    if (ver or None) != exp:
        cnt['version'] += 1
        if cnt['version'] <= 8: print('VERSION', n, '| page:', ver, '| emacs pkg/ver:', pk, ev)
print(cnt, 'of', len(E))
