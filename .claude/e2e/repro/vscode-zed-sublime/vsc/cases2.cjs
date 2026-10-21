// Batch 2: date prompt via the calendar, babel yes/no, display, structure,
// tags, refile/archive, footnotes, links, clocking.
const fs = require('node:fs');
const path = require('node:path');

const read = (ctx, name) => {
  try {
    return fs.readFileSync(path.join(ctx.ws, name), 'utf8');
  } catch (e) {
    return `<${e.code}>`;
  }
};
const hints = async (ctx) => {
  const e = ctx.vscode.window.activeTextEditor;
  const found = await ctx.vscode.commands.executeCommand('vscode.executeInlayHintProvider', e.document.uri, new ctx.vscode.Range(0, 0, e.document.lineCount, 0));
  return (found ?? []).map((h) => [h.position.line, h.position.character, typeof h.label === 'string' ? h.label : h.label.map((l) => l.value).join('')]);
};
// The calendar opens first; `i` (org.calendar.type) asks for the answer.
const dated = (command, answer, at) => [
  { at, command, wait: 800 },
  { label: `${command} typed`, command: 'org.calendar.type', answers: [answer], wait: 800 },
];

module.exports = [
  {
    name: 'babel-elisp-yes',
    file: 'babel.org',
    content: '#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\nsrc_emacs-lisp{(* 6 7)}\n',
    steps: [
      { at: [1, 1], command: 'org.babel.execute', answers: ['Yes', 'yes', 'y'], wait: 5000 },
      { at: [4, 3], command: 'org.babel.execute', answers: ['Yes', 'yes', 'y'], wait: 4000 },
      { at: [1, 1], command: 'org.babel.remove_result', wait: 1500 },
    ],
  },
  {
    name: 'schedule-deadline-remove',
    file: 'dates.org',
    content: '* TODO Task\n',
    steps: [
      ...dated('org.schedule', '+2d', [0, 3]),
      ...dated('org.deadline', 'fri 10:00', [0, 3]),
      { label: 'delay', at: [0, 3], command: 'org.schedule.delay', wait: 800 },
      { label: 'delay typed', command: 'org.calendar.type', answers: ['-3d'], wait: 800 },
      { label: 'remove schedule', at: [0, 3], command: 'org.schedule.remove' },
      { label: 'remove deadline', at: [0, 3], command: 'org.deadline.remove' },
    ],
  },
  {
    name: 'timestamp-range',
    file: 'ts.org',
    content: '* A\nwhen \n',
    steps: [...dated('org.timestamp.insert', '2026-05-20', [1, 5]), ...dated('org.timestamp.insert', '2026-05-22')],
  },
  {
    name: 'num-and-clock-display',
    file: 'num.org',
    content: '#+STARTUP: num\n* A\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 08:00]--[2026-05-18 Mon 09:30] =>  1:30\n:END:\n** B\n* C\n',
    steps: [
      { label: 'hints', run: hints, wait: 300 },
      { command: 'org.clock.display', wait: 800, after: hints },
      { command: 'org.num.mode', wait: 800, after: hints },
    ],
  },
  {
    name: 'lint-misplaced',
    file: 'lint.org',
    content: '* A\ntext** heading\n',
    steps: [{ label: 'diags', wait: 2500, run: async (ctx) => ctx.vscode.languages.getDiagnostics(ctx.vscode.Uri.file(path.join(ctx.ws, 'lint.org'))).map((d) => [d.range.start.line, d.severity, d.message]) }],
  },
  {
    name: 'inlinetask',
    file: 'inl.org',
    content: '* A\ntext\n',
    steps: [
      { at: [1, 4], command: 'org.inlinetask.insert', answers: ['Inline'], wait: 800 },
      { command: 'org.meta.right', wait: 800 },
    ],
  },
  {
    name: 'goto-outline-path',
    file: 'goto.org',
    content: '* A\n** B\n*** C\n* D\n',
    steps: [
      { at: [0, 0], command: 'org.goto', answers: ['A/B/'], wait: 800 },
      { at: [2, 0], command: 'org.outline.path', wait: 800 },
      { at: [0, 0], command: 'org.heading.up', wait: 800 },
    ],
  },
  {
    name: 'tags-typed-and-rename',
    file: 'tags.org',
    content: '#+FILETAGS: :old:\n* A :old:\n* B\n',
    steps: [
      { at: [2, 2], command: 'org.tags.set', answers: [':zz:old:'], wait: 800 },
      {
        label: 'rename tag',
        run: async (ctx) => {
          const uri = ctx.vscode.window.activeTextEditor.document.uri;
          const edit = await ctx.vscode.commands.executeCommand('vscode.executeDocumentRenameProvider', uri, new ctx.vscode.Position(1, 6), 'new');
          await ctx.vscode.workspace.applyEdit(edit);
          return edit.size;
        },
      },
    ],
  },
  {
    name: 'refile-copy-goto-log',
    file: 'ref.org',
    content: '* Inbox\n** Item\n* Projects\n',
    steps: [
      { at: [1, 3], command: 'org.refile.copy', answers: ['Projects', { choice: 'Projects' }], wait: 1000 },
      { at: [0, 0], command: 'org.refile.goto', answers: ['Projects', { choice: 'Projects' }], wait: 800 },
      { at: [0, 0], command: 'org.refile.goto.last', wait: 800 },
    ],
  },
  {
    name: 'archive-sibling-tag',
    file: 'arch.org',
    content: '* P\n** DONE x\n** TODO y\n',
    steps: [
      { at: [1, 3], command: 'org.archive.sibling', wait: 1000 },
      { at: [1, 3], command: 'org.archive.tag', wait: 800 },
    ],
  },
  {
    name: 'footnote-menu',
    file: 'fnm.org',
    content: '* A\nOne[fn:b] two[fn:a].\n\n[fn:a] A.\n[fn:b] B.\n',
    steps: [{ at: [1, 0], command: 'org.footnote.menu', answers: ['r', { choice: 'renumber' }], wait: 2500 }],
  },
  {
    name: 'link-next-wrap-id',
    file: 'lnk.org',
    content: '* A\n[[https://a.example]] and [[https://b.example]]\n',
    steps: [
      { at: [0, 0], command: 'org.link.next', wait: 400 },
      { command: 'org.link.next', wait: 400 },
      { command: 'org.link.next', wait: 400 },
      { command: 'org.link.next', wait: 400 },
      { at: [0, 2], command: 'org.id.create', wait: 800 },
    ],
  },
  {
    name: 'clock-effort-goto',
    files: { 'other.org': '* Other\n' },
    file: 'clock.org',
    content: '* TODO Task\n:PROPERTIES:\n:Effort:   0:01\n:END:\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 09:00]--[2026-05-18 Mon 10:00] =>  1:00\n:END:\n',
    steps: [
      { at: [0, 3], command: 'org.clock.in', wait: 1500, after: (ctx) => ctx.api.clock?.() },
      { label: 'open other', run: async (ctx) => { await ctx.vscode.window.showTextDocument(await ctx.vscode.workspace.openTextDocument(path.join(ctx.ws, 'other.org'))); } },
      { command: 'org.clock.goto', wait: 800 },
      { command: 'org.clock.out', wait: 800 },
      { command: 'org.clock.in.last', wait: 800 },
      { command: 'org.clock.cancel', wait: 800 },
    ],
  },
  {
    name: 'structure-emphasis-drawer',
    file: 'struct.org',
    content: '* A\nword\n\n',
    steps: [
      { at: [2, 0], command: 'org.structure.template', answers: ['s', { choice: 'src' }], wait: 1500 },
      { at: [1, 0, 1, 4], command: 'org.emphasize', answers: ['*', '*'], wait: 1500 },
    ],
  },
  {
    name: 'toggle-heading-item-comment',
    file: 'tog.org',
    content: '* A\nline one\n',
    steps: [
      { at: [1, 0], command: 'org.toggle.heading', wait: 1500 },
      { at: [1, 0], command: 'org.toggle.comment', wait: 1500 },
      { at: [1, 0], command: 'org.toggle.heading', wait: 1500 },
      { at: [1, 0], command: 'org.toggle.item', wait: 1500 },
    ],
  },
  {
    name: 'clone-subtree',
    file: 'clone.org',
    content: '* Meet <2026-05-18 Mon>\n',
    steps: [{ at: [0, 2], command: 'org.subtree.clone', answers: ['2', '+1w'], wait: 2500 }],
  },
  {
    name: 'citation',
    files: { 'refs.bib': '@book{knuth84,\n  title = {The TeXbook},\n  author = {Knuth, Donald},\n  year = {1984}\n}\n' },
    file: 'cite.org',
    content: '#+bibliography: refs.bib\n* A\nSee \n',
    steps: [
      { at: [2, 4], command: 'org.cite.insert', answers: ['knuth84', { choice: 'knuth84' }, ''], wait: 1500 },
      {
        label: 'complete',
        run: async (ctx) => {
          const e = ctx.vscode.window.activeTextEditor;
          await e.edit((b) => b.insert(new ctx.vscode.Position(3, 0), '[cite:@kn'));
          const list = await ctx.vscode.commands.executeCommand('vscode.executeCompletionItemProvider', e.document.uri, new ctx.vscode.Position(3, 9));
          return list.items.slice(0, 5).map((i) => (typeof i.label === 'string' ? i.label : i.label.label));
        },
      },
    ],
  },
  {
    name: 'completion-misc',
    file: 'comp.org',
    content: '* \n#+STA\n#+begin_\n\\alp\n',
    steps: [
      {
        label: 'complete',
        run: async (ctx) => {
          const e = ctx.vscode.window.activeTextEditor;
          const out = {};
          for (const [l, c] of [[0, 2], [1, 5], [2, 8], [3, 4]]) {
            const list = await ctx.vscode.commands.executeCommand('vscode.executeCompletionItemProvider', e.document.uri, new ctx.vscode.Position(l, c));
            out[`${l}:${c}`] = list.items.slice(0, 6).map((i) => (typeof i.label === 'string' ? i.label : i.label.label));
          }
          return out;
        },
      },
    ],
  },
  {
    name: 'capture-templates',
    file: 'cap.org',
    content: '* Here\n',
    steps: [
      { at: [0, 2], command: 'org.capture', answers: ['p', 'Alice'], wait: 2000 },
      { label: 'finish', command: 'org.capture.finish', wait: 1500, after: (ctx) => read(ctx, 'inbox.org') },
      { at: [0, 2], command: 'org.capture', answers: ['d'], wait: 2000 },
      { label: 'finish', command: 'org.capture.finish', wait: 1500, after: (ctx) => read(ctx, 'tree.org') },
    ],
  },
];
