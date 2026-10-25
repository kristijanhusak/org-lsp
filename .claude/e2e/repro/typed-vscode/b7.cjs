// Batch 7: hovers through the Command Palette (Ctrl+K Ctrl+I is Emacs's
// C-k in an Org file with the Emacs keys), the image toggled off again.
const text = (ctx) => ctx.editor().document.getText();
const hover = (name) => ({ palette: 'Show or Focus Hover', wait: 1500, shot: name });

module.exports = [
  {
    name: 'hover-link',
    file: 'lh.org',
    content: '* Target\nbody\n* Linker\nSee [[*Target]] here.\n',
    steps: [{ wait: 1500 }, { at: '[[*Target', column: 4, ...hover('hover-link') }, { keys: 'escape' }],
  },
  {
    name: 'hover-cite',
    file: 'c.org',
    files: { 'refs.bib': '@book{knuth84,\n  author = {Donald Knuth},\n  title = {The TeXbook},\n  year = 1984\n}\n' },
    content: '#+bibliography: refs.bib\n* Cite\nSee [cite:@knuth84].\n',
    steps: [{ wait: 1500 }, { at: '@knuth84', column: 3, ...hover('hover-cite') }, { keys: 'escape' }, { at: '@knuth84', column: 3, keys: 'ctrl+c ctrl+o', wait: 1500, after: (ctx) => [ctx.editor().document.uri.fsPath.split('/').pop(), ctx.editor().selection.active.line] }],
  },
  {
    name: 'image-toggle',
    file: 'img.org',
    content: '* Pics\n[[file:pic.png]]\n',
    steps: [
      { run: async (ctx) => { ctx.fs.copyFileSync('/home/user/org-lsp/.claude/worktrees/agent-ae42e9344686defef/editors/vscode/icon.png', ctx.path.join(ctx.ws, 'pic.png')); } },
      { at: [1, 3], keys: 'ctrl+c ctrl+x ctrl+v', wait: 1500, shot: 'image-on', after: (ctx) => ctx.api.images(ctx.editor()) },
      { at: [1, 3], ...hover('image-hover') },
      { keys: 'escape' },
      { at: [1, 3], keys: 'ctrl+c ctrl+x ctrl+v', wait: 1500, after: (ctx) => [ctx.api.images(ctx.editor()), text(ctx)] },
    ],
  },
  {
    name: 'agenda-files-hover',
    file: 'h.org',
    content: '* TODO Something\nSCHEDULED: <2026-10-25 Sun>\nbody text\n',
    steps: [
      { keys: 'ctrl+c a', answers: ['a'], wait: 1500 },
      { at: 'Something', keys: 'ctrl+k ctrl+i', wait: 1500, shot: 'agenda-hover2' },
      { keys: 'escape' },
      { keys: 'q' },
    ],
  },
];
