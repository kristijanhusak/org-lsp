// Write the parts of the Sublime Text package that mirror the server: the
// key bindings of both presets, the commands for the Command Palette, and
// the scopes of the server's semantic tokens. They come from the server's
// manifest, so nothing about Org is listed by hand in the package.
//
//   node editors/sublime/generate.mjs [path/to/org-lsp]   rewrite the files
//   node editors/sublime/generate.mjs --check             fail if out of date
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
const pkg = join(here, 'LSP-org');
const args = process.argv.slice(2);
const check = args.includes('--check');
const exe = process.platform === 'win32' ? 'org-lsp.exe' : 'org-lsp';
const binary =
  args.find((arg) => !arg.startsWith('--')) ??
  ['release', 'debug'].map((profile) => join(root, 'target', profile, exe)).find(existsSync);
if (!binary) {
  console.error('No server binary found. Build it with: cargo build -p org-lsp');
  process.exit(1);
}
const manifest = JSON.parse(execFileSync(resolve(binary), ['manifest'], { encoding: 'utf8' }));

/** An Emacs key (`C-c C-t`, `S-<up>`, `RET`) as Sublime's list of keys. */
function fromEmacs(emacs) {
  const names = { RET: 'enter', TAB: 'tab', SPC: 'space', DEL: 'backspace', ESC: 'escape' };
  const modifiers = { C: 'ctrl', M: 'alt', S: 'shift' };
  return emacs.split(' ').map((stroke) => {
    const parts = [];
    let rest = stroke;
    for (let match; (match = /^([CMS])-(.+)$/.exec(rest)); rest = match[2]) {
      parts.push(modifiers[match[1]]);
    }
    const named = /^<(.+)>$/.exec(rest);
    let base = names[rest] ?? (named ? named[1] : rest);
    if (base.length === 1 && base !== base.toLowerCase()) {
      parts.push('shift');
      base = base.toLowerCase();
    }
    return [...parts, base].join('+');
  });
}

/** A Vim key (`<C-Space>`, `cit`, `<S-Right>`) the same way; null for the leader. */
function fromVim(vim) {
  const named = { CR: 'enter', Tab: 'tab', Esc: 'escape', Space: 'space', BS: 'backspace' };
  const modifiers = { C: 'ctrl', M: 'alt', A: 'alt', S: 'shift' };
  const keys = [];
  for (const [, special, plain] of vim.matchAll(/<([^>]+)>|(.)/g)) {
    if (plain !== undefined) {
      keys.push(plain.length === 1 && plain !== plain.toLowerCase() ? `shift+${plain.toLowerCase()}` : plain);
      continue;
    }
    const parts = special.split('-');
    const base = parts.pop();
    if (base === 'Leader') return null;
    keys.push([...parts.map((part) => modifiers[part]), named[base] ?? base.toLowerCase()].join('+'));
  }
  return keys;
}

const selector = {
  org: 'text.org',
  capture: 'text.org',
  agenda: 'text.org-agenda',
};
// Keys for the whole editor are bound where Org's are.
const globalScopes = ['org', 'agenda'];
// What a key does where the command does not apply.
const fallbacks = {
  'shift+up': { command: 'move', args: { by: 'lines', forward: false, extend: true } },
  'shift+down': { command: 'move', args: { by: 'lines', forward: true, extend: true } },
  'shift+left': { command: 'move', args: { by: 'characters', forward: false, extend: true } },
  'shift+right': { command: 'move', args: { by: 'characters', forward: true, extend: true } },
  'ctrl+a': { command: 'nv_feed_key', args: { key: '<C-a>' } },
  'ctrl+x': { command: 'nv_feed_key', args: { key: '<C-x>' } },
  tab: { command: 'insert_best_completion', args: { default: '\t', exact: false } },
  enter: { command: 'insert', args: { characters: '\n' } },
  'shift+enter': { command: 'insert', args: { characters: '\n' } },
  // Org's structure keys: Sublime Text's moves by part of a word, and
  // its lines up and down.
  'alt+left': { command: 'move', args: { by: 'subwords', forward: false } },
  'alt+right': { command: 'move', args: { by: 'subword_ends', forward: true } },
  'alt+shift+left': { command: 'move', args: { by: 'subwords', forward: false, extend: true } },
  'alt+shift+right': { command: 'move', args: { by: 'subword_ends', forward: true, extend: true } },
  'alt+up': { command: 'swap_line_up' },
  'alt+down': { command: 'swap_line_down' },
};

