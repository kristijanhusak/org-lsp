// Write the parts of package.json that mirror the server: its commands and
// their keys, the semantic tokens and how themes should color them, and the
// agenda's document type. They come from the server's manifest, so nothing
// about Org is listed by hand in this extension.
//
//   npm run generate [-- path/to/org-lsp]     rewrite package.json
//   npm run generate -- --check               fail if it is out of date
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

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

// Keys are bound in the agenda, the date prompt and capture buffers only.
// The first two are read-only, so plain letters are free there; in Org
// files Emacs's keys would take over Ctrl+C and Shift+arrows. A capture
// buffer is short-lived, and `C-c C-c` is how it is finished.
const scopeWhen = {
  agenda: `editorLangId == '${AGENDA}'`,
  calendar: `editorLangId == '${DATE}'`,
  capture: `editorLangId == '${ORG}' && resourceFilename =~ /^${manifest.capture.prefix}/`,
};
const keybindings = manifest.commands.flatMap((command) =>
  command.keys
    .filter((binding) => binding.preset === 'emacs' && scopeWhen[binding.in])
    .map((binding) => ({
      command: command.command,
      key: key(binding.key),
      when: `editorTextFocus && ${scopeWhen[binding.in]}`,
    })),
);

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
contributes.commands = [
  ...commands,
  ...own(
    contributes.commands,
    contributes.commands.filter((command) => command.command.startsWith('org.')).map((command) => command.command),
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
  [`[${ORG}]`]: editorDefaults,
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
// For the extension itself: which documents belong to the server.
pkg.orgLsp = {
  agendaScheme: manifest.agenda.scheme,
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
