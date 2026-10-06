// Write the parts of the Zed extension that mirror the server's manifest:
// the semantic token rules (languages/org/semantic_token_rules.json), how
// each token type and modifier looks in the names of Zed's themes; a task
// per agenda command (languages/org/tasks.json); and the agenda's keys for
// the user's keymap (keymap.json).
//
//   node editors/zed/generate.mjs [path/to/org-lsp]   rewrite the files
//   node editors/zed/generate.mjs --check             fail if out of date
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
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

// Headline levels take colors in turn, as Org's level faces do.
const levels = ['title', 'constant', 'function', 'keyword', 'preproc', 'type', 'variant', 'string'];
/** The look of each selector of the manifest (`type`, `type.modifier`, `*.modifier`) in Zed. */
const looks = {
  stars: ['punctuation.special'],
  todoKeyword: ['keyword'],
  'todoKeyword.done': ['comment'],
  priority: ['constant'],
  tag: ['tag'],
  planning: ['keyword'],
  timestamp: ['string.special'],
  'timestamp.inactive': ['comment'],
  drawer: ['comment'],
  property: ['property'],
  keyword: ['keyword'],
  block: ['keyword'],
  comment: ['comment'],
  bullet: ['punctuation.list_marker'],
  checkbox: ['punctuation.list_marker'],
  'checkbox.checked': ['comment'],
  'checkbox.partial': ['punctuation.list_marker'],
  link: ['link_uri'],
  code: ['text.literal'],
  verbatim: ['text.literal'],
  tableDelimiter: ['punctuation.delimiter'],
  footnote: ['link_uri'],
  cookie: ['number'],
  target: ['link_text'],
  latex: ['string.special'],
  entity: ['string.special'],
  macro: ['function'],
  '*.bold': { font_weight: 'bold' },
  '*.italic': { font_style: 'italic' },
  '*.underline': { underline: true },
  '*.strikethrough': { strikethrough: true },
  agendaStructure: ['title'],
  agendaDate: ['keyword'],
  'agendaDate.today': ['emphasis.strong'],
  'agendaDate.weekend': ['emphasis'],
  agendaScheduled: ['string'],
  'agendaScheduled.past': ['constant'],
  agendaDeadline: ['keyword'],
  'agendaDeadline.imminent': ['keyword'],
  'agendaDeadline.distant': ['comment'],
  agendaDone: ['comment'],
  agendaTimeGrid: ['comment'],
  'agendaTimeGrid.current': ['emphasis.strong'],
  agendaClocking: ['emphasis.strong'],
  '*.archived': ['comment'],
};
levels.forEach((style, index) => {
  looks[`headline.level${index + 1}`] = [style];
  looks[`stars.level${index + 1}`] = [style];
});

function rule(selector, look) {
  const [type, modifier] = selector.split('.');
  const entry = {};
  if (type !== '*') entry.token_type = type;
  if (modifier) entry.token_modifiers = [modifier];
  return Array.isArray(look) ? { ...entry, style: look } : { ...entry, ...look };
}

const selectors = manifest.highlights.map((look) => look.token).filter((token) => looks[token]);
// Zed applies every rule that matches, and of two that set the same
// thing the one earlier in the file wins (as a user's rules win over an
// extension's): a type with a modifier first (`todoKeyword.done` over
// `todoKeyword`), then the plain types, then the modifiers of any type.
// An archived headline is shadowed over all its colors (`org-archived` is
// prepended), so that one goes before everything.
const rank = (selector) =>
  selector === '*.archived' ? -1 : selector.startsWith('*.') ? 2 : selector.includes('.') ? 0 : 1;
const rules = selectors.sort((a, b) => rank(a) - rank(b)).map((selector) => rule(selector, looks[selector]));
const text = JSON.stringify(rules, null, 2) + '\n';

// The agenda's keys. An extension cannot bind keys, and a key of Zed's
// cannot run a language server's command, but it can run a task, and the
// extension's tasks (languages/org/tasks.json) are there in Org buffers.
// With `commandActions` the server writes `org-lsp-command` next to the
// agenda, which runs a command where the cursor is: a task per agenda
// command calls it, and keymap.json, for the user's keymap, binds the
// agenda's keys to the tasks.
/** An Emacs key (`C-c C-s`, `S-<right>`, `J`) in Zed's notation. */
function zedKey(emacs) {
  const names = { RET: 'enter', TAB: 'tab', SPC: 'space', DEL: 'backspace', ESC: 'escape' };
  const modifiers = { C: 'ctrl', M: 'alt', S: 'shift' };
  return emacs
    .split(' ')
    .map((stroke) => {
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
      return [...parts, base].join('-');
    })
    .join(' ');
}
const agendaCommands = manifest.commands
  .map((command) => ({
    command: command.command,
    label: `Org agenda: ${command.title.replace(/^Agenda: /, '')}`,
    keys: command.keys.filter((key) => key.in === 'agenda' && key.preset === 'emacs').map((key) => zedKey(key.key)),
  }))
  .filter((command) => command.keys.length > 0);
const tasks = agendaCommands.map((command) => ({
  label: command.label,
  command: '$ZED_DIRNAME/org-lsp-command',
  args: [command.command, '$ZED_ROW', '$ZED_COLUMN'],
  tags: ['org-agenda'],
  // Out of sight: a terminal shows only when the command fails.
  reveal: 'never',
  hide: 'on_success',
  show_summary: false,
  show_command: false,
  allow_concurrent_runs: true,
  use_new_terminal: false,
}));
const bindings = {
  // Zed's own: close the agenda, and go to a line's entry (the server
  // answers go to definition there) in this pane or in a split.
  q: 'pane::CloseActiveItem',
  enter: 'editor::GoToDefinition',
  tab: 'editor::GoToDefinitionSplit',
};
for (const command of agendaCommands) {
  for (const key of command.keys) {
    if (!(key in bindings)) bindings[key] = ['task::Spawn', { task_name: command.label }];
  }
}
const keymap = [{ context: 'Editor && extension == org-agenda', bindings }];
const files = {
  [join(here, 'languages', 'org', 'semantic_token_rules.json')]: text,
  [join(here, 'languages', 'org', 'tasks.json')]: JSON.stringify(tasks, null, 2) + '\n',
  [join(here, 'keymap.json')]: JSON.stringify(keymap, null, 2) + '\n',
};

let stale = false;
for (const [path, text] of Object.entries(files)) {
  const before = existsSync(path) ? readFileSync(path, 'utf8') : '';
  if (before === text) continue;
  if (check) {
    console.error(`${path} is out of date with the server. Run: node editors/zed/generate.mjs`);
    stale = true;
  } else {
    writeFileSync(path, text);
  }
}
if (stale) process.exit(1);
console.log(check ? 'the Zed extension matches the server' : `generated ${rules.length} semantic token rules, ${tasks.length} agenda tasks`);
