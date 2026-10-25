// C-c ' on a block, a line typed in the file of the block, saved: does
// the block get both lines?
const text = (ctx) => ctx.editor().document.getText();
module.exports = [
  {
    name: 'edit-special',
    file: 'b.org',
    content: '#+NAME: answer\n#+begin_src emacs-lisp\n(* 6 7)\n#+end_src\n',
    steps: [
      { at: [2, 0], keys: "ctrl+c '", wait: 3000, after: text },
      { type: ';; edited', wait: 500, after: text },
      { keys: 'enter', wait: 800, shot: 'special-typed', after: text },
      { keys: 'ctrl+s', wait: 2500, after: (ctx) => [text(ctx), ctx.vscode.workspace.textDocuments.filter((d) => d.uri.fsPath.endsWith('b.org')).map((d) => d.getText()), ctx.read('b.org')] },
    ],
  },
];
