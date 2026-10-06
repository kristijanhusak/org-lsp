// Write the parts of package.json that mirror the server: its commands and
// their keys (with the extension's own), the semantic tokens and how themes should color them, the
// agenda's document type, and the server's options. They come from the server's manifest, so nothing
// about Org is listed by hand in this extension.
//
//   npm run generate [-- path/to/org-lsp]     rewrite package.json
//   npm run generate -- --check               fail if it is out of date
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { schemaProperties } from '../../../scripts/settings-schema.mjs';

const args = process.argv.slice(2);
const check = args.includes('--check');
const exe = process.platform === 'win32' ? 'org-lsp.exe' : 'org-lsp';
const binary =
  args.find((arg) => !arg.startsWith('--')) ??
  ['release', 'debug'].map((profile) => join('..', '..', 'target', profile, exe)).find(existsSync);
if (!binary) {
  console.error('No server binary found. Build it with: cargo build -p org-lsp');
  process.exit(1);
}
const manifest = JSON.parse(execFileSync(resolve(binary), ['manifest'], { encoding: 'utf8' }));
const settings = JSON.parse(execFileSync(resolve(binary), ['settings'], { encoding: 'utf8' }));

const ORG = 'org';
const AGENDA = manifest.agenda.languageId;
const DATE = manifest.datePrompt.languageId;

/** An Emacs key such as `C-c a`, `RET` or `S-<right>` in VS Code's notation. */
function key(emacs) {
  const names = { RET: 'enter', TAB: 'tab', SPC: 'space', DEL: 'backspace', ESC: 'escape' };
  const modifiers = { C: 'ctrl', M: 'alt', S: 'shift' };
  // VS Code names a symbol by the key it is typed with, as on a US
  // keyboard, and `+` separates keys there.
  const shifted = {
    '+': '=', '_': '-', '<': ',', '>': '.', '?': '/', ':': ';', '"': "'", '{': '[', '}': ']', '|': '\\',
    '~': '`', '!': '1', '@': '2', '#': '3', $: '4', '%': '5', '^': '6', '&': '7', '*': '8', '(': '9', ')': '0',
  };
  return emacs
    .split(' ')
    .map((stroke) => {
      const parts = [];
      let rest = stroke;
      for (let match; (match = /^([CMS])-(.+)$/.exec(rest)); rest = match[2]) {
        parts.push(modifiers[match[1]]);
      }
      if (shifted[rest]) {
        parts.push('shift');
        rest = shifted[rest];
      }
      const named = /^<(.+)>$/.exec(rest);
      const base = names[rest] ?? (named ? named[1] : rest);
      if (base.length > 1 && !named && !names[rest]) {
        throw new Error(`Cannot translate the key \`${emacs}\``);
      }
      if (base.length === 1 && base !== base.toLowerCase()) {
        parts.push('shift');
      }
      return [...parts, base.toLowerCase()].join('+');
    })
    .join(' ');
}

const commands = manifest.commands.map((command) => ({
  command: command.command,
  title: command.title,
  category: 'Org',
}));

/**
 * A Vim key such as `<Leader>oa`, `cit`, `<C-Space>` or `<S-Right>` in
 * Emacs's notation, or null when VS Code cannot bind it (the leader).
 */
function fromVim(vim) {
  const named = { CR: 'RET', Tab: 'TAB', Esc: 'ESC', Space: 'SPC', BS: 'DEL' };
  const strokes = [];
  for (const [, special, plain] of vim.matchAll(/<([^>]+)>|(.)/g)) {
    if (plain !== undefined) {
      strokes.push(plain);
      continue;
    }
    const parts = special.split('-');
    const base = parts.pop();
    if (base === 'Leader') {
      return null;
    }
    const modifiers = parts.map((part) => (part === 'A' ? 'M' : part)).map((part) => `${part}-`).join('');
    strokes.push(modifiers + (named[base] ?? (base.length > 1 ? `<${base.toLowerCase()}>` : base.toLowerCase())));
  }
  return strokes.join(' ');
}

