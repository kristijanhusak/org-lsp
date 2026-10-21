// Batch 3: org-roam, org-journal, tables, column view, properties, notes,
// countdown, export, babel, settings.json schema of emacs_options.
const fs = require('node:fs');
const path = require('node:path');

const read = (ctx, name) => {
  try {
    return fs.readFileSync(path.join(ctx.ws, name), 'utf8');
  } catch (e) {
    return `<${e.code}>`;
  }
};
const ls = (ctx, dir) => {
  try {
    return fs.readdirSync(path.join(ctx.ws, dir)).sort();
  } catch (e) {
    return `<${e.code}>`;
  }
};
const visible = (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => path.basename(e.document.uri.fsPath || e.document.uri.path));

module.exports = [
  {
    name: 'roam-find-insert-alias-tag',
    file: 'roam/a.org',
    steps: [
      { command: 'org.roam.node.find', answers: ['B note'], wait: 1000 },
      { label: 'insert', at: [5, 0], command: 'org.roam.node.insert', answers: ['A note'], wait: 1000 },
      { command: 'org.roam.alias.add', answers: ['Bee'], wait: 1000 },
      { command: 'org.roam.alias.remove', answers: ['Bee', { choice: 'Bee' }], wait: 1000 },
      { command: 'org.roam.tag.add', answers: ['tagx'], wait: 1000 },
      { command: 'org.roam.buffer.toggle', wait: 1500, after: visible },
      { command: 'org.roam.buffer.toggle', wait: 1000, after: visible },
    ],
  },
  {
    name: 'roam-new-node-and-dailies',
    file: 'roam/a.org',
    steps: [
      { at: [5, 0], command: 'org.roam.node.insert', answers: ['Brand new'], wait: 1500 },
      { label: 'finish new', command: 'org.capture.finish', wait: 1500, after: (ctx) => ls(ctx, 'roam') },
      { command: 'org.roam.dailies.goto.today', wait: 1500, after: (ctx) => ls(ctx, 'roam/daily') },
      { command: 'org.roam.dailies.capture.today', answers: ['d', 'Daily'], wait: 1500 },
      { label: 'finish daily', command: 'org.capture.finish', wait: 1500, after: (ctx) => read(ctx, 'roam/daily/2026-05-18.org') },
      { command: 'org.roam.node.random', wait: 1000 },
    ],
  },
  {
    name: 'roam-link-follow',
    file: 'roam/c.org',
    content: ':PROPERTIES:\n:ID:       c-id\n:END:\n#+title: C note\n\n[[roam:A note]]\n',
    steps: [
      { at: [5, 4], command: 'org.open', wait: 1500 },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'roam/c.org'))); }, wait: 800 },
    ],
  },
  {
    name: 'journal',
    file: 'notes.org',
    content: '* x\n',
    steps: [
      { command: 'org.journal.new.entry', answers: [], wait: 1500, after: (ctx) => ls(ctx, 'journal') },
      { command: 'org.journal.new.scheduled.entry', wait: 1500 },
      { label: 'typed', command: 'org.calendar.type', answers: ['+1d', 'Tomorrow thing'], wait: 1500 },
      { command: 'org.journal.previous.entry', wait: 1000 },
      { command: 'org.journal.previous.entry', wait: 1000 },
      { command: 'org.journal.search.future.scheduled', wait: 1500 },
    ],
  },
  {
    name: 'table-structure',
    file: 'tbl.org',
    content: '| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n#+TBLFM: $2=$1*2\n',
    steps: [
      { at: [2, 2], command: 'org.meta.right', wait: 600 },
      { at: [2, 2], command: 'org.shiftmeta.right', wait: 600 },
      { at: [2, 2], command: 'org.shiftmeta.left', wait: 600 },
      { at: [2, 2], command: 'org.meta.down', wait: 600 },
      { at: [2, 2], command: 'org.shiftmeta.down', wait: 600 },
      { at: [2, 2], command: 'org.table.hline', wait: 600 },
      { at: [4, 2], command: 'org.table.blank.field', wait: 600 },
      { at: [4, 2], command: 'org.table.copy.down', wait: 2500 },
      { at: [4, 6], command: 'org.shift.up', wait: 2500 },
    ],
  },
  {
    name: 'table-shrink',
    file: 'shrink.org',
    content: '| <5>        |\n| abcdefghij |\n',
    steps: [
      { at: [1, 3], command: 'org.table.toggle.column.width', wait: 800, after: (ctx) => ctx.api.concealed?.(ctx.vscode.window.activeTextEditor) },
      { at: [1, 3], command: 'org.table.toggle.column.width', wait: 800, after: (ctx) => ctx.api.concealed?.(ctx.vscode.window.activeTextEditor) },
    ],
  },
  {
    name: 'property-all-cycle-delete',
    file: 'pall.org',
    content: '* E\n:PROPERTIES:\n:Status_ALL: a b c\n:Status: a\n:END:\n* F\n:PROPERTIES:\n:Status: b\n:END:\n',
    steps: [
      { at: [3, 3], command: 'org.shift.right', wait: 600 },
      { at: [3, 3], command: 'org.shift.left', wait: 600 },
      { at: [3, 3], command: 'org.checkbox.toggle', answers: ['s', 'c'], wait: 800 },
      { at: [0, 0], command: 'org.property.delete', answers: ['Status'], wait: 800 },
      { at: [0, 0], command: 'org.property.delete.globally', answers: ['Status'], wait: 800 },
    ],
  },
  {
    name: 'notes-logging',
    file: 'log.org',
    content: '* TODO Task\n:PROPERTIES:\n:LOG_INTO_DRAWER: t\n:END:\n',
    steps: [
      { at: [0, 3], command: 'org.add.note', wait: 1200 },
      { label: 'type note', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(e.selection.active, 'My note')); return path.basename(e.document.uri.fsPath); } },
      { label: 'finish', command: 'org.capture.finish', wait: 1500 },
      { label: 'yesterday', at: [0, 3], command: 'org.todo.yesterday', answers: ['d', { choice: 'DONE' }], wait: 1200 },
    ],
  },
  {
    name: 'countdown',
    file: 'cd.org',
    content: '* Brew\n:PROPERTIES:\n:Effort:   0:01\n:END:\n* Other\n',
    steps: [
      { at: [0, 2], command: 'org.timer.set', answers: ['1'], wait: 1500, after: (ctx) => ctx.api.timer?.() },
      { command: 'org.timer.stop', wait: 600 },
      { at: [4, 2], command: 'org.timer.set', answers: ['0:00:02'], wait: 4500, after: (ctx) => ctx.api.timer?.() },
    ],
  },
  {
    name: 'export-subtree-text-stack-unsaved',
    file: 'ex.org',
    content: '* One\nfirst\n* Two\nsecond\n',
    steps: [
      { label: 'unsaved edit', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(new ctx.vscode.Position(3, 0), 'UNSAVED ')); } },
      { at: [3, 0], command: 'org.export.dispatch', answers: ['C-s', 't', 'U'], wait: 5000 },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'ex.org'))); } },
      { command: 'org.export.dispatch', answers: ['&'], wait: 3000 },
    ],
  },
  {
    name: 'babel-inline-call-nav',
    file: 'bab.org',
    content: '#+name: double\n#+begin_src emacs-lisp :var x=2\n(* 2 x)\n#+end_src\n\nsrc_emacs-lisp{(* 6 7)}\n\n#+call: double(x=5)\n',
    steps: [
      { at: [5, 3], command: 'org.babel.execute', answers: ['Yes'], wait: 4000 },
      { at: [7, 2], command: 'org.babel.execute', answers: ['Yes'], wait: 4000 },
      { at: [0, 0], command: 'org.babel.next', wait: 1500 },
      { at: [7, 0], command: 'org.babel.goto', answers: ['double', { choice: 'double' }], wait: 2000 },
      { at: [2, 1], command: 'org.edit.special', wait: 2500 },
      { label: 'edit', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(new ctx.vscode.Position(0, 0), ';; c\n')); await e.document.save(); return path.basename(e.document.uri.fsPath); }, wait: 2500, after: (ctx) => read(ctx, 'bab.org') },
    ],
  },
  {
    name: 'return-follows-link',
    file: 'ret.org',
    content: '* A\n[[*B][to b]] tail\n* B\n',
    steps: [
      { at: [1, 3], command: 'org.return', wait: 800 },
      { at: [1, 17], command: 'org.return', wait: 800 },
    ],
  },
  {
    name: 'dblock-columnview',
    file: 'dbl.org',
    content: '* A\n:PROPERTIES:\n:Effort:   1:00\n:END:\n** B\n:PROPERTIES:\n:Effort:   0:30\n:END:\n\n',
    steps: [
      { at: [8, 0], command: 'org.dblock.insert', answers: ['columnview', { choice: 'columnview' }, 'file', ''], wait: 3000 },
      { at: [0, 2], command: 'org.columns', wait: 2500, after: (ctx) => ({ header: ctx.api.columns?.(), shown: ctx.api.columnsShown?.(ctx.vscode.window.activeTextEditor) }) },
      { command: 'org-lsp.columns.right', wait: 600, after: (ctx) => ctx.api.columns?.() },
      { command: 'org.columns.edit', answers: ['2:00'], wait: 2500 },
      { command: 'org.columns.quit', wait: 800 },
    ],
  },
  {
    name: 'settings-json-schema',
    steps: [
      {
        label: 'diagnostics',
        run: async (ctx) => {
          const file = path.join(ctx.ws, '.vscode', 'settings.json');
          // A settings.json VS Code validates against the configuration schema.
          const uri = ctx.vscode.Uri.file(file);
          const old = JSON.parse(fs.readFileSync(file, 'utf8'));
          const text = JSON.stringify({ ...old, 'org-lsp.settings': { ...old['org-lsp.settings'], emacs_options: { 'org-attach-method': 'zz', 'org-export-with-toc': 'x', 'no-such-option': 1, 'org-agenda-span': 'week' }, org_log_done: 'time' } }, null, 2);
          fs.writeFileSync(file, text);
          const doc = await ctx.vscode.workspace.openTextDocument(uri);
          await ctx.vscode.window.showTextDocument(doc);
          await ctx.sleep(4000);
          const diags = ctx.vscode.languages.getDiagnostics(uri).map((d) => [d.range.start.line, d.severity, d.message]);
          // Completion of option names inside emacs_options.
          const line = doc.getText().split('\n').findIndex((l) => l.includes('org-attach-method'));
          const list = await ctx.vscode.commands.executeCommand('vscode.executeCompletionItemProvider', uri, new ctx.vscode.Position(line, 6));
          const items = (list?.items ?? []).map((i) => (typeof i.label === 'string' ? i.label : i.label.label));
          return { lang: doc.languageId, diags, completions: items.length, sample: items.filter((i) => i.includes('org-export-with')).slice(0, 5) };
        },
      },
    ],
  },
];
