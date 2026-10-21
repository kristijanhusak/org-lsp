// Batch 6: detangle from the Org file, export of a subtree as text,
// Markdown through emacs_config, clock tables, reveal, column view sums,
// the agenda's column view and super agenda, capture %[file] and %c,
// refile with a note, BibTeX, the formula debugger.
const fs = require('node:fs');
const path = require('node:path');

const read = (ctx, name) => {
  try {
    return fs.readFileSync(path.join(ctx.ws, name), 'utf8');
  } catch (e) {
    return `<${e.code}>`;
  }
};
const back = (name) => ({ label: `back to ${name}`, run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, name))); }, wait: 600 });

module.exports = [
  {
    name: 'detangle',
    file: 'lob.org',
    content: '#+begin_src emacs-lisp :tangle out.el :comments link\n(message "a")\n#+end_src\n',
    steps: [
      { at: [1, 0], command: 'org.babel.tangle', wait: 2500 },
      { label: 'change tangled', run: (ctx) => { const p = path.join(ctx.ws, 'out.el'); fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace('(message "a")', '(message "b")')); return fs.readFileSync(p, 'utf8'); } },
      { at: [1, 0], command: 'org.babel.detangle', answers: ['out.el', { choice: 'out.el' }], wait: 3500, after: (ctx) => read(ctx, 'lob.org') },
    ],
  },
  {
    name: 'export-subtree-text-markdown',
    file: 'ex.org',
    content: '* One\nfirst\n* Two\nsecond\n',
    steps: [
      { label: 'unsaved edit', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(new ctx.vscode.Position(3, 0), 'UNSAVED ')); } },
      { at: [3, 0], command: 'org.export.dispatch', answers: ['\u0013', 't', 'U'], wait: 6000 },
      back('ex.org'),
      { at: [0, 0], command: 'org.export.dispatch', answers: ['m', 'm'], wait: 6000, after: (ctx) => read(ctx, 'ex.md') },
      back('ex.org'),
      { at: [0, 0], command: 'org.export.dispatch', answers: ['&', { choice: 'ex.md' }], wait: 3000 },
    ],
  },
  {
    name: 'clocktable-update-shift',
    file: 'ct.org',
    content: '* A\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 08:00]--[2026-05-18 Mon 09:30] =>  1:30\nCLOCK: [2026-05-11 Mon 08:00]--[2026-05-11 Mon 09:00] =>  1:00\n:END:\n#+BEGIN: clocktable :scope file :block thisweek\n#+END:\n',
    steps: [
      { at: [5, 3], command: 'org.dblock.update', wait: 1500 },
      { at: [5, 40], command: 'org.shift.left', wait: 1500 },
      { at: [5, 3], command: 'org.clock.report', wait: 1500 },
    ],
  },
  {
    name: 'reveal-global-cycle',
    file: 'rv.org',
    content: 'intro\n* A\n** A1\ndeep text\n* B\n** B1\n',
    steps: [
      { at: [0, 0], command: 'org-lsp.cycle', wait: 1000, after: (ctx) => ctx.vscode.window.activeTextEditor.visibleRanges.map((r) => [r.start.line, r.end.line]) },
      { at: [3, 2], command: 'org.reveal', wait: 1000, after: (ctx) => ctx.vscode.window.activeTextEditor.visibleRanges.map((r) => [r.start.line, r.end.line]) },
    ],
  },
  {
    name: 'columns-effort-sum',
    file: 'col.org',
    content: '#+COLUMNS: %25ITEM %Effort{:}\n* Parent\n** A\n:PROPERTIES:\n:Effort:   1:00\n:END:\n** B\n:PROPERTIES:\n:Effort:   0:30\n:END:\n',
    steps: [
      { at: [1, 2], command: 'org.columns', wait: 3000, after: (ctx) => ({ header: ctx.api.columns?.(), shown: ctx.api.columnsShown?.(ctx.vscode.window.activeTextEditor) }) },
      { at: [2, 3], command: 'org-lsp.columns.right', wait: 600 },
      { at: [2, 3], command: 'org.columns.edit', answers: ['2:00'], wait: 3000, after: (ctx) => ctx.api.columnsShown?.(ctx.vscode.window.activeTextEditor) },
      { command: 'org.columns.show', wait: 1200 },
      { command: 'org.columns.quit', wait: 1000 },
    ],
  },
  {
    name: 'capture-file-clipboard',
    files: { 'snippet.txt': 'snippet text\n' },
    file: 'cap.org',
    content: '* Here\n',
    steps: [
      { label: 'clipboard', run: async (ctx) => { await ctx.vscode.env.clipboard.writeText('copied words'); } },
      { at: [0, 2], command: 'org.capture', answers: ['f'], wait: 2500 },
      { label: 'finish', command: 'org.capture.finish', wait: 1500, after: (ctx) => read(ctx, 'inbox.org') },
    ],
  },
  {
    name: 'refile-note',
    file: 'rf.org',
    content: '* Inbox\n** Item\n* Projects\n',
    steps: [
      { at: [1, 3], command: 'org.refile', answers: [{ choice: 'Projects' }], wait: 2000 },
      { label: 'type note', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(e.selection.active, 'Why')); return path.basename(e.document.uri.fsPath); } },
      { label: 'finish', command: 'org.capture.finish', wait: 1500 },
    ],
  },
  {
    name: 'bibtex',
    file: 'bib.org',
    content: '* The TeXbook\n:PROPERTIES:\n:TITLE:    The TeXbook\n:BTYPE:    book\n:CUSTOM_ID: knuth84\n:AUTHOR:   Knuth, Donald\n:PUBLISHER: AW\n:YEAR:     1984\n:END:\n',
    steps: [
      { at: [0, 2], command: 'org.bibtex.copy', wait: 2000, after: async (ctx) => ctx.vscode.env.clipboard.readText() },
      { at: [0, 2], command: 'org.bibtex.export', answers: (ctx) => [path.join(ctx.ws, 'out.bib'), 'y', 'yes'], wait: 2500, after: (ctx) => read(ctx, 'out.bib') },
      { label: 'clip', run: async (ctx) => { await ctx.vscode.env.clipboard.writeText('@article{doe20,\n  title = {A Paper},\n  author = {Doe, Jane},\n  journal = {J},\n  year = {2020}\n}\n'); } },
      { at: [8, 5], command: 'org.bibtex.yank', wait: 2500 },
      { at: [0, 0], command: 'org.bibtex.create', answers: ['article', 'T2', 'Roe, R', 'Jour', '2021', 'roe21', '', '', '', '', '', '', ''], wait: 3000 },
    ],
  },
  {
    name: 'formula-debugger',
    file: 'fd.org',
    content: '| 1 | 2 |   |\n#+TBLFM: $3=$1+$2\n',
    steps: [
      { at: [0, 2], command: 'org.table.formula.debugger', wait: 1000 },
      { at: [0, 2], command: 'org.table.recalculate', answers: ['y', 'yes', 'Yes', 'y', 'y'], wait: 3000 },
      { at: [0, 2], command: 'org.table.formula.debugger', wait: 1000 },
    ],
  },
  {
    name: 'agenda-columns-super-prefix',
    file: 'ag.org',
    content: '* TODO Alpha\nSCHEDULED: <2026-05-18 Mon>\n:PROPERTIES:\n:Effort:   1:00\n:END:\n* TODO Beta\n',
    steps: [
      { command: 'org.agenda.todos', wait: 2500 },
      { command: 'org.agenda.list', wait: 2000 },
      { label: 'to alpha', run: (ctx) => { const e = ctx.vscode.window.activeTextEditor; const lines = e.document.getText().split('\n'); const i = lines.findIndex((l) => l.includes('Alpha')); e.selection = new ctx.vscode.Selection(i, 0, i, 0); return i; } },
      { command: 'org.agenda.columns', wait: 3000, after: (ctx) => ({ header: ctx.api.columns?.(), shown: ctx.api.columnsShown?.(ctx.vscode.window.activeTextEditor) }) },
      { command: 'org.columns.quit', wait: 1000 },
    ],
  },
];