// The setting `org-lsp.keys` picks the keys: Emacs's (the default), the
// Neovim plugin's for VSCodeVim, or none.
const PRESET = 'org-lsp.keys';
const preset = (name) => `config.${PRESET} == '${name}'`;
const scopeWhen = {
  org: `editorLangId == '${ORG}'`,
  agenda: `editorLangId == '${AGENDA}'`,
  calendar: `editorLangId == '${DATE}'`,
  capture: `editorLangId == '${ORG}' && resourceFilename =~ /^${manifest.capture.prefix}/`,
};
// Keys for the whole editor are bound where `C-c` is Org's already.
const globalScopes = ['org', 'agenda'];
// Keys that keep their own meaning where the command does not apply: the
// command VS Code (or VSCodeVim) runs for them otherwise, tried in turn.
const fallbacks = {
  'shift+up': ['cursorUpSelect'],
  'shift+down': ['cursorDownSelect'],
  'shift+left': ['cursorLeftSelect'],
  'shift+right': ['cursorRightSelect'],
  'ctrl+a': ['extension.vim_ctrl+a', 'editor.action.selectAll'],
  'ctrl+x': ['extension.vim_ctrl+x', 'editor.action.clipboardCutAction'],
  tab: ['tab'],
};
// In Org files, VSCodeVim's keys apply in Normal mode only, and keys that
// start with a plain letter would take it from Vim (`c` of `cit`).
const vimNormal = "(!vim.active || vim.mode == 'Normal')";
// Tab does not cycle where VS Code needs it.
const tabFree = '!suggestWidgetVisible && !inlineSuggestionVisible && !inSnippetMode && !editorHasSelection && !editorTabMovesFocus';

/** The commands of the extension itself, with their keys. */
const ownCommands = [
  {
    command: 'org-lsp.cycle',
    title: 'Cycle the visibility of the subtree, block or drawer at the cursor',
    keys: [
      { key: 'TAB', in: 'org', preset: 'emacs', native: true },
      { key: '<Tab>', in: 'org', preset: 'vim' },
    ],
  },
  {
    command: 'org-lsp.cycleGlobal',
    title: 'Cycle the visibility of the whole file',
    keys: [
      { key: 'S-TAB', in: 'org', preset: 'emacs' },
      { key: '<S-Tab>', in: 'org', preset: 'vim' },
    ],
  },
];

const ORDER = ['org', 'global', 'capture', 'agenda', 'calendar'];
// Later bindings win: a capture buffer's `C-c C-c` over an Org file's.
const keybindings = [...manifest.commands, ...ownCommands]
  .flatMap((command) => command.keys.map((binding) => ({ command: command.command, ...binding })))
  .sort((a, b) => ORDER.indexOf(a.in) - ORDER.indexOf(b.in))
  .flatMap((binding) => {
    const emacs = binding.preset === 'emacs' ? binding.key : fromVim(binding.key);
    if (emacs === null) {
      return [];
    }
    const vsKey = key(emacs);
    const inText = binding.in === 'org' || binding.in === 'capture' || binding.in === 'global';
    if (binding.preset === 'vim' && inText && !/[+]/.test(vsKey.split(' ')[0]) && vsKey.split(' ')[0].length === 1) {
      return [];
    }
    const conditions = [];
    if (binding.preset === 'vim' && (binding.in === 'org' || binding.in === 'capture')) {
      conditions.push(vimNormal);
    }
    if (vsKey === 'tab' || vsKey === 'shift+tab') {
      conditions.push(tabFree);
    }
    const scopes = binding.in === 'global' ? globalScopes : [binding.in];
    return scopes.map((scope) => {
      const entry = {
        command: binding.command,
        key: vsKey,
        when: ['editorTextFocus', preset(binding.preset), scopeWhen[scope], ...conditions].join(' && '),
      };
      if (binding.native) {
        if (!fallbacks[vsKey]) {
          throw new Error(`No fallback for the key \`${vsKey}\` of ${binding.command}`);
        }
        entry.args = { fallback: fallbacks[vsKey] };
      }
      return entry;
    });
  });

const scopes = Object.fromEntries(
  manifest.highlights.filter((look) => look.textmate.length > 0).map((look) => [look.token, look.textmate]),
);

