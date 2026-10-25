"""Removing the last org-roam alias of a file node: the blank line after
`#+title:` goes too. Run with python3 roam_alias.py (the LSP client of
.claude/e2e/repro/lsp)."""
import os
import shutil
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'lsp'))
from client import FULL, SCRATCH, Client  # noqa: E402

root = os.path.join(SCRATCH, 'roam-alias')
shutil.rmtree(root, ignore_errors=True)
os.makedirs(os.path.join(root, 'roam'))
text = ':PROPERTIES:\n:ID:       b-id\n:ROAM_ALIASES: Bee\n:END:\n#+title: Beta note\n\nSee it.\n'
path = os.path.join(root, 'roam', 'b.org')
open(path, 'w').write(text)
c = Client(profile=FULL, root=root, options={'org_roam_enabled': True, 'org_roam_directory': os.path.join(root, 'roam')}, name='alias')
c.open(path)
result, steps = c.run('org.roam.alias.remove', c.at(path, 6, 0), answers=['Bee'])
c.settle(0.5, 5)
print('asked:', [s.get('input', {}).get('prompt') for s in steps if isinstance(s, dict)])
print('result:', result)
print('before:', repr(text))
print('after: ', repr(c.text(path)))
c.shutdown()
