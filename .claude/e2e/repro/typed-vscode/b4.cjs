// Batch 4: org-roam, org-journal, refile and archive, capture to a date
// tree, a dangling clock. Keys typed (Emacs preset), the real clock.
const text = (ctx) => ctx.editor().document.getText();
const where = (ctx) => [ctx.vscode.window.activeTextEditor?.document.uri.fsPath?.replace(ctx.ws + '/', ''), ctx.editor()?.selection.active.line];
const visible = (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => e.document.uri.fsPath.replace(ctx.ws + '/', ''));
const today = (() => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; })();

module.exports = [
  {
    name: 'roam-find-insert',
    file: 'roam/c.org',
    steps: [
      { keys: 'ctrl+c n f', answers: ['Bee'], after: where },
      { run: async (ctx) => { await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, 'roam/c.org'))); } },
      { at: [3, 0], keys: 'ctrl+c n i', answers: ['Alpha note'], after: text },
      { select: [0, 8, 0, 13], keys: 'ctrl+c n i', answers: ['Gamma'], after: text },
      { at: [3, 0], keys: 'ctrl+c n i', answers: ['Delta new'], wait: 1000, after: (ctx) => [where(ctx), text(ctx)] },
      { keys: 'ctrl+c ctrl+c', wait: 1000, after: (ctx) => [where(ctx), text(ctx), ctx.fs.readdirSync(ctx.path.join(ctx.ws, 'roam'))] },
    ],
  },
  {
    name: 'roam-random-alias-tags',
    file: 'roam/a.org',
    steps: [
      { palette: 'Org: Roam: visit a random node', after: where },
      { run: async (ctx) => { await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, 'roam/a.org'))); } },
      { at: [5, 0], palette: 'Org: Roam: add an alias', answers: ['Ay'], after: text },
      { at: [5, 0], palette: 'Org: Roam: remove an alias', answers: [{ choice: 'Ay' }], after: text },
      { at: [5, 0], palette: 'Org: Roam: add tags', answers: ['topic'], after: text },
    ],
  },
  {
    name: 'roam-link-follow',
    file: 'roam/c.org',
    steps: [{ at: [2, 10], keys: 'ctrl+c ctrl+o', after: (ctx) => [where(ctx), ctx.read('roam/c.org')] }],
  },
  {
    name: 'roam-buffer',
    file: 'roam/a.org',
    steps: [
      { at: [5, 0], keys: 'ctrl+c n l', wait: 1500, shot: 'roam-buffer', after: (ctx) => [where(ctx), visible(ctx), ctx.vscode.window.visibleTextEditors.map((e) => e.document.getText()).filter((t) => t.includes('Backlinks'))] },
      { run: async (ctx) => { await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, 'roam/b.org')), { viewColumn: ctx.vscode.ViewColumn.One }); } },
      { at: [10, 0], wait: 1500, after: (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => e.document.getText()).filter((t) => t.includes('Backlinks') || t.includes('References')) },
      { run: async (ctx) => { await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, 'roam/a.org')), { viewColumn: ctx.vscode.ViewColumn.One }); } },
      { at: [5, 0], wait: 1500, after: (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => e.document.getText()).filter((t) => t.includes('Backlinks')) },
      {
        run: async (ctx) => {
          const e = ctx.vscode.window.visibleTextEditors.find((x) => x.document.getText().includes('Backlinks'));
          await ctx.vscode.window.showTextDocument(e.document, { viewColumn: e.viewColumn });
          const lines = e.document.getText().split('\n');
          const l = lines.findIndex((t) => t.includes('Beta note') && !t.startsWith('#'));
          ctx.vscode.window.activeTextEditor.selection = new ctx.vscode.Selection(l, 4, l, 4);
          return lines;
        },
      },
      { keys: 'ctrl+c ctrl+o', after: where },
      { run: async (ctx) => { await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, 'roam/a.org')), { viewColumn: ctx.vscode.ViewColumn.One }); } },
      { keys: 'ctrl+c n l', wait: 1000, after: visible },
      { palette: 'Org: Roam: show the org-roam buffer of a node', answers: ['Beta note'], wait: 1500, after: (ctx) => [where(ctx), visible(ctx)] },
    ],
  },
  {
    name: 'roam-dailies',
    file: 'roam/a.org',
    steps: [
      { keys: 'ctrl+c n d d', wait: 1000, after: (ctx) => [where(ctx), text(ctx)] },
      { keys: 'ctrl+c n d n', answers: [], wait: 1000, after: where },
      { type: 'daily entry' },
      { keys: 'ctrl+c ctrl+c', wait: 800, after: (ctx) => ctx.read(`roam/daily/${today}.org`) },
      { keys: 'ctrl+c n d c', wait: 1000, shot: 'dailies-calendar', after: (ctx) => text(ctx).split('\n').slice(-3) },
      { keys: 'shift+left' },
      { keys: 'enter', wait: 1000, after: where },
      { keys: 'ctrl+c n d f', after: where },
      { keys: 'ctrl+c n d b', after: where },
      { keys: 'ctrl+c n d period', answers: [`${today}.org`], after: where },
    ],
  },
  {
    name: 'journal',
    file: 'agenda/a.org',
    steps: [
      { keys: 'ctrl+c j j', wait: 1500, shot: 'journal-new', after: (ctx) => [where(ctx), text(ctx), ctx.read('journal/20251024'), ctx.vscode.window.activeTextEditor.document.languageId] },
      { keys: 'ctrl+c j b', after: where },
      { keys: 'ctrl+c j b', after: (ctx) => where(ctx) },
      { palette: 'Org: Journal: new entry for a date', wait: 1000, shot: 'journal-calendar', after: (ctx) => text(ctx) },
      { keys: 'i', answers: ['+3d'], after: (ctx) => [where(ctx), text(ctx)] },
      { palette: 'Org: Journal: new entry for a day to come', wait: 1000 },
      { keys: 'i', answers: ['+1d'], after: (ctx) => [where(ctx), text(ctx)] },
      { type: 'future task' },
      { keys: 'ctrl+x ctrl+s' },
      { keys: 'ctrl+c j s', answers: ['standup', '2025-10-01', '+5d'], wait: 1500, after: (ctx) => [where(ctx), text(ctx)] },
      { palette: 'Org: Journal: the TODO entries to come', wait: 1500, after: text },
      { palette: 'Org: Journal: the entries to come with a date', wait: 1500, after: text },
    ],
  },
  {
    name: 'journal-agenda',
    file: 'agenda/a.org',
    steps: [{ keys: 'ctrl+c a', answers: ['a'], wait: 1500, after: text }, { keys: 'q' }],
  },
  {
    name: 'journal-calendar-keys',
    file: 'agenda/a.org',
    steps: [
      { palette: 'Org: Journal: new entry for a date', wait: 1000 },
      { keys: 'bracketleft', after: (ctx) => text(ctx).split('\n').slice(-3) },
      { keys: 'j r', wait: 1000, after: (ctx) => [where(ctx), visible(ctx)] },
      { palette: 'Org: Journal: new entry for a date', wait: 1000 },
      { keys: 'bracketleft' },
      { keys: 'j d', wait: 1000, after: (ctx) => [where(ctx), visible(ctx)] },
      { keys: 'j s w', answers: ['standup'], wait: 1500, after: (ctx) => [where(ctx), text(ctx)] },
    ],
  },
  {
    name: 'refile-archive',
    file: 'r.org',
    steps: [
      { at: [1, 3], keys: 'ctrl+c alt+w', answers: [{ choice: 'Projects' }], after: text },
      { palette: 'Org: Go to a refile target', answers: [{ choice: 'Alpha' }], after: where },
      { at: [1, 3], keys: 'ctrl+c ctrl+w', answers: [{ choice: 'Elsewhere' }], after: (ctx) => [text(ctx), ctx.read('r2.org')] },
      { palette: 'Org: Go to where the last refile put its entry', after: where },
      { run: async (ctx) => { await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, 'r.org'))); } },
      { at: 'Alpha', keys: 'ctrl+c ctrl+x ctrl+s', after: (ctx) => [text(ctx), ctx.read('r.org_archive')] },
      { at: 'Item one', keys: 'ctrl+c ctrl+x shift+a', after: text },
      { at: 'Inbox', keys: 'ctrl+c ctrl+x a', after: text },
      { at: 'Done stuff', palette: 'Org: Archive the subtrees without open TODO items', answers: ['y'], after: text },
    ],
  },
  {
    name: 'refile-log',
    settings: { org_log_refile: 'time' },
    file: 'r2.org',
    content: '* Elsewhere\n* Move me\n',
    steps: [{ at: [1, 2], keys: 'ctrl+c ctrl+w', answers: [{ choice: 'Projects' }], after: (ctx) => ctx.read('r.org') }],
  },
  {
    name: 'refile-log-note',
    settings: { org_log_refile: 'note' },
    file: 'r2.org',
    content: '* Elsewhere\n* Move me too\n',
    steps: [
      { at: [1, 2], keys: 'ctrl+c ctrl+w', answers: [{ choice: 'Projects' }], wait: 1000, after: where },
      { type: 'why' },
      { keys: 'ctrl+c ctrl+c', wait: 800, after: (ctx) => ctx.read('r.org') },
    ],
  },
  {
    name: 'datetree',
    file: 'r2.org',
    steps: [
      { keys: 'ctrl+c c', answers: ['d'], after: text },
      { type: 'diary entry' },
      { keys: 'ctrl+c ctrl+c', after: (ctx) => ctx.read('diary.org') },
    ],
  },
  {
    name: 'dangling-real',
    file: 'dang.org',
    content: `* TODO Left running\n:LOGBOOK:\nCLOCK: [${today} ${new Date().toDateString().slice(0, 3)} 00:01]\n:END:\n* TODO Another\n`,
    steps: [
      { at: [4, 2], keys: 'ctrl+c ctrl+x ctrl+i', answers: ['K', ''], after: (ctx) => [text(ctx), ctx.api.clock()] },
      { keys: 'ctrl+c ctrl+x ctrl+o' },
    ],
  },
];
