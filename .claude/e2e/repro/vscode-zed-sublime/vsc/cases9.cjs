// Batch 9: folding ranges (two blank lines, inline tasks, drawers), the
// outline leaving inline tasks out.
const folds = async (ctx) => (await ctx.vscode.commands.executeCommand('vscode.executeFoldingRangeProvider', ctx.vscode.window.activeTextEditor.document.uri)).map((f) => [f.start, f.end, f.kind]);
const symbols = async (ctx) => {
  const walk = (s) => s.map((x) => [x.name, x.kind, walk(x.children ?? [])]);
  return walk((await ctx.vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', ctx.vscode.window.activeTextEditor.document.uri)) ?? []);
};
module.exports = [
  {
    name: 'folding',
    file: 'fold.org',
    content: '* A\ntext\n\n\n* B\n:PROPERTIES:\n:X: 1\n:END:\nbody\n*************** TODO Inline\ninline body\n*************** END\nafter\n** B1\n#+begin_src sh\necho\n#+end_src\n',
    steps: [{ label: 'folds', run: folds, wait: 1500 }, { label: 'symbols', run: symbols }],
  },
];