/** The package's own commands, with their keys. */
const own = [
  {
    command: 'org_lsp_cycle',
    title: 'Cycle the visibility of the subtree, block or drawer at the cursor',
    keys: [
      { key: 'TAB', in: 'org', preset: 'emacs', native: true },
      { key: '<Tab>', in: 'org', preset: 'vim' },
    ],
  },
  {
    command: 'org_lsp_cycle_global',
    title: 'Cycle the visibility of the whole file',
    keys: [
      { key: 'S-TAB', in: 'org', preset: 'emacs' },
      { key: '<S-Tab>', in: 'org', preset: 'vim' },
    ],
  },
];

// Keys without a palette entry of their own. The agenda's keys are the
// same in every editor: `q` closes it and `g ?` lists its keys, as in
// Neovim and Vim.
const keysOnly = [
  {
    command: 'org_lsp_help',
    keys: [
      { key: 'g ?', in: 'agenda', preset: 'emacs' },
      { key: 'g?', in: 'agenda', preset: 'vim' },
    ],
  },
  {
    command: 'close',
    keys: [
      { key: 'q', in: 'agenda', preset: 'emacs' },
      { key: 'q', in: 'agenda', preset: 'vim' },
    ],
  },
];

const ORDER = ['org', 'global', 'capture', 'agenda'];
// Later bindings win in Sublime: a capture buffer's `C-c C-c` over an Org file's.
const bindings = [...manifest.commands, ...own, ...keysOnly]
  .flatMap((command) => command.keys.map((binding) => ({ ...binding, command: command.command })))
  .filter((binding) => ORDER.includes(binding.in))
  .sort((a, b) => ORDER.indexOf(a.in) - ORDER.indexOf(b.in))
  .flatMap((binding) => {
    const keys = binding.preset === 'emacs' ? fromEmacs(binding.key) : fromVim(binding.key);
    if (keys === null) return [];
    const inText = binding.in !== 'agenda';
    // In Vim's command mode a plain first key belongs to Vim (`c` of `cit`).
    if (binding.preset === 'vim' && inText && binding.mode !== 'insert' && keys[0].length === 1) return [];
    const scopes = binding.in === 'global' ? globalScopes : [binding.in];
    return scopes.map((scope) => {
      const context = [
        { key: 'org_lsp.keys', operator: 'equal', operand: binding.preset },
        { key: 'selector', operator: 'equal', operand: selector[scope] },
      ];
      if (scope === 'capture') context.push({ key: 'org_lsp.capture', operator: 'equal', operand: true });
      if (binding.preset === 'vim' && inText) {
        context.push({ key: 'setting.command_mode', operator: 'equal', operand: binding.mode !== 'insert' });
      }
      if (keys[0] === 'tab' || keys[0] === 'shift+tab' || ((keys[0] === 'enter' || keys[0] === 'shift+enter') && scope !== 'agenda')) {
        context.push(
          { key: 'auto_complete_visible', operator: 'equal', operand: false },
          { key: 'has_next_field', operator: 'equal', operand: false },
          { key: 'selection_empty', operator: 'equal', operand: true, match_all: true },
        );
      }
      const own = !binding.command.startsWith('org.');
      const entry = own
        ? { keys, command: binding.command, args: {} }
        : { keys, command: 'org_lsp_run', args: { command: binding.command } };
      if (binding.native) {
        const fallback = fallbacks[keys[0]];
        if (!fallback) throw new Error(`No fallback for the key ${keys.join(' ')} of ${binding.command}`);
        entry.args.fallback = fallback;
      }
      if (Object.keys(entry.args).length === 0) delete entry.args;
      entry.context = context;
      return entry;
    });
  });

// The Command Palette: every command of the server's and the package's.
const palette = [
  ...manifest.commands.map((command) => ({
    caption: `Org: ${command.title}`,
    command: 'org_lsp_run',
    args: { command: command.command },
  })),
  ...own.map((command) => ({ caption: `Org: ${command.title}`, command: command.command })),
  { caption: 'Org: Show the keys', command: 'org_lsp_help' },
  { caption: 'Org: Download the latest server', command: 'org_lsp_update_server' },
  {
    caption: 'Preferences: LSP-org Settings',
    command: 'edit_settings',
    args: { base_file: '${packages}/LSP-org/LSP-org.sublime-settings', default: '{\n\t$0\n}\n' },
  },
];

// The scopes of the server's tokens, from the TextMate scopes it names. A
// selector for a modifier on any type (`*.bold`) is given for text. All of
// them, as a stack with the most specific last: Sublime Text colors a token
// by the last scope its color scheme has a color for, so a scheme without
// one for `constant.other.timestamp.org` still colors it as
// `constant.numeric`.
const semanticTokens = Object.fromEntries(
  manifest.highlights
    .filter((look) => look.textmate.length > 0)
    .map((look) => [look.token.replace(/^\*\./, 'text.'), [...look.textmate].reverse().join(' ')]),
);

