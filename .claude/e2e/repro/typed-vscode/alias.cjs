// Removing the last alias of a node in VS Code: is the blank line after
// `#+title:` kept, as the server's edit keeps it?
const text = (ctx) => ctx.editor().document.getText();
module.exports = [
  {
    name: 'alias-remove',
    file: 'roam/b2.org',
    content: ':PROPERTIES:\n:ID:       b2-id\n:ROAM_ALIASES: Bee\n:END:\n#+title: Beta two\n\nSee it.\n',
    steps: [
      { at: [6, 0], label: 'before', after: text },
      { at: [6, 0], palette: 'Org: Roam: remove an alias', answers: ['Bee'], after: text },
      { label: 'later', wait: 1500, after: text },
    ],
  },
];
