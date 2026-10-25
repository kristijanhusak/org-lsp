"""S-RET (org.table.copy.down) at the start of a table row, and below the
table: what the server answers and says. python3 copy_down.py"""
import os
import shutil
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'lsp'))
from client import FULL, SCRATCH, Client  # noqa: E402

root = os.path.join(SCRATCH, 'copy-down')
shutil.rmtree(root, ignore_errors=True)
os.makedirs(root)
path = os.path.join(root, 't.org')
open(path, 'w').write('| 1 | 2 |\n|   |   |\n\nafter\n')
c = Client(profile=FULL, root=root, options={}, name='copydown')
c.open(path)
for line, char in ((0, 2), (1, 0), (3, 2)):
    c.messages.clear()
    try:
        result = c.command('org.table.copy.down', c.at(path, line, char))
    except Exception as error:  # an error response
        result = f'error response: {getattr(error, "error", error)}'
    c.settle(0.5, 30)
    print((line, char), 'result:', result, 'messages:', c.messages)
print(repr(c.text(path)))
c.shutdown()
