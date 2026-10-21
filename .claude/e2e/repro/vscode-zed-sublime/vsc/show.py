import json,sys
for c in json.load(open(sys.argv[1])):
    if len(sys.argv)>2 and sys.argv[2] not in c['name']: continue
    print('=====',c['name'], c.get('error','')[:800])
    for s in c['steps']:
        print('--',s['label'],'| active:',s['active'],'cursor',s['cursor'])
        for e in s['log']:
            print('   ',json.dumps(e,ensure_ascii=False)[:400])
        t=s.get('text')
        if t is not None and not (s['active'] or '').startswith('org-agenda'): print('   TEXT:',json.dumps(t,ensure_ascii=False)[:600])
        elif t: print('   TEXT:\n'+t[:1500])
        if 'after' in s: print('   AFTER:',json.dumps(s['after'],ensure_ascii=False)[:600])
        if s.get('leftover'): print('   LEFTOVER',s['leftover'])
    if c.get('check'): print('  CHECK',c['check'])
    for o in c.get('output',[]): print('  OUT',o[:300])
