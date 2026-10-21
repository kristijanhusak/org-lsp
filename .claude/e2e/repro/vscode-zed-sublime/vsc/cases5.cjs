// Batch 5: TODO dependencies and repeaters, priorities, lists, links,
// footnotes, table formulas, babel extras, citations, indentation.
const fs = require('node:fs');
const path = require('node:path');

const read = (ctx, name) => {
  try {
    return fs.readFileSync(path.join(ctx.ws, name), 'utf8');
  } catch (e) {
    return `<${e.code}>`;
  }
};
const diags = (name) => async (ctx) => ctx.vscode.languages.getDiagnostics(ctx.vscode.Uri.file(path.join(ctx.ws, name))).map((d) => [d.range.start.line, d.severity, d.message]);
const hover = (line, ch) => async (ctx) => {
  const e = ctx.vscode.window.activeTextEditor;
  const found = await ctx.vscode.commands.executeCommand('vscode.executeHoverProvider', e.document.uri, new ctx.vscode.Position(line, ch));
  return (found ?? []).map((h) => h.contents.map((c) => c.value ?? String(c)).join('\n').slice(0, 300));
};
const format = async (ctx) => {
  const e = ctx.vscode.window.activeTextEditor;
  const edits = await ctx.vscode.commands.executeCommand('vscode.executeFormatDocumentProvider', e.document.uri, { tabSize: 2, insertSpaces: true });
  const w = new ctx.vscode.WorkspaceEdit();
  w.set(e.document.uri, edits ?? []);
  await ctx.vscode.workspace.applyEdit(w);
  return (edits ?? []).length;
};

