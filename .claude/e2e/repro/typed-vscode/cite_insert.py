"""org.cite.insert: what it asks, and what an empty answer does.
python3 cite_insert.py"""
import json
import os
import shutil
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'lsp'))
from client import FULL, SCRATCH, Client  # noqa: E402

root = os.path.join(SCRATCH, 'cite-insert')
shutil.rmtree(root, ignore_errors=True)
os.makedirs(root)
open(os.path.join(root, 'refs.bib'), 'w').write('@book{knuth84,\n  author = {Donald Knuth},\n  title = {The TeXbook},\n  year = 1984\n}\n@book{lamport94,\n  author = {Leslie Lamport},\n  title = {LaTeX},\n  year = 1994\n}\n')
path = os.path.join(root, 'c.org')
open(path, 'w').write('#+bibliography: refs.bib\n* Cite\nSee \n')
c = Client(profile=FULL, root=root, options={}, name='cite')
c.open(path)
seen = []


def answer(value):
    def give(inp):
        seen.append({k: inp.get(k) for k in ('kind', 'prompt', 'value', 'select', 'choices', 'completions')})
        return value
    return give


result, steps = c.run('org.cite.insert', c.at(path, 2, 4), answers=[answer('knuth84'), answer('')])
for s in seen:
    print(json.dumps(s)[:600])
print('result:', json.dumps(result)[:300])
print(repr(c.text(path)))
c.shutdown()
