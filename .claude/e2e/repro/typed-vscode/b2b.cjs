// Batch 2, again: what b2 did wrong or found, after fixes.
const text = (ctx) => ctx.editor().document.getText();

module.exports = [
  {
    name: 'tags-append',
    file: 'ta.org',
    content: '* A :zeta:\n* B :old:\n',
    steps: [{ at: [1, 2], keys: 'ctrl+c ctrl+q', answers: ['new'], after: text }],
  },
  {
    name: 'timestamp-append',
    file: 'tsa.org',
    content: '* A <2026-05-18 Mon 10:00>\n',
    steps: [
      { at: [0, 8], palette: 'Org: Change the timestamp at the cursor', wait: 800 },
      { keys: 'i', answers: [' fri'], after: text },
    ],
  },
  {
    name: 'todo-menu-colors',
    settings: { org_todo_keywords: ['TODO(t)', 'WAIT(w@)', '|', 'DONE(d)'] },
    file: 'tp.org',
    content: '* TODO Task\n',
    steps: [{ at: [0, 2], run: async (ctx) => { await ctx.xdotool('key', 'ctrl+c', 'ctrl+t'); await ctx.sleep(1200); ctx.shot('todo-menu'); }, answers: ['t'] }],
  },
  {
    name: 'tag-rename',
    file: 'tr.org',
    content: '#+FILETAGS: :old:\n* A :old:\n',
    steps: [
      { at: [1, 5], keys: 'f2', wait: 800 },
      { keys: ['ctrl+a'], type: 'new', wait: 300 },
      { keys: 'enter', wait: 1000, after: text },
    ],
  },
  {
    name: 'numbered-and-cookie',
    file: 'nc.org',
    content: '1. one\n2. two\n\n\n* Parent [0/2]\n** TODO a\n** TODO b\n',
    steps: [
      { at: [1, 6], keys: 'alt+enter', after: text },
      { at: [5, 3], keys: 'ctrl+c ctrl+t', after: text },
    ],
  },
  {
    name: 'columns-quit',
    file: 'co.org',
    content: '* P\n** TODO a\n:PROPERTIES:\n:Effort: 0:30\n:END:\n',
    steps: [
      { at: [0, 0], keys: 'ctrl+c ctrl+x ctrl+c', wait: 3000, after: (ctx) => ctx.api.columns() },
      { keys: 'q', wait: 800, after: (ctx) => [ctx.api.columns(), ctx.api.columnsShown(ctx.editor())] },
      { at: [0, 0], keys: 'ctrl+c ctrl+x ctrl+c', wait: 3000, after: (ctx) => ctx.api.columns() },
      { at: [1, 0], keys: 'e', answers: [{ keys: 'Escape' }], after: (ctx) => ctx.api.columns() },
      { keys: 'q', wait: 800, after: (ctx) => [ctx.api.columns(), ctx.api.columnsShown(ctx.editor())] },
    ],
  },
  {
    name: 'inactive-ts',
    file: 'it.org',
    content: '* Other\n',
    steps: [
      { at: [0, 7], keys: 'ctrl+c shift+1', wait: 600 },
      { keys: 'enter', after: text },
    ],
  },
  {
    name: 'property-delete',
    file: 'pd.org',
    content: '* A\n:PROPERTIES:\n:Color: red\n:END:\n* B\n:PROPERTIES:\n:Color: blue\n:END:\n',
    steps: [{ at: [0, 0], palette: 'Org: Delete a property of the entry', answers: [{ choice: 'COLOR' }], after: text }],
  },
];