const settingsPath = join(pkg, 'LSP-org.sublime-settings');
const settings = {
  // The server: `org-lsp` from PATH, or the path to the binary.
  command: ['org-lsp'],
  selector: 'text.org | text.org-agenda',
  // LSP shows the agenda as a document of the server's own (`org-agenda:`)
  // where it can fetch one, in the syntax named for its scheme: without
  // one, as plain text, which neither the server nor the keys know.
  syntax_map: { [manifest.agenda.scheme]: 'Packages/LSP-org/Org Agenda.sublime-syntax' },
  // The schemes of the documents the server is for: LSP leaves a view of
  // any other out (no tokens, no edits), and shows the agenda in a new tab
  // each time the server asks, not knowing the one it has.
  schemes: ['file', manifest.agenda.scheme],
  // The server's options, such as agendaFiles; see docs/configuration.md.
  initializationOptions: {},
  // The keys: "emacs" (the default), "vim" (for Vintage and NeoVintageous)
  // or "custom" (none; bind the commands yourself).
  org_keys: 'emacs',
  semantic_tokens: semanticTokens,
};
if (existsSync(settingsPath)) {
  // The user-facing values stay as they are; only the tokens are generated.
  const before = JSON.parse(readFileSync(settingsPath, 'utf8'));
  for (const key of ['command', 'selector', 'initializationOptions', 'org_keys']) {
    if (key in before) settings[key] = before[key];
  }
}

// Source and export blocks, and inline source blocks, in their language's
// own syntax: rules for each language Sublime Text ships, between the
// markers of Org.sublime-syntax.
const sourceLanguages = JSON.parse(readFileSync(join(root, 'editors', 'src-languages.json'), 'utf8')).languages;
const quote = (pattern) => `'${pattern.replace(/'/g, "''")}'`;
const escapeRegex = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const embedded = sourceLanguages
  .filter((language) => language.sublime)
  .map((language) => {
    const names = language.names.map(escapeRegex).join('|');
    return [
      // `#+begin_src LANG` to `#+end_src`, `#+begin_export LANG` to
      // `#+end_export`.
      `    - match: ${quote(`^\\s*(?i:#\\+begin_(src|export))\\s+(?i:${names})(?=\\s|$).*$`)}`,
      '      scope: comment punctuation.definition.block.org',
      `      embed: scope:${language.sublime}`,
      '      embed_scope: meta.embedded.block.org',
      `      escape: ${quote('^\\s*(?i:#\\+end_\\1)\\b.*$')}`,
      '      escape_captures:',
      '        0: comment punctuation.definition.block.org',
      // `src_LANG[HEADERS]{CODE}`, to the first `}`.
      `    - match: ${quote(`(?<![\\w-])src_(?i:${names})(?:\\[[^\\]\\n]*\\])?\\{`)}`,
      '      scope: markup.inline.raw.org',
      `      embed: scope:${language.sublime}`,
      '      embed_scope: meta.embedded.inline.org',
      `      escape: ${quote('\\}')}`,
      '      escape_captures:',
      '        0: markup.inline.raw.org',
    ].join('\n');
  })
  .join('\n');
const syntaxPath = join(pkg, 'Org.sublime-syntax');
const syntaxBefore = readFileSync(syntaxPath, 'utf8');
const begin = '    # --- source blocks ---\n';
const end = '    # --- end of source blocks ---\n';
const syntax =
  syntaxBefore.slice(0, syntaxBefore.indexOf(begin) + begin.length) + embedded + '\n' + syntaxBefore.slice(syntaxBefore.indexOf(end));

const files = {
  'Org.sublime-syntax': syntax,
  'Default.sublime-keymap': JSON.stringify(bindings, null, 2) + '\n',
  'LSP-org.sublime-commands': JSON.stringify(palette, null, 2) + '\n',
  'LSP-org.sublime-settings': JSON.stringify(settings, null, 2) + '\n',
};
let stale = false;
for (const [name, text] of Object.entries(files)) {
  const path = join(pkg, name);
  const before = existsSync(path) ? readFileSync(path, 'utf8') : '';
  if (before === text) continue;
  if (check) {
    console.error(`${name} is out of date with the server. Run: node editors/sublime/generate.mjs`);
    stale = true;
  } else {
    writeFileSync(path, text);
  }
}
if (stale) process.exit(1);
console.log(check ? 'the Sublime Text package matches the server' : `generated ${bindings.length} keys, ${palette.length} commands`);
