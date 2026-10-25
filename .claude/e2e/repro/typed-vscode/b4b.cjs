// Batch 4, again: what b4 left out or did wrong.
const text = (ctx) => ctx.editor().document.getText();
const where = (ctx) => [ctx.vscode.window.activeTextEditor?.document.uri.fsPath?.replace(ctx.ws + '/', ''), ctx.editor()?.selection.active.line];
const visible = (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => e.document.uri.fsPath.replace(ctx.ws + '/', ''));
const doc = (name) => (ctx) => {
  const open = ctx.vscode.workspace.textDocuments.find((d) => d.uri.fsPath === ctx.path.join(ctx.ws, name));
  return open ? `(open) ${open.getText()}` : ctx.read(name);
};
const focus = (name) => async (ctx) => {
  await ctx.vscode.window.showTextDocument(ctx.vscode.Uri.file(ctx.path.join(ctx.ws, name)), { viewColumn: ctx.vscode.ViewColumn.One });
};

module.exports = [
  {
    name: 'roam-toggle',
    file: 'roam/a.org',
    steps: [
      { at: [5, 0], keys: 'ctrl+c n l', wait: 1500, after: visible },
      { keys: 'ctrl+c n l', wait: 1500, after: visible },
    ],
  },
  {
    name: 'roam-alias-remove',
    file: 'roam/b.org',
    steps: [{ at: [6, 0], palette: 'Org: Roam: remove an alias', answers: ['Bee'], after: text }],
  },
  {
    name: 'roam-dedicated-unlinked',
    file: 'roam/a.org',
    steps: [
      { palette: 'Org: Roam: show the org-roam buffer of a node', answers: ['Beta note'], wait: 1500, after: (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => e.document.getText()).filter((t) => t.includes('Unlinked')) },
      { palette: 'Org: Roam: refresh the org-roam buffer', wait: 1500, after: (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => e.document.getText()).filter((t) => t.includes('Unlinked')) },
    ],
  },
  {
    name: 'roam-complete',
    file: 'roam/c.org',
    steps: [
      { at: [1, 0], type: '[[roam:', wait: 1500, shot: 'roam-complete' },
      { keys: 'enter', wait: 800, after: text },
    ],
  },
  {
    name: 'roam-dailies-marks',
    file: 'roam/a.org',
    files: { 'roam/daily/2026-10-20.org': ':PROPERTIES:\n:ID: d1\n:END:\n#+title: 2026-10-20\n' },
    steps: [{ keys: 'ctrl+c n d c', wait: 1500, shot: 'dailies-marks' }, { keys: 'q' }],
  },
  {
    name: 'journal-search',
    file: 'agenda/a.org',
    steps: [
      { keys: 'ctrl+c j s', answers: ['standup'], wait: 1000 },
      { keys: 'i', answers: ['2025-10-01'], wait: 1000 },
      { keys: 'i', answers: ['2025-12-31'], wait: 1500, after: (ctx) => [where(ctx), text(ctx)] },
    ],
  },
  {
    name: 'journal-todo-list',
    file: 'agenda/a.org',
    steps: [{ keys: 'ctrl+c j j', wait: 1500 }, { run: focus('agenda/a.org') }, { keys: 'ctrl+c a', answers: ['t'], wait: 1500, after: text }, { keys: 'q' }],
  },
  {
    name: 'journal-calendar-jn-follow',
    settings: { org_journal_follow_mode: true },
    file: 'agenda/a.org',
    steps: [
      { palette: 'Org: Journal: new entry for a date', wait: 1200, shot: 'journal-marks', after: (ctx) => [where(ctx), visible(ctx)] },
      { keys: 'bracketleft', wait: 2500, shot: 'follow-after', after: (ctx) => [where(ctx), visible(ctx)] },
      { keys: 'bracketright', wait: 2500, after: (ctx) => [where(ctx), visible(ctx)] },
      { keys: 'j n', wait: 1200, after: (ctx) => [where(ctx), text(ctx)] },
    ],
  },
  {
    name: 'journal-reschedule',
    file: 'agenda/a.org',
    steps: [
      { palette: 'Org: Journal: new entry for a day to come', wait: 1000 },
      { keys: 'i', answers: ['+2d'] },
      { type: 'move me' },
      { palette: 'Org: Journal: move the entry to come to another day', wait: 1000 },
      { keys: 'i', answers: ['+4d'], wait: 1000, after: (ctx) => [where(ctx), text(ctx)] },
    ],
  },
  {
    name: 'refile-log-note',
    settings: { org_log_refile: 'note' },
    file: 'r2.org',
    content: '* Elsewhere\n* Move me too\n',
    steps: [
      { at: [1, 2], keys: 'ctrl+c ctrl+w', answers: [{ choice: 'Projects' }], wait: 1000, after: where },
      { type: 'why' },
      { keys: 'ctrl+c ctrl+c', wait: 800, after: doc('r.org') },
    ],
  },
  {
    name: 'goto-calendar-journal-keys',
    file: 'agenda/a.org',
    content: '* TODO x <2025-10-24 Fri>\n',
    steps: [
      { at: [0, 12], keys: 'ctrl+c shift+period', wait: 1000 },
      { keys: 'j d', wait: 1000, after: (ctx) => [where(ctx), visible(ctx)] },
      { keys: 'q' },
    ],
  },
];
