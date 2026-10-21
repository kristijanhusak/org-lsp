"""org-crypt through the server: encrypt/decrypt with a key (emacs_options
org-crypt-key), symmetric (no key), and the before-save magic (latency of
willSaveWaitUntil)."""
import sys, json, os, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
G = open(SCRATCH + '/gnupghome').read().strip()
caps = {'workspace': {'applyEdit': True, 'workspaceEdit': {'documentChanges': True}},
        'window': {'showDocument': {'support': True}},
        'textDocument': {'synchronization': {'willSave': True, 'willSaveWaitUntil': True}}}
DOC = '* Secret :crypt:\nmy password\n* Plain\ntext\n'

def run(label, opts, config='', saves=False):
    c = Client(settings={'message_prompts': False, 'emacs_config': LOAD + config, 'emacs_options': opts},
               env={'GNUPGHOME': G}, capabilities=caps)
    u = c.open(SCRATCH + f'/crypt/{label}.org', DOC)
    print('==', label)
    if saves:
        for i in range(2):
            t = time.time()
            r = c.request('textDocument/willSaveWaitUntil', {'textDocument': {'uri': u}, 'reason': 1}, timeout=120)
            print(f'   willSaveWaitUntil #{i+1}: {round(time.time()-t, 2)}s, edits: {bool(r)}',
                  (r[0]['newText'][:60] if r else ''), c.messages[-1:] if c.messages else '')
            if r: c._apply_text_edits(u, r)
        print('   text now:', c.text(u)[:120].replace('\n', '|'))
    else:
        r = c.cmd('org.crypt.encrypt', u, (0, 2)); c.settle(3)
        print('   encrypt ->', r, c.messages, '\n   ', c.text(u)[:160].replace('\n', '|'))
        r = c.cmd('org.crypt.decrypt', u, (0, 2)); c.settle(3)
        print('   decrypt ->', r, c.messages[-2:], '\n   ', c.text(u)[:160].replace('\n', '|'))
        c.change(u, DOC)
        r = c.cmd('org.crypt.encrypt.all', u, (0, 0)); c.settle(3)
        print('   encrypt.all ->', r, '\n   ', c.text(u)[:100].replace('\n', '|'))
        r = c.cmd('org.crypt.decrypt.all', u, (0, 0)); c.settle(3)
        print('   decrypt.all ->', r, c.messages[-1:], '\n   ', c.text(u)[:100].replace('\n', '|'))
    c.close()

run('key', {'org-crypt-key': 'tester@example.org'})
run('symmetric', {})
run('magic', {'org-crypt-key': 'tester@example.org'}, "\n(require 'org-crypt)\n(org-crypt-use-before-save-magic)", saves=True)
