// While VS Code shows the server's question (a toast with buttons) that
// the user has not answered, do other keys still work?
const text = (ctx) => ctx.editor().document.getText();
module.exports = [
  {
    name: 'pending-question',
    file: 'fn.org',
    content: '* A\nText[fn:1] and [fn:2].\n* Footnotes\n[fn:1] The first.\n| a | bbb |\n| 1 | 2 |\n',
    steps: [
      { at: '[fn:2]', column: 2, run: async (ctx) => { await ctx.xdotool('key', 'ctrl+c', 'ctrl+x', 'f'); await ctx.sleep(2000); ctx.shot('question'); } , timeout: 3000 },
      { at: [0, 2], run: async (ctx) => { ctx.editor().selection = new ctx.vscode.Selection(0, 2, 0, 2); await ctx.xdotool('key', 'ctrl+c', 'ctrl+t'); await ctx.sleep(3000); return text(ctx); }, timeout: 3000 },
      { at: [5, 2], run: async (ctx) => { ctx.editor().selection = new ctx.vscode.Selection(5, 2, 5, 2); await ctx.xdotool('key', 'alt+Right'); await ctx.sleep(3000); return [text(ctx), ctx.api.commands()]; }, timeout: 3000 },
    ],
  },
];