// `/Org%20Agenda` in the agenda's URI is the file name VS Code sees.
const agendaName = decodeURIComponent(manifest.agenda.uri.split('/').pop());
const dateName = decodeURIComponent(manifest.datePrompt.uri.split('/').pop());
const agendaExtension = '.' + manifest.agenda.fileName.split('.').pop();

const path = 'package.json';
const before = readFileSync(path, 'utf8');
const pkg = JSON.parse(before);
const own = (list, generated, id) => (list ?? []).filter((item) => !generated.includes(item[id]));
const contributes = pkg.contributes;

contributes.languages = [
  ...contributes.languages.filter((language) => language.id !== AGENDA && language.id !== DATE),
  { id: AGENDA, aliases: ['Org Agenda'], extensions: [agendaExtension], filenames: [agendaName] },
  { id: DATE, aliases: ['Org Date Prompt'], filenames: [dateName] },
];
const ownIds = ownCommands.map((command) => command.command);
contributes.commands = [
  ...commands,
  ...ownCommands.map((command) => ({ command: command.command, title: command.title, category: 'Org' })),
  ...own(
    contributes.commands,
    contributes.commands
      .filter((command) => command.command.startsWith('org.') || ownIds.includes(command.command))
      .map((command) => command.command),
    'command',
  ),
];
contributes.keybindings = keybindings;
contributes.semanticTokenTypes = manifest.tokenTypes
  .filter((type) => !type.standard)
  .map((type) => ({ id: type.id, description: type.description }));
contributes.semanticTokenModifiers = manifest.tokenModifiers.map((modifier) => ({
  id: modifier.id,
  description: modifier.description,
}));
contributes.semanticTokenScopes = [ORG, AGENDA, DATE].map((language) => ({ language, scopes }));
// The server's tokens are the highlighting. Its commands are code actions,
// which would light the bulb on every headline. Brackets are link syntax in
// Org, not nesting to be colored by depth.
const editorDefaults = {
  'editor.semanticHighlighting.enabled': true,
  'editor.lightbulb.enabled': 'off',
  'editor.bracketPairColorization.enabled': false,
};
contributes.configurationDefaults = {
  ...contributes.configurationDefaults,
  // New lines are indented by the server, as Org's RET does
  // (`org-indent-line`, on-type formatting).
  [`[${ORG}]`]: { ...editorDefaults, 'editor.formatOnType': true },
  // The agenda is a list to read, not code: no gutter, guides or wrapping.
  [`[${AGENDA}]`]: {
    ...editorDefaults,
    'editor.lineNumbers': 'off',
    'editor.wordWrap': 'off',
    'editor.folding': false,
    'editor.guides.indentation': false,
    'editor.occurrencesHighlight': 'off',
    'editor.selectionHighlight': false,
    'editor.stickyScroll.enabled': false,
  },
  // The date prompt is a calendar to move about in, like the agenda.
  [`[${DATE}]`]: {
    ...editorDefaults,
    'editor.lineNumbers': 'off',
    'editor.wordWrap': 'off',
    'editor.folding': false,
    'editor.guides.indentation': false,
    'editor.occurrencesHighlight': 'off',
    'editor.selectionHighlight': false,
    'editor.stickyScroll.enabled': false,
    'editor.minimap.enabled': false,
    'editor.renderWhitespace': 'none',
  },
};
// The server's options, so that settings.json completes and checks them.
contributes.configuration.properties['org-lsp.settings'].properties = schemaProperties(settings, { markdown: true });

// For the extension itself: which documents belong to the server.
pkg.orgLsp = {
  agendaScheme: manifest.agenda.scheme,
  capturePrefix: manifest.capture.prefix,
  datePromptUri: manifest.datePrompt.uri,
  languages: [ORG, AGENDA, DATE],
};

const after = JSON.stringify(pkg, null, 2) + '\n';
if (check) {
  if (after !== before) {
    console.error('package.json is out of date with the server. Run: npm run generate');
    process.exit(1);
  }
  console.log('package.json matches the server');
} else {
  writeFileSync(path, after);
  console.log(`generated ${commands.length} commands, ${keybindings.length} keys, ${Object.keys(scopes).length} token looks`);
}
