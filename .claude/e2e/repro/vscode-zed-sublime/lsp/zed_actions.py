"""The code actions the server offers Zed (initialization options as the
Zed extension sends them) at a few places, against what editors/zed/TESTING.org
says is, or is not, among Zed's code actions or keys."""
import json, os, re, sys
sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/lsp')
from client import *
ws = os.path.join(SCRATCH, '..', 'vscode-zed-sublime', 'lsp-zed-actions')
os.makedirs(ws, exist_ok=True)
files = {
    'a.org': '#+TODO: TODO | DONE\n* TODO Head :tag:\n:PROPERTIES:\n:Status_ALL: a b\n:Status: a\n:END:\nSCHEDULED: <2026-05-18 Mon>\n- item\n| a | b |\n|---+---|\n| 1 | 2 |\n#+begin_src emacs-lisp :tangle x.el\n(+ 1 2)\n#+end_src\n[fn:1] def\n* Second\n',
}
for n, t in files.items():
    open(os.path.join(ws, n), 'w').write(t)
opts = {'virtual_documents': False, 'org_client': True, 'command_actions': True, 'message_prompts': True,
        'clock': {'today': '2026-05-18', 'now': '10:30'}, 'org_roam_enabled': True, 'org_roam_directory': ws,
        'org_journal_enabled': True, 'org_journal_dir': ws}
c = Client(profile=PLAIN, root=ws, options=opts, name='zedactions')
g = c.open(os.path.join(ws, 'a.org')); c.settle(0.5, 5)
seen = set()
lines = c.text(g).split('\n')
for line in range(len(lines)):
    for ch in (0, 3):
        r = c.request('textDocument/codeAction', {'textDocument': {'uri': g}, 'range': {'start': {'line': line, 'character': ch}, 'end': {'line': line, 'character': ch}}, 'context': {'diagnostics': []}})
        for a in r or []:
            cmd = (a.get('command') or {}).get('command') or a.get('kind')
            seen.add(cmd)
print(len(seen), 'commands among the code actions')
keymap = open('/home/user/org-lsp/editors/zed/keymap.json').read()
tasks = json.load(open('/home/user/org-lsp/editors/zed/tasks.json'))
task_cmds = {t['command'].split()[-1] for t in tasks}
testing = open('/home/user/org-lsp/editors/zed/TESTING.org').read()
manifest = json.loads(subprocess.run([SERVER, 'manifest'], capture_output=True, text=True).stdout)
titles = {}
for cmd in manifest.get('commands', []):
    titles[cmd.get('title')] = cmd.get('command') or cmd.get('name') or cmd.get('id')
# "is not among its keys or code actions" claims
for m in re.finditer(r'/([^/\n]+)/(?:, /([^/\n]+)/)? (?:is|are) not among its keys or code actions', testing):
    for title in filter(None, m.groups()):
        cmd = titles.get(title)
        print('NOT-IN-ZED', repr(title), cmd, 'in actions' if cmd in seen else '', 'in tasks' if cmd in task_cmds else '')
for m in re.finditer(r'the code action /([^/\n]+)/', testing):
    cmd = titles.get(m.group(1))
    if cmd not in seen:
        print('CLAIMED ACTION NOT SEEN here:', m.group(1), cmd)
c.shutdown()
