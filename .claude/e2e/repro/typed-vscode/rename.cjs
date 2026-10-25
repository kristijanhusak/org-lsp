// F2 on a headline and on a tag: does VS Code's rename box show?
const text = (ctx) => ctx.editor().document.getText();
module.exports = [
  {
    name: 'rename-headline',
    file: 'rn.org',
    content: '#+FILETAGS: :old:\n* Target :old:\nbody\n* Linker\nSee [[*Target]] here.\n',
    steps: [
      { wait: 2000 },
      {
        at: [1, 4],
        run: async (ctx) => {
          const prepared = await ctx.vscode.commands.executeCommand('vscode.prepareRename', ctx.editor().document.uri, new ctx.vscode.Position(1, 4)).catch((e) => String(e));
          await ctx.xdotool('key', 'F2');
          await ctx.sleep(1500);
          ctx.shot('rename-headline');
          return prepared;
        },
      },
      { keys: 'escape' },
      {
        at: [1, 11],
        run: async (ctx) => {
          const prepared = await ctx.vscode.commands.executeCommand('vscode.prepareRename', ctx.editor().document.uri, new ctx.vscode.Position(1, 11)).catch((e) => String(e));
          await ctx.xdotool('key', 'F2');
          await ctx.sleep(1500);
          ctx.shot('rename-tag');
          return prepared;
        },
      },
      { type: 'new', wait: 300 },
      { keys: 'enter', wait: 1500, after: text },
    ],
  },
];
