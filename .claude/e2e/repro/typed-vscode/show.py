"""show.py OUT.json [NAME]: what each step of the typed explorer showed.
The active editor's text only with TEXT=1."""
import json
import os
import sys

for c in json.load(open(sys.argv[1])):
    if len(sys.argv) > 2 and sys.argv[2] not in c['name']:
        continue
    print('=====', c['name'], c.get('error', '')[:800])
    for s in c['steps']:
        print('--', s['label'], '| active:', s['active'], 'cursor', s['cursor'])
        for e in s['log']:
            print('   ', json.dumps(e, ensure_ascii=False)[:400])
        t = s.get('text')
        if t and os.environ.get('TEXT'):
            print('   TEXT:\n' + t[:1500])
        if 'after' in s:
            a = s['after']
            print('   AFTER:', a if isinstance(a, str) and '\n' in a else json.dumps(a, ensure_ascii=False)[:1200])
        if s.get('leftover'):
            print('   LEFTOVER', s['leftover'])
    for o in c.get('output', []):
        print('  OUT', o[:300])
