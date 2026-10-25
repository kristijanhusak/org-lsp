// Batch 5, again: what b5 did wrong.
const text = (ctx) => ctx.editor().document.getText();
const where = (ctx) => [ctx.vscode.window.activeTextEditor?.document.uri.toString().replace('file://' + ctx.ws + '/', ''), ctx.editor()?.selection.active.line, ctx.editor()?.selection.active.character];

module.exports = [
  {
    name: 'rename-headline-tag',
    file: 'rn.org',
    files: { 'other.org': '* Elsewhere\nSee [[file:rn.org::*Target][it]].\n' },
    content: '#+FILETAGS: :old:\n* Target :old:\nbody\n* Linker\nSee [[*Target]] here.\n',
    steps: [
      { wait: 2000 },
      { at: [1, 4], keys: 'f2', wait: 1000 },
      { nofocus: true, type: 'Renamed', wait: 300 },
      { nofocus: true, keys: 'enter', wait: 2000, after: (ctx) => [text(ctx), ctx.vscode.workspace.textDocuments.filter((d) => d.uri.fsPath.endsWith('other.org')).map((d) => d.getText())] },
      { at: [1, 12], keys: 'f2', wait: 1000 },
      { nofocus: true, type: 'new', wait: 300 },
      { nofocus: true, keys: 'enter', wait: 2000, after: text },
    ],
  },
  {
    name: 'footnote-create',
    file: 'fn.org',
    content: '* A\nText[fn:1] and [fn:2].\n* Footnotes\n[fn:1] The first.\n',
    steps: [
      { at: '[fn:2]', column: 2, keys: 'ctrl+c ctrl+x f', answers: ['Yes'], wait: 2500, after: (ctx) => [where(ctx), text(ctx)] },
      { palette: 'Org: Sort, renumber, normalize or delete footnotes', answers: ['n'], wait: 2000, after: text },
    ],
  },
  {
    name: 'tables',
    file: 't.org',
    content: '| a | bbbbbbbbbb |\n|---+------|\n| 1 | 2 |\n| 3 | 4 |\n',
    steps: [
      { at: [0, 2], palette: 'Format Document', after: text },
      { at: [2, 2], keys: 'alt+right', after: text },
      { at: [2, 15], keys: 'alt+left', after: text },
      { at: [2, 2], keys: 'alt+down', after: text },
      { at: [3, 2], keys: 'alt+up', after: text },
      { at: [2, 2], keys: 'alt+shift+right', after: text },
      { at: [2, 2], keys: 'alt+shift+left', after: text },
      { at: [2, 2], keys: 'alt+shift+down', after: text },
      { at: [2, 2], keys: 'alt+shift+up', after: text },
      { at: [2, 2], keys: 'ctrl+c minus', after: text },
      { at: [2, 2], keys: 'ctrl+c enter', after: (ctx) => [text(ctx), where(ctx)] },
      { at: [2, 2], palette: 'Org: Blank the table field', after: text },
      { at: [4, 2], keys: 'shift+enter', wait: 2500, after: text },
      { at: [4, 2], keys: 'ctrl+c shift+=', wait: 2500 },
      { at: [4, 2], keys: 'ctrl+c shift+/', wait: 2500 },
      { at: [2, 2], keys: 'ctrl+c shift+6', answers: ['n'], after: text },
      { at: [2, 2], keys: 'shift+up', wait: 2500, after: text },
      { at: [6, 0], keys: 'shift+enter', wait: 2500, after: text },
    ],
  },
  {
    name: 'table-create',
    file: 'tcr.org',
    content: 'text\n\n\n',
    steps: [
      { at: [2, 0], keys: 'ctrl+c shift+\\', answers: ['3x2'], wait: 4000, after: text },
      { at: [0, 0], keys: 'end' },
      { keys: 'enter' },
      { keys: 'ctrl+c shift+\\', answers: [''], wait: 4000, after: text },
    ],
  },
  {
    name: 'formulas',
    file: 'f.org',
    content: '| 2 | 3 |   |\n| 4 | 5 |   |\n#+TBLFM: $3=$1*$2\n\n| 1 |\n| 2 |\n|   |\n#+TBLFM: @>$1=vsum(@1..@-1);%.2f\n\n| <2026-05-18 Mon> |   |\n#+TBLFM: $2=$1+7\n\n| 1 | 2 |   |\n#+TBLFM: $3=\'(+ $1 $2);N\n\n#+NAME: other\n| 9 |\n\n| x |\n#+TBLFM: $1=remote(other,@1$1)\n',
    steps: [
      { at: '#+TBLFM: @>', keys: 'ctrl+c ctrl+c', wait: 1500, after: text },
      { at: '#+TBLFM: $2=$1+7', keys: 'ctrl+c ctrl+c', wait: 3000, after: text },
      { at: "#+TBLFM: $3='", keys: 'ctrl+c ctrl+c', wait: 5000, after: text },
      { at: '#+TBLFM: $1=remote', keys: 'ctrl+c ctrl+c', wait: 5000, after: text },
      { at: [1, 2], keys: 'ctrl+c shift+8', after: text },
    ],
  },
];
