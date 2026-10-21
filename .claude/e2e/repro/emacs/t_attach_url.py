"""C-c C-a u: org-attach-url, with org_resource_download_policy / org_safe_remote_resources set as settings."""
import sys, json, os, shutil; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
D = SCRATCH + '/attachu'; shutil.rmtree(D, ignore_errors=True); os.makedirs(D)
for s in [{}, {'org_resource_download_policy': True}, {'org_safe_remote_resources': ['\\`file:///etc/']}]:
    c = Client(settings=dict({'message_prompts': False, 'emacs_config': LOAD}, **s))
    u = c.open(D + '/t.org', '* Entry\n')
    c.cmd('org.attach', u, (0, 2), answers=['u', 'file:///etc/hostname']); c.settle(2)
    print(s, c.messages, 'attached:', os.popen(f'find {D}/data -type f 2>/dev/null').read().strip())
    c.close()
