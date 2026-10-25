// Batch 6: the looks the settings give (screenshots), org-protocol,
// encryption, remote images. Keys typed (Emacs preset).
const text = (ctx) => ctx.editor().document.getText();
const shotOf = (name, wait = 1800) => ({ wait, shot: name });

module.exports = [
  {
    name: 'level-colors',
    file: 'lv.org',
    content: '* One\n** Two\n*** Three\n**** Four\n***** Five\n****** Six\n******* Seven\n******** Eight\n* DONE Done one\n',
    steps: [
      shotOf('levels-default'),
      { run: async (ctx) => { await ctx.vscode.workspace.getConfiguration('org-lsp').update('headlineLevelColors', true, ctx.vscode.ConfigurationTarget.Workspace); await ctx.sleep(2500); } , shot: 'levels-colors' },
      { run: async (ctx) => { await ctx.vscode.workspace.getConfiguration('org-lsp').update('headlineLevelColors', undefined, ctx.vscode.ConfigurationTarget.Workspace); await ctx.sleep(2500); } , shot: 'levels-off' },
    ],
  },
  {
    name: 'fontify-todo-quote',
    settings: { org_fontify_todo_headline: true, org_fontify_quote_and_verse_blocks: true, org_n_level_faces: 2, org_fontify_whole_heading_line: true },
    file: 'ft.org',
    content: '* TODO A task headline\n* Plain\n** Two\n*** Three\n#+begin_quote\nQuoted line\n#+end_quote\n',
    steps: [shotOf('fontify-todo-quote', 2500)],
  },
  {
    name: 'emphasis-options',
    settings: { org_hide_emphasis_markers: true, org_emphasis_alist: { '*': ['bold', 'italic'] }, org_hidden_keywords: ['title'], org_hide_macro_markers: true, org_pretty_entities: true },
    file: 'em.org',
    content: '#+TITLE: The title\n\n*this* and /that/ and {{{x}}} and \\alpha and x^2 and H_{2}O\n\nmore\n',
    steps: [{ at: [4, 0], ...shotOf('emphasis-options', 2500), after: (ctx) => [ctx.api.concealed(ctx.editor()), ctx.api.standIns(ctx.editor())] }],
  },
  {
    name: 'custom-times-bullets',
    settings: { org_display_custom_times: true, org_bullets_mode: true },
    file: 'ct.org',
    content: '* One <2026-10-05 Mon 10:00>\n** Two\n- [X] a\n- [ ] b\n\nend\n',
    steps: [
      { at: [5, 0], ...shotOf('custom-times-bullets', 2500), after: (ctx) => [ctx.api.concealed(ctx.editor()), ctx.api.standIns(ctx.editor())] },
      { at: [5, 0], keys: 'ctrl+c ctrl+x ctrl+t', wait: 1500, shot: 'custom-times-off', after: (ctx) => ctx.api.standIns(ctx.editor()) },
    ],
  },
  {
    name: 'archive-shade-src',
    file: 'as.org',
    content: '* Kept\n* Old :ARCHIVE:\nhidden\n* Code\n#+begin_src python\ndef f(x):\n    return x + 1\n#+end_src\nInline src_python{x = 1} here.\n',
    steps: [shotOf('archive-src', 2500)],
  },
  {
    name: 'config-toml',
    file: 'cf.org',
    content: '* WAITING x\n',
    steps: [{ at: [0, 2], keys: 'ctrl+c ctrl+t', after: text }],
  },
  {
    name: 'protocol-store-link',
    file: 'pr.org',
    content: '* Here\n\n',
    steps: [
      {
        run: async (ctx) => {
          const dir = ctx.path.join(process.env.XDG_STATE_HOME ?? '', 'org-lsp', 'protocol');
          const server = ctx.vscode.workspace.getConfiguration('org-lsp').get('server.path');
          const { execFileSync } = require('node:child_process');
          try {
            return execFileSync(server, ['protocol', 'org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F&title=Org+mode'], { encoding: 'utf8' });
          } catch (error) {
            return `failed: ${error.message}`;
          }
        },
        wait: 1500,
      },
      { at: [1, 0], keys: 'ctrl+c ctrl+l', answers: ['https://orgmode.org/', 'Org mode'], after: text },
    ],
  },
];
