// Batch 8: crypt before save, a slow block while typing, C-c ' elsewhere.
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
    name: 'crypt-before-save',
    file: 'cs.org',
    content: '* Secret :crypt:\nplain words\n',
    steps: [
      { label: 'edit+save', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(new ctx.vscode.Position(1, 0), 'more ')); await e.document.save(); }, wait: 8000, after: (ctx) => ({ disk: read(ctx, 'cs.org'), editor: ctx.vscode.window.activeTextEditor.document.getText() }) },
    ],
  },
  {
    name: 'slow-block-typing',
    file: 'slow.org',
    content: '#+begin_src emacs-lisp\n(progn (sleep-for 3) 1)\n#+end_src\n\n',
    steps: [
      { label: 'run and type', run: async (ctx) => {
        const e = ctx.vscode.window.activeTextEditor;
        e.selection = new ctx.vscode.Selection(1, 1, 1, 1);
        const p = ctx.vscode.commands.executeCommand('org.babel.execute');
        await ctx.sleep(1200);
        await e.edit((b) => b.insert(new ctx.vscode.Position(3, 0), 'typed meanwhile\n'));
        await p;
      }, answers: ['Yes'], wait: 4000 },
    ],
  },
  {
    name: 'edit-special-others',
    files: { 'inc.txt': 'included text\n' },
    file: 'es.org',
    content: '* A\nRef[fn:lab] here.\n#+begin_export html\n<b>x</b>\n#+end_export\n#+INCLUDE: "inc.txt"\n\n[fn:lab] The definition.\n',
    steps: [
      { at: [1, 5], command: 'org.edit.special', wait: 2500 },
      { label: 'edit fn', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; const t = e.document.getText(); await e.edit((b) => b.insert(new ctx.vscode.Position(0, e.document.lineAt(0).text.length), ' More')); await e.document.save(); return [path.basename(e.document.uri.fsPath), t]; }, wait: 2500, after: (ctx) => read(ctx, 'es.org') },
      back('es.org'),
      { at: [3, 2], command: 'org.edit.special', wait: 2500 },
      back('es.org'),
      { at: [5, 3], command: 'org.edit.special', wait: 2500 },
    ],
  },
];
