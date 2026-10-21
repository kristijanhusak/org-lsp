import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
c = Client(settings={'message_prompts': False})
for i, doc in enumerate(["| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n#+TBLFM: $3=$1*$2::@3$1=7\n",
                         "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n#+TBLFM: $3=$1*$2\n",
                         "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n#+TBLFM: @3$1=7\n",
                         "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n#+TBLFM: $3=$1*$2::@2$1=7\n"]):
    u = c.open(SCRATCH + f'/edit/r{i}.org', doc)
    m = len(c.messages); r = c.cmd('org.table.recalculate', u, (2, 2)); c.settle(1.5)
    print(repr(doc.split('#+TBLFM: ')[1]), '->', r, repr(c.text(u).split('\n')[2]), c.messages[m:])
c.close()