module.exports = [
  {
    name: 'todo-dependencies-repeat',
    file: 'dep.org',
    content: '* TODO Parent\n** TODO Child\n* TODO Boxes\n- [ ] one\n* TODO Weekly\nSCHEDULED: <2026-01-01 Thu +1w>\n* Cookie [/]\n** TODO a\n** TODO b\n',
    steps: [
      { at: [0, 3], command: 'org.todo', answers: ['d', { choice: 'DONE' }], wait: 1000 },
      { at: [2, 3], command: 'org.todo', answers: ['d', { choice: 'DONE' }], wait: 1000 },
      { at: [4, 3], command: 'org.todo', answers: ['d', { choice: 'DONE' }], wait: 1000 },
      { at: [7, 4], command: 'org.todo.next', wait: 1000 },
    ],
  },
  {
    name: 'priorities-numbers-and-tags-groups',
    file: 'pr.org',
    content: '#+PRIORITIES: 1 5 3\n#+TAGS: { @office(o) @home(m) } work(w)\n* TODO x\n',
    steps: [
      { at: [2, 3], command: 'org.priority.up', wait: 600 },
      { at: [2, 3], command: 'org.tags.set', answers: ['o', 'w', 'm', '\r'], wait: 1200 },
    ],
  },
  {
    name: 'lists',
    file: 'lists.org',
    content: '* L [/]\n- [ ] a\n- [ ] b\n- [X] c\n\n1. one\n3. two\n2. three\n\n- zeta\n- alpha\n',
    steps: [
      { at: [1, 3], command: 'org.checkbox.toggle', wait: 800 },
      { at: [5, 1], command: 'org.checkbox.toggle', wait: 800 },
      { at: [9, 2], command: 'org.sort', answers: ['a', { choice: 'alpha' }], wait: 1000 },
      { at: [1, 2], command: 'org.meta.return', wait: 800 },
      { at: [6, 1], command: 'org.shift.right', wait: 800 },
    ],
  },
  {
    name: 'links-follow-targets',
    files: { 'other.org': '* Heading there\ntext\n' },
    file: 'lk.org',
    content: '* A\n:PROPERTIES:\n:CUSTOM_ID: cid\n:END:\n<<tgt>> target\n* B\n[[#cid]] [[tgt]] [[file:other.org::*Heading there]] [[*A]]\n',
    steps: [
      { at: [6, 3], command: 'org.open', wait: 800 },
      { at: [6, 12], command: 'org.open', wait: 800 },
      { at: [6, 20], command: 'org.open', wait: 1200 },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'lk.org'))); } },
      { label: 'hover link', run: hover(6, 3) },
      {
        label: 'references+symbols',
        run: async (ctx) => {
          const uri = ctx.vscode.Uri.file(path.join(ctx.ws, 'lk.org'));
          const refs = await ctx.vscode.commands.executeCommand('vscode.executeReferenceProvider', uri, new ctx.vscode.Position(0, 3));
          const syms = await ctx.vscode.commands.executeCommand('vscode.executeWorkspaceSymbolProvider', 'Heading');
          const def = await ctx.vscode.commands.executeCommand('vscode.executeDefinitionProvider', uri, new ctx.vscode.Position(6, 3));
          return { refs: (refs ?? []).map((r) => [path.basename(r.uri.fsPath), r.range.start.line]), syms: (syms ?? []).map((s) => [s.name, path.basename(s.location.uri.fsPath)]), def: (def ?? []).map((d) => [path.basename((d.uri ?? d.targetUri).fsPath), (d.range ?? d.targetRange).start.line]) };
        },
      },
      {
        label: 'rename heading',
        run: async (ctx) => {
          const uri = ctx.vscode.Uri.file(path.join(ctx.ws, 'lk.org'));
          const edit = await ctx.vscode.commands.executeCommand('vscode.executeDocumentRenameProvider', uri, new ctx.vscode.Position(0, 2), 'Alpha');
          await ctx.vscode.workspace.applyEdit(edit);
          return edit.entries().map(([u, es]) => [path.basename(u.fsPath), es.map((x) => x.newText)]);
        },
      },
    ],
  },
  {
    name: 'footnotes-follow-diag',
    file: 'fns.org',
    content: '* A\nText[fn:1] and[fn:2].\n\n[fn:1] Def one.\n',
    steps: [
      { at: [1, 6], command: 'org.open', wait: 800 },
      { command: 'org.open', wait: 800 },
      { label: 'diags', run: diags('fns.org'), wait: 1500 },
      { at: [1, 16], command: 'org.footnote.action', answers: ['y', 'yes', 'Yes'], wait: 2500 },
    ],
  },
  {
    name: 'table-formulas',
    file: 'tf.org',
    content: '| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n|   |   |\n#+TBLFM: @>$2=vsum(@2..@-1);%.2f\n\n| <2026-05-18 Mon> |   |\n#+TBLFM: $2=$1+7\n\n#+name: rem\n| 9 |\n\n| 1 | 2 |   |   |\n#+TBLFM: $3=\'(+ $1 $2);N::$4=remote(rem,@1$1)\n',
    steps: [
      { at: [0, 2], command: 'org.table.recalculate', wait: 1000 },
      { at: [7, 3], command: 'org.table.recalculate', wait: 1000 },
      { at: [13, 3], command: 'org.table.recalculate', wait: 3000 },
      { label: 'format', run: format, wait: 800 },
      { at: [14, 3], command: 'org.checkbox.toggle', wait: 2000 },
    ],
  },
  {
    name: 'table-edit-formulas-field',
    file: 'tef.org',
    content: '| 1 | 2 |   |\n#+TBLFM: $3=$1+$2\n',
    steps: [
      { at: [0, 2], command: 'org.edit.special', wait: 2500 },
      { label: 'edit', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; const t = e.document.getText(); const i = t.indexOf('$1+$2'); if (i < 0) return t; await e.edit((b) => b.replace(new ctx.vscode.Range(e.document.positionAt(i), e.document.positionAt(i + 5)), '$1*$2')); await e.document.save(); return t; }, wait: 2500, after: (ctx) => read(ctx, 'tef.org') },
    ],
  },
  {
    name: 'babel-lob-detangle',
    files: { 'lib.org': '#+name: sq\n#+begin_src emacs-lisp :var x=3\n(* x x)\n#+end_src\n' },
    file: 'lob.org',
    content: '#+call: sq(x=4)\n\n#+begin_src emacs-lisp :tangle out.el :comments link\n(message "a")\n#+end_src\n',
    steps: [
      { label: 'ingest', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'lib.org'))); }, wait: 800 },
      { command: 'org.babel.lob.ingest', wait: 2500 },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'lob.org'))); }, wait: 800 },
      { at: [0, 2], command: 'org.babel.execute', answers: ['Yes'], wait: 4000 },
      { at: [3, 0], command: 'org.babel.tangle', wait: 2500, after: (ctx) => read(ctx, 'out.el') },
      { label: 'change tangled', run: (ctx) => { const p = path.join(ctx.ws, 'out.el'); fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace('(message "a")', '(message "b")')); } },
      { label: 'open out.el', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'out.el'))); }, wait: 800 },
      { command: 'org.babel.detangle', wait: 3000, after: (ctx) => read(ctx, 'lob.org') },
    ],
  },
  {
    name: 'citations-hover-unknown',
    files: { 'refs.bib': '@book{knuth84,\n  title = {The TeXbook},\n  author = {Knuth, Donald},\n  year = {1984}\n}\n' },
    file: 'ct.org',
    content: '#+bibliography: refs.bib\n* A\nSee [cite:@knuth84] and [cite:@knuth85].\n\n',
    steps: [
      { label: 'hover', run: hover(2, 14), wait: 1500 },
      { label: 'diags', run: diags('ct.org') },
      { at: [4, 0], command: 'org.cite.insert', answers: ['knuth84', { choice: '(done)' }], wait: 1500 },
      { at: [2, 14], command: 'org.open', wait: 1500 },
    ],
  },
  {
    name: 'indent-hidestars-adapt',
    file: 'ind.org',
    content: '#+STARTUP: indent hidestars\n* A\ntext\n** B\nmore\n',
    steps: [
      { label: 'virtual indent', run: async (ctx) => ctx.api.virtualIndent?.(ctx.vscode.window.activeTextEditor), wait: 800 },
      { label: 'concealed', run: async (ctx) => ctx.api.concealed?.(ctx.vscode.window.activeTextEditor) },
    ],
  },
  {
    name: 'attachment-link',
    files: { 'pic.txt': 'attached\n' },
    file: 'atl.org',
    content: '* Entry\n',
    steps: [
      { at: [0, 2], command: 'org.attach', answers: (ctx) => ['c', path.join(ctx.ws, 'pic.txt')], wait: 3000 },
      { label: 'add link', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; const n = e.document.lineCount; await e.edit((b) => b.insert(new ctx.vscode.Position(n, 0), '[[attachment:pic.txt]]\n')); return e.document.getText(); }, wait: 600 },
      { label: 'follow', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; const n = e.document.lineCount - 2; e.selection = new ctx.vscode.Selection(n, 4, n, 4); await ctx.vscode.commands.executeCommand('org.open'); }, wait: 2000 },
    ],
  },
];
