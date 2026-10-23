// Batch 4: the agenda's edits written back, crypt, file apps, abbreviations,
// man links, appointments, dangling clocks, export and open.
const fs = require('node:fs');
const path = require('node:path');

const read = (ctx, name) => {
  try {
    return fs.readFileSync(path.join(ctx.ws, name), 'utf8');
  } catch (e) {
    return `<${e.code}>`;
  }
};
const opened = [];
const stubExternal = (ctx) => {
  try {
    ctx.vscode.env.openExternal = async (uri) => {
      opened.push(uri.toString());
      return true;
    };
    return 'stubbed';
  } catch (e) {
    return `not stubbed: ${e.message}`;
  }
};
const agendaLine = (ctx, text) => {
  const e = ctx.vscode.window.activeTextEditor;
  const lines = e.document.getText().split('\n');
  const i = lines.findIndex((l) => l.includes(text));
  e.selection = new ctx.vscode.Selection(i, 0, i, 0);
  return i;
};
const visible = (ctx) => ctx.vscode.window.visibleTextEditors.map((e) => path.basename(e.document.uri.fsPath || e.document.uri.path));

module.exports = [
  {
    name: 'agenda-edits',
    file: 'work.org',
    content: '* TODO Write report :office:\nSCHEDULED: <2026-05-18 Mon 09:00>\n* TODO Call the client\nDEADLINE: <2026-05-20 Wed>\n* Projects\n** Alpha\n',
    steps: [
      { command: 'org.agenda.dispatch', answers: ['a'], wait: 1500 },
      { label: 'to report', run: (ctx) => agendaLine(ctx, 'Write report'), wait: 1200, after: (ctx) => ctx.api.echoed?.() },
      { command: 'org.tags.set', answers: [':office:urgent:'], wait: 1500 },
      { label: 'to report', run: (ctx) => agendaLine(ctx, 'Write report'), wait: 300 },
      { command: 'org.add.note', wait: 1500 },
      { label: 'type note', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(e.selection.active, 'From agenda')); return path.basename(e.document.uri.fsPath); } },
      { label: 'finish note', command: 'org.capture.finish', wait: 1500, after: (ctx) => read(ctx, 'work.org') },
      { label: 'to call', run: (ctx) => { const i = agendaLine(ctx, 'Call the client'); return [i, path.basename(ctx.vscode.window.activeTextEditor.document.uri.path)]; }, wait: 300 },
      { command: 'org.refile', answers: ['Projects/Alpha', { choice: 'Alpha' }], wait: 2000, after: (ctx) => read(ctx, 'work.org') },
      { label: 'to report', run: (ctx) => agendaLine(ctx, 'Write report'), wait: 300 },
      { command: 'org.archive.default', answers: ['y', 'yes'], wait: 2000, after: (ctx) => ({ work: read(ctx, 'work.org'), archive: read(ctx, 'work.org_archive') }) },
      { command: 'org.agenda.save', wait: 800 },
    ],
  },
  {
    name: 'agenda-follow-inactive-search',
    file: 'work2.org',
    content: '* TODO Alpha :work:urgent:\nSCHEDULED: <2026-05-18 Mon>\n[2026-05-19 Tue] noted\n* TODO Beta :work:\nSome word here\n',
    steps: [
      { command: 'org.agenda.dispatch', answers: ['a'], wait: 1500 },
      { command: 'org.agenda.inactive', wait: 1500 },
      { command: 'org.agenda.redo', wait: 1500 },
      { command: 'org.agenda.dispatch', answers: ['m', 'work+urgent'], wait: 1500 },
      { command: 'org.agenda.dispatch', answers: ['s', 'word'], wait: 1500 },
      { command: 'org.agenda.follow', wait: 1000 },
      { label: 'move', run: (ctx) => agendaLine(ctx, 'Beta'), wait: 1500, after: visible },
      { command: 'org.agenda.follow', wait: 800 },
    ],
  },
  {
    name: 'crypt',
    file: 'crypt.org',
    content: '* Secret :crypt:\nhidden text\n',
    steps: [
      { at: [0, 2], command: 'org.crypt.encrypt', answers: ['y', 'yes', 'Yes'], wait: 8000 },
      { at: [0, 2], command: 'org.crypt.decrypt', answers: ['y', 'yes', 'Yes'], wait: 8000 },
    ],
  },
  {
    name: 'file-apps-and-abbrev',
    files: { 'notes.txt': 'hello\n' },
    file: 'apps.org',
    content: '#+LINK: gh https://github.com/%s\n* A\n[[file:notes.txt]]\n[[gh:org-lsp]]\n[[man:ls::-l]]\n<2026-05-19 Tue>\n',
    steps: [
      { label: 'stub', run: stubExternal },
      { at: [2, 4], command: 'org.open', wait: 2500, after: (ctx) => ({ copy: read(ctx, 'copy.txt'), opened }) },
      { at: [3, 4], command: 'org.open', wait: 1500, after: () => opened.slice() },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'apps.org'))); } },
      { at: [4, 4], command: 'org.open', wait: 3000, after: () => opened.slice() },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'apps.org'))); } },
      { at: [5, 3], command: 'org.open', wait: 2000 },
    ],
  },
  {
    name: 'export-open',
    file: 'exo.org',
    content: '* A\ntext\n',
    steps: [
      { label: 'stub', run: stubExternal },
      { at: [0, 0], command: 'org.export.dispatch', answers: ['h', 'o'], wait: 6000, after: () => opened.slice() },
      { label: 'back', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'exo.org'))); } },
      { at: [0, 0], command: 'org.export.dispatch', answers: ['&'], wait: 3000 },
    ],
  },
  {
    name: 'dangling-clock',
    file: 'dang.org',
    content: '* TODO Left running\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 09:00]\n:END:\n* TODO Other\n',
    steps: [
      { at: [4, 3], command: 'org.clock.in', answers: ['K', 'k', ''], wait: 2500 },
      { at: [0, 3], command: 'org.clock.resolve', answers: ['K', ''], wait: 2500 },
    ],
  },
  {
    name: 'appointments',
    file: 'appt.org',
    content: '* Meeting <2026-05-18 Mon 10:40>\n',
    steps: [{ command: 'org.agenda.appt', wait: 6000 }],
  },
];
