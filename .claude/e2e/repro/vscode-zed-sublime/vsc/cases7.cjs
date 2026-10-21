// Batch 7: config.toml reaching the server, image width from #+ATTR_ORG,
// remote images, LaTeX without latex, messages in the status bar.
const fs = require('node:fs');
const path = require('node:path');

const images = (ctx) => ctx.api.images?.(ctx.vscode.window.activeTextEditor);

module.exports = [
  {
    name: 'config-toml',
    file: 'cfg.org',
    content: '* TODO x\n',
    steps: [
      { at: [0, 3], command: 'org.todo.next', wait: 800 },
      { at: [0, 3], command: 'org.todo.next', wait: 800 },
    ],
  },
  {
    name: 'image-attr-width',
    files: {},
    file: 'img.org',
    content: '* Pics\n#+ATTR_ORG: :width 100\n[[file:pic.png]]\n\n[[https://raw.githubusercontent.com/kristijanhusak/org-lsp/main/editors/vscode/icon.png]]\n',
    steps: [
      { label: 'copy png', run: (ctx) => { fs.copyFileSync(path.join(ctx.ws, '..', 'pic.png'), path.join(ctx.ws, 'pic.png')); } },
      { at: [2, 3], command: 'org.link.preview', wait: 1500, after: images },
      { at: [4, 5], command: 'org.link.preview', answers: ['y'], wait: 6000, after: images },
      {
        label: 'hover',
        run: async (ctx) => {
          const e = ctx.vscode.window.activeTextEditor;
          const found = await ctx.vscode.commands.executeCommand('vscode.executeHoverProvider', e.document.uri, new ctx.vscode.Position(2, 5));
          return (found ?? []).map((h) => h.contents.map((c) => (c.value ?? String(c)).slice(0, 200)).join('|'));
        },
      },
    ],
  },
  {
    name: 'latex-missing',
    file: 'tex.org',
    content: '* M\n$e^{i\\pi} = -1$\n',
    steps: [{ at: [1, 3], command: 'org.latex.preview', wait: 8000, after: images }],
  },
];
