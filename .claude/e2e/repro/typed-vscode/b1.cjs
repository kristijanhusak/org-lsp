// Batch 1: setup, highlighting (tests/visual.org, screenshots), folding,
// headings and structure. Keys typed (Emacs preset).
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..', '..', '..', '..', '..', '..', '..');

const visual = fs.readFileSync('/home/user/org-lsp/.claude/worktrees/agent-ae42e9344686defef/tests/visual.org', 'utf8');
const text = (ctx) => ctx.editor().document.getText();
const scroll = (line) => async (ctx) => {
  const e = ctx.editor();
  e.revealRange(new ctx.vscode.Range(line, 0, line, 0), ctx.vscode.TextEditorRevealType.AtTop);
  await ctx.sleep(500);
};

module.exports = [
  {
    name: 'palette',
    file: 'p.org',
    content: '* A\n',
    steps: [
      { label: 'F1 Org:', run: async (ctx) => { await ctx.xdotool('key', 'F1'); await ctx.sleep(400); await ctx.typeText('Org: '); await ctx.sleep(800); }, shot: 'palette' },
      { keys: 'escape' },
    ],
  },
  {
    name: 'visual',
    file: 'visual.org',
    content: visual,
    steps: [
      { at: [0, 0], wait: 1500, shot: 'visual-0' },
      { run: scroll(40), shot: 'visual-40' },
      { run: scroll(80), shot: 'visual-80' },
      { run: scroll(120), shot: 'visual-120' },
      { run: scroll(160), shot: 'visual-160' },
      { at: 'Org\'s site', run: scroll(38), shot: 'visual-cursor-link' },
    ],
  },
  {
    name: 'cycle',
    file: 'fold.org',
    content: '* A\n** A1\ntext a1\n** A2\n* B\n** B1\n',
    steps: [
      { at: [0, 0], keys: 'tab', shot: 'fold-1', after: (ctx) => ctx.api.concealed(ctx.editor()) },
      { keys: 'tab', shot: 'fold-2' },
      { keys: 'tab', shot: 'fold-3' },
      { keys: 'shift+tab', shot: 'global-1' },
      { keys: 'shift+tab', shot: 'global-2' },
      { keys: 'shift+tab', shot: 'global-3' },
    ],
  },
  {
    name: 'startup-overview',
    file: 'ov.org',
    content: '#+STARTUP: overview\n* A\n** A1\ntext\n* B\n:PROPERTIES:\n:X: 1\n:END:\nb\n',
    steps: [{ wait: 1500, shot: 'startup-overview' }],
  },
  {
    name: 'startup-content',
    file: 'ct.org',
    content: '#+STARTUP: content\n* A\n** A1\ntext\n* B\n:PROPERTIES:\n:X: 1\n:END:\nb\n* C\n:VISIBILITY: children\n** C1\n*** C11\n',
    steps: [{ wait: 1500, shot: 'startup-content' }],
  },
  {
    name: 'two-blank-lines',
    file: 'bl.org',
    content: '* A\ntext\n\n\n* B\n',
    steps: [{ at: [0, 0], keys: 'tab', shot: 'two-blank-lines' }],
  },
  {
    name: 'tab-new-heading',
    file: 'th.org',
    content: '* A\n** \n- a\n- \n',
    steps: [
      { at: [1, 3], keys: 'tab', after: text },
      { at: [1, 2], keys: 'end', label: 'end' },
      { keys: 'tab', after: text },
      { at: [3, 2], keys: 'tab', after: text },
    ],
  },
  {
    name: 'narrow-widen',
    file: 'nw.org',
    content: '* A\na\n* B\nb\n',
    steps: [
      { at: [2, 0], keys: 'ctrl+x n s', wait: 1000 },
      { type: 'more ', after: (ctx) => text(ctx) },
      { keys: 'ctrl+x n w', wait: 1000, after: (ctx) => ctx.read('nw.org') },
    ],
  },
  {
    name: 'sparse-tree',
    file: 'sp.org',
    content: '* TODO A\na\n* B\nb\n** TODO B1\n* C\n',
    steps: [
      { at: [0, 0], keys: 'ctrl+c slash', answers: ['t'], shot: 'sparse', after: (ctx) => ctx.api.sparse(ctx.editor()) },
      { at: [3, 1], type: 'x', after: (ctx) => ctx.api.sparse(ctx.editor()) },
    ],
  },
  {
    name: 'reveal',
    file: 'rv.org',
    content: '* A\n** A1\n*** A11\ndeep\n* B\n** B1\n',
    steps: [
      { at: [0, 0], keys: 'shift+tab', label: 'overview' },
      { keys: 'shift+tab' },
      { keys: 'shift+tab' },
      { keys: 'shift+tab', label: 'overview again', shot: 'reveal-overview' },
      { run: (ctx) => { const e = ctx.editor(); e.selection = new ctx.vscode.Selection(3, 1, 3, 1); }, keys: 'ctrl+c ctrl+r', shot: 'reveal' },
    ],
  },
  {
    name: 'headings',
    file: 'h.org',
    content: '* A :tag:\ntext\n* B\n',
    steps: [
      { at: [0, 3], keys: 'alt+enter', after: text },
      { type: 'New' },
      { keys: 'alt+shift+enter', after: text },
      { type: 'Todo' },
      { at: [0, 1], keys: 'ctrl+enter', after: text },
      { at: [0, 1], keys: 'ctrl+shift+enter', after: text },
      { at: [0, 1], keys: 'alt+right', after: text },
      { at: [0, 1], keys: 'alt+left', after: text },
      { at: [0, 1], keys: 'alt+down', after: text },
      { at: 'B', keys: 'alt+up', after: text },
    ],
  },
  {
    name: 'subtree-promote',
    file: 'sub.org',
    content: '* A\n** A1\n*** A11\n* B\n',
    steps: [
      { at: [1, 1], keys: 'alt+shift+right', after: text },
      { at: [1, 1], keys: 'alt+shift+left', after: text },
    ],
  },
  {
    name: 'inline-task',
    file: 'it.org',
    content: '* A\ntext\n',
    steps: [
      { at: [1, 4], keys: 'ctrl+c ctrl+x t', after: text },
      { keys: 'alt+right', after: text },
    ],
  },
  {
    name: 'cut-paste-subtree',
    file: 'cp.org',
    content: '* A\n** A1\n* B\n** B1\n',
    steps: [
      { at: [1, 0], keys: 'ctrl+c ctrl+x ctrl+w', after: text },
      { at: [2, 0], keys: 'end' },
      { keys: 'enter' },
      { keys: 'ctrl+c ctrl+x ctrl+y', after: text },
      { at: [0, 0], keys: 'ctrl+c ctrl+x alt+w' },
      { run: async (ctx) => { const e = ctx.editor(); const end = e.document.lineAt(e.document.lineCount - 1).range.end; e.selection = new ctx.vscode.Selection(end, end); } },
      { keys: 'ctrl+c ctrl+x ctrl+y', after: text },
    ],
  },
  {
    name: 'promote-top',
    settings: { org_allow_promoting_top_level_subtree: true },
    file: 'pt.org',
    content: '* A\n',
    steps: [{ at: [0, 0], keys: 'alt+left', after: text }],
  },
  {
    name: 'sort',
    file: 'so.org',
    content: '* P\n** TODO b\n** DONE a\n** c\n',
    steps: [
      { at: [0, 0], keys: 'ctrl+c ^', answers: ['a'], after: text },
      { at: [0, 0], keys: 'ctrl+c ^', answers: ['o'], after: text },
    ],
  },
  {
    name: 'motion',
    file: 'mo.org',
    content: '* A\n** A1\n** A2\n* B\n',
    steps: [
      { at: [0, 0], keys: 'ctrl+c ctrl+n', after: (ctx) => ctx.editor().selection.active.line },
      { keys: 'ctrl+c ctrl+f', after: (ctx) => ctx.editor().selection.active.line },
      { keys: 'ctrl+c ctrl+f', after: (ctx) => ctx.editor().selection.active.line },
      { keys: 'ctrl+c ctrl+b', after: (ctx) => ctx.editor().selection.active.line },
      { keys: 'ctrl+c ctrl+u', after: (ctx) => ctx.editor().selection.active.line },
      { keys: 'ctrl+c ctrl+u' },
      { keys: 'ctrl+c ctrl+p', after: (ctx) => ctx.editor().selection.active.line },
      { at: [3, 0], keys: 'ctrl+c ctrl+j', answers: [{ choice: 'A/A2' }], after: (ctx) => ctx.editor().selection.active.line },
    ],
  },
  {
    name: 'special-ctrl-a-e-k',
    settings: { org_special_ctrl_a_e: true, org_special_ctrl_k: true },
    file: 'sk.org',
    content: '* TODO Title :tag:\n',
    steps: [
      { at: [0, 12], keys: 'ctrl+a', after: (ctx) => ctx.editor().selection.active.character },
      { keys: 'ctrl+a', after: (ctx) => ctx.editor().selection.active.character },
      { keys: 'ctrl+e', after: (ctx) => ctx.editor().selection.active.character },
      { keys: 'ctrl+e', after: (ctx) => ctx.editor().selection.active.character },
      { at: [0, 9], keys: 'ctrl+k', after: text },
      { keys: 'ctrl+k', after: text },
    ],
  },
  {
    name: 'toggle-heading-comment-clone',
    file: 'tg.org',
    content: 'a line\n* B\n* TODO C\nSCHEDULED: <2026-05-18 Mon>\n',
    steps: [
      { at: [0, 0], palette: 'Org: Turn the line into a heading, or the heading into a line', wait: 2000, after: text },
      { at: [1, 0], keys: 'ctrl+c ;', wait: 2000, after: text },
      { at: [2, 0], keys: 'ctrl+c ctrl+x c', answers: ['2', '+1w'], wait: 3000, after: text },
    ],
  },
  {
    name: 'selection-demote',
    file: 'sd.org',
    content: '* A\n* B\nx\ny\n',
    steps: [
      { select: [0, 0, 1, 3], keys: 'alt+right', after: text },
      { select: [2, 0, 3, 1], palette: 'Org: Turn the line into a heading, or the heading into a line', wait: 2000, after: text },
    ],
  },
  {
    name: 'outline',
    file: 'ol.org',
    content: '* A\n** A1\n* B\n',
    steps: [{ keys: 'ctrl+shift+o', wait: 800, shot: 'outline-symbols' }, { keys: 'escape' }],
  },
];
