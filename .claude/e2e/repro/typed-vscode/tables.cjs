// Table keys alone, with a screenshot after each.
const text = (ctx) => ctx.editor().document.getText();
module.exports = [
  {
    name: 'tables-alone',
    file: 't.org',
    content: '| a | bbbbbbbbbb |\n|---+------|\n| 1 | 2 |\n| 3 | 4 |\n',
    steps: [
      { wait: 2000, shot: 't0' },
      { at: [0, 2], keys: 'ctrl+shift+i', wait: 1500, shot: 't1', after: (ctx) => [text(ctx), ctx.api.commands()] },
      { at: [2, 2], keys: 'alt+right', wait: 1500, shot: 't2', after: (ctx) => [text(ctx), ctx.api.commands()] },
      { at: [2, 2], palette: 'Org: Move the column right', wait: 1500, shot: 't3', after: (ctx) => [text(ctx), ctx.api.commands()] },
    ],
  },
];
