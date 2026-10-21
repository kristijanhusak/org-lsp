// Batch 1: emacs_options, export, attach, dates, timer, tables, babel, links.
const fs = require('node:fs');
const path = require('node:path');

const read = (ctx, name) => {
  try {
    return fs.readFileSync(path.join(ctx.ws, name), 'utf8');
  } catch (e) {
    return `<${e.code}>`;
  }
};
const ls = (ctx, dir) => {
  const out = [];
  const walk = (d) => {
    for (const n of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, n.name);
      if (n.isDirectory()) walk(p);
      else out.push(path.relative(ctx.ws, p));
    }
  };
  try {
    walk(path.join(ctx.ws, dir));
  } catch {}
  return out;
};

module.exports = [
  {
    name: 'attach-mv',
    files: { 'src.txt': 'hello\n' },
    file: 'att.org',
    content: '* Entry\nText.\n',
    steps: [
      { at: [0, 3], command: 'org.attach', answers: (ctx) => ['a', path.join(ctx.ws, 'src.txt')], wait: 3000, after: (ctx) => ({ src: fs.existsSync(path.join(ctx.ws, 'src.txt')), data: ls(ctx, 'data') }) },
    ],
  },
  {
    name: 'export-html-no-toc',
    file: 'exp.org',
    content: '#+TITLE: Exp\n* One\nText one.\n* Two\nText two.\n',
    steps: [
      { at: [1, 0], command: 'org.export.dispatch', answers: ['h', 'h'], wait: 6000, after: (ctx) => { const h = read(ctx, 'exp.html'); return { exists: !h.startsWith('<'), toc: h.includes('table-of-contents'), len: h.length }; } },
    ],
  },
  {
    name: 'export-menu-hash-template',
    file: 'exp2.org',
    content: '* One\n',
    steps: [{ at: [0, 0], command: 'org.export.dispatch', answers: ['#', 'default'], wait: 4000 }],
  },
  {
    name: 'schedule-deadline-remove',
    file: 'dates.org',
    content: '* TODO Task\n',
    steps: [
      { at: [0, 3], command: 'org.schedule', answers: ['+2d'] },
      { at: [0, 3], command: 'org.deadline', answers: ['fri 10:00'] },
      { label: 'remove schedule', at: [0, 3], command: 'org.schedule.remove' },
      { label: 'remove deadline', at: [0, 3], command: 'org.deadline.remove' },
    ],
  },
  {
    name: 'timestamp-range',
    file: 'ts.org',
    content: '* A\nwhen \n',
    steps: [
      { at: [1, 5], command: 'org.timestamp.insert', answers: ['2026-05-20'] },
      { command: 'org.timestamp.insert', answers: ['2026-05-22'] },
    ],
  },
  {
    name: 'timer',
    file: 'timer.org',
    content: '* Timer\n\n',
    steps: [
      { at: [1, 0], command: 'org.timer.start', wait: 1500, after: (ctx) => ctx.api.timer?.() },
      { command: 'org.timer.insert', wait: 500 },
      { command: 'org.timer.pause', wait: 500, after: (ctx) => ctx.api.timer?.() },
      { command: 'org.timer.pause', wait: 500 },
      { command: 'org.timer.stop', wait: 500, after: (ctx) => ctx.api.timer?.() },
    ],
  },
  {
    name: 'table-formula',
    file: 'tbl.org',
    content: '| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n| 3 | 4 |   |\n#+TBLFM: $3=$1*$2\n',
    steps: [
      { at: [2, 2], command: 'org.table.recalculate' },
      { at: [2, 2], command: 'org.table.sum', wait: 2500 },
      { at: [2, 2], command: 'org.table.field.info', wait: 2500 },
    ],
  },
  {
    name: 'table-create',
    file: 'tblc.org',
    content: 'x\n\n',
    steps: [{ at: [1, 0], command: 'org.table.create', answers: ['3x2'], wait: 3000 }],
  },
  {
    name: 'babel-elisp',
    file: 'babel.org',
    content: '#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\nsrc_emacs-lisp{(* 6 7)}\n',
    steps: [
      { at: [1, 1], command: 'org.babel.execute', answers: ['y', 'yes'], wait: 5000 },
      { at: [6, 3], command: 'org.babel.execute', answers: ['y', 'yes'], wait: 4000 },
    ],
  },
  {
    name: 'tangle',
    file: 'tangle.org',
    content: '#+begin_src sh :tangle out/t.sh :mkdirp yes :shebang "#!/bin/sh"\necho hi\n#+end_src\n',
    steps: [{ at: [0, 0], command: 'org.babel.tangle', wait: 2000, after: (ctx) => read(ctx, 'out/t.sh') }],
  },
  {
    name: 'footnote-new',
    file: 'fn.org',
    content: '* A\nSome text\n',
    steps: [{ at: [1, 9], command: 'org.footnote.action', wait: 4000 }],
  },
  {
    name: 'link-store-insert',
    file: 'links.org',
    content: '* Target heading\n* Other\nhere \n',
    steps: [
      { at: [0, 3], command: 'org.link.store', wait: 500 },
      { at: [2, 5], command: 'org.link.insert', answers: ['', 'Desc'], wait: 800 },
    ],
  },
  {
    name: 'link-follow-missing-heading',
    file: 'links2.org',
    content: '* A\n[[No such heading]]\n',
    steps: [{ at: [1, 4], command: 'org.open', answers: ['y', 'yes'], wait: 1500 }],
  },
  {
    name: 'link-shell',
    file: 'links3.org',
    content: '* A\n[[shell:echo hi]]\n',
    steps: [
      { at: [1, 4], command: 'org.open', answers: ['n', 'no'], wait: 1500 },
      { at: [1, 4], command: 'org.open', answers: ['y', 'yes'], wait: 2500 },
    ],
  },
  {
    name: 'todo-keyword-sets',
    file: 'kw.org',
    content: '#+TODO: TODO | DONE\n#+TODO: REPORT BUG | FIXED\n* TODO x\n',
    steps: [
      { at: [2, 3], command: 'org.todo.nextset', wait: 600 },
      { at: [2, 3], command: 'org.todo.previousset', wait: 600 },
    ],
  },
  {
    name: 'priority-up-down',
    file: 'pri.org',
    content: '* TODO x\n',
    steps: [
      { at: [0, 3], command: 'org.priority.up' },
      { at: [0, 3], command: 'org.priority.down' },
    ],
  },
  {
    name: 'property-effort-ordered',
    file: 'prop.org',
    content: '* Entry\n',
    steps: [
      { at: [0, 3], command: 'org.property.set', answers: ['Owner', 'me'] },
      { at: [0, 3], command: 'org.effort.set', answers: ['1:00'] },
      { at: [0, 3], command: 'org.ordered.toggle' },
    ],
  },
  {
    name: 'narrow-widen',
    file: 'narrow.org',
    content: '* A\na text\n* B\nb text\n',
    steps: [
      { at: [2, 1], command: 'org.narrow.subtree', wait: 1500 },
      { label: 'edit', run: async (ctx) => { const e = ctx.vscode.window.activeTextEditor; await e.edit((b) => b.insert(new ctx.vscode.Position(1, 0), 'new line\n')); } },
      { command: 'org.narrow.widen', wait: 1500, after: (ctx) => read(ctx, 'narrow.org') },
    ],
  },
  {
    name: 'num-mode',
    file: 'num.org',
    content: '#+STARTUP: num\n* A\n** B\n* C\n',
    steps: [
      { label: 'shown', run: async (ctx) => ctx.api.standIns(ctx.vscode.window.activeTextEditor), wait: 300 },
      { command: 'org.num.mode', wait: 800, after: (ctx) => ctx.api.standIns(ctx.vscode.window.activeTextEditor) },
    ],
  },
  {
    name: 'clock-report-display',
    file: 'clk.org',
    content: '* A\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 08:00]--[2026-05-18 Mon 09:30] =>  1:30\n:END:\n\n',
    steps: [
      { at: [0, 2], command: 'org.clock.display', wait: 800 },
      { at: [4, 0], command: 'org.clock.report', wait: 1500 },
    ],
  },
  {
    name: 'sparse-todo-removal',
    file: 'sparse.org',
    content: '* TODO a\n* b\n* TODO c\n',
    steps: [
      { at: [1, 0], command: 'org.sparse.tree', answers: ['t'], wait: 800 },
      { at: [1, 3], command: 'org.checkbox.toggle', wait: 800 },
    ],
  },
  {
    name: 'lint-severities',
    file: 'lint.org',
    content: '* A\ntext ** heading\n[[file:nowhere.org]]\n#+begin_src python :foo 1\nx\n#+end_src\n#+begin_quote\nopen\n',
    steps: [{ label: 'diags', wait: 2500, run: async (ctx) => ctx.vscode.languages.getDiagnostics(ctx.vscode.Uri.file(path.join(ctx.ws, 'lint.org'))).map((d) => [d.range.start.line, d.severity, d.message]) }],
  },
];
