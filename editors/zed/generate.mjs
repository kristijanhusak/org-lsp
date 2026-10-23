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
  // `org-list-dt`: bold, with no color of its own.
  descriptionTerm: { font_weight: 'bold' },
  link: ['link_uri'],
  code: ['text.literal'],
  verbatim: ['text.literal'],
  linkDescription: ['link_text'],
  linkUrl: ['link_uri'],
  tableDelimiter: ['punctuation.delimiter'],
  tableHeader: ['title'],
  footnote: ['link_uri'],
  cookie: ['number'],
  target: ['link_text'],
  latex: ['string.special'],
  latexEnvironment: ['string.special'],
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
  agendaDimmed: ['comment'],
  agendaTimeGrid: ['comment'],
  'agendaTimeGrid.current': ['emphasis.strong'],
  '*.archived': ['comment'],
  // `org-headline-done` and `org-headline-todo` past a keyword, the
  // statistics cookie of `org-checkbox-statistics-done`, `org-inlinetask`.
  'headline.done': ['comment'],
  '*.done': ['comment'],
  'headline.todo': ['string'],
  '*.todo': ['string'],
  'cookie.done': ['comment'],
  '*.inlinetask': ['comment'],
  documentTitle: ['title'],
  documentInfo: ['string'],
};
levels.forEach((style, index) => {
  looks[`headline.level${index + 1}`] = [style];
  looks[`stars.level${index + 1}`] = [style];
});

// The looks of Org's that are a background only (the days of a habit's
// graph, the entry the clock runs on) are the manifest's. Zed lets no
// extension highlight a line from edge to edge: the clocked entry's line
// has its background under its text, which its tokens cover, not past
// its end. A rule's color is the same with every theme: Org's for a dark
// background, a little transparent, which shows close to them on a dark
// theme and as a lighter shade of them on a light one; more transparent
// under the clocked entry's text, which it must leave readable.
const backgrounds = Object.fromEntries(
  manifest.highlights
    .filter((look) => look.background)
    .map((look) => [look.token, look.background.dark + (look.token.startsWith('agendaHabit') ? 'b3' : '66')]),
);

function rule(selector, look) {
  const [type, ...modifiers] = selector.split('.');
  const entry = {};
  if (type !== '*') entry.token_type = type;
  if (modifiers.length > 0) entry.token_modifiers = modifiers;
  if (backgrounds[selector]) entry.background_color = backgrounds[selector];
  if (!look) return entry;
  return Array.isArray(look) ? { ...entry, style: look } : { ...entry, ...look };
}

const selectors = manifest.highlights
  .map((look) => look.token)
  .filter((token) => looks[token] || backgrounds[token]);
// Zed applies every rule that matches, and of two that set the same
// thing the one earlier in the file wins (as a user's rules win over an
// extension's): a type with several modifiers first
// (`agendaHabit.ready.future` over `agendaHabit.ready`), then those with
// one (`todoKeyword.done` over `todoKeyword`), then the plain types, then
// the modifiers of any type.
// What Org prepends over a headline's other faces (`org-archived`,
// `org-headline-done`, `org-headline-todo`, `org-inlinetask`) goes before
// everything, as a priority or tags in it take that look.
const over = ['*.archived', '*.done', '*.todo', '*.inlinetask'];
const rank = (selector) => {
  const dots = selector.split('.').length - 1;
  return over.includes(selector) ? -2 : selector.startsWith('*.') ? 2 : dots > 1 ? -1 : dots === 1 ? 0 : 1;
};
const rules = selectors.sort((a, b) => rank(a) - rank(b)).map((selector) => rule(selector, looks[selector]));
const text = JSON.stringify(rules, null, 2) + '\n';

// The keys. An extension cannot bind keys, and a key of Zed's
// cannot run a language server's command, but it can run a task. A task
// of the extension (a language's) does not do: Zed looks for those only
// in files of the project's folders, and the agenda is a file of the
// server's own, elsewhere. So tasks.json is for the user's own tasks
// (`zed: open tasks`), and keymap.json, for the user's keymap, binds the
// agenda's keys, and Org's in Org files, to them. With `command_actions`
// the server writes `~/.local/state/org-lsp/agenda-command`, which runs a
// command where the cursor last was (the server follows it): in the
// agenda, on its line; with `file:`, at the cursor in an Org file; with
// `key:`, the command a key has there, which differs in a capture buffer
// (`C-c C-c` files the capture). The tasks use no `$ZED_FILE` or
// `$ZED_ROW`: Zed offers no task that uses them in a file outside the
// project, as the agenda and a capture buffer are. The commands that need
// no agenda (the agenda itself, the TODO list, the agenda menu, capture)
// run anywhere, from the task picker too; with no server running, the
// script starts one.
// The command is one string, with no `args`: Zed quotes a command that has
// arguments, and `$HOME` would then not be expanded.
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
// Zed offers a task that names `$ZED_FILE` or `$ZED_ROW` only in files of
// the project's folders, and the agenda and capture buffers are elsewhere:
// a task names none. The server follows the cursor through the document
// highlights Zed asks for as it moves, and runs an agenda task where it is
// in the agenda, and an Org file's (`file:COMMAND`) where it is in an Org
// file or a capture buffer. Keys that keep a meaning of their own where
// the command does not apply (`native`: Shift and Alt with the arrows,
// Enter) stay Zed's.
const keysIn = (command, scopes) =>
  command.keys.filter((key) => scopes.includes(key.in) && key.preset === 'emacs' && !key.native).map((key) => key.key);
const task = (label, run, tag, keys) => ({ label, run, tag, keys });
// What opens the agenda and capture, from anywhere (the task picker): the
// global commands, and the two views the agenda menu has first.
const OPENING = ['org.agenda.list', 'org.agenda.todos'];
const opening = manifest.commands
  .filter((command) => OPENING.includes(command.command) || keysIn(command, ['global']).length > 0)
  .map((command) => task(`Org: ${command.title}`, command.command, 'org', keysIn(command, ['global'])));
const openingIds = new Set(opening.map((task) => task.run));
// `org-save-all-org-buffers` is Zed's own Save All: a task, like the
// server, cannot save Zed's files.
const SAVE_ALL = 'org.agenda.save';
const inAgenda = manifest.commands
  .filter((command) => keysIn(command, ['agenda']).length > 0 && command.command !== SAVE_ALL)
  .map((command) =>
    task(`Org agenda: ${command.title.replace(/^Agenda: /, '')}`, command.command, 'org-agenda', keysIn(command, ['agenda'])),
  );
// A key that runs one command in a capture buffer and another in an Org
// file (`C-c C-c` files the entry, or toggles a checkbox): a task of its
// own, which has the server pick.
const commandsOfKey = (key, scope) =>
  manifest.commands.filter((command) => keysIn(command, [scope]).includes(key)).map((command) => command.command);
const shared = new Set(
  [...new Set(manifest.commands.flatMap((command) => keysIn(command, ['org', 'capture'])))].filter((key) => {
    const [org, capture] = [commandsOfKey(key, 'org'), commandsOfKey(key, 'capture')];
    return org.length > 0 && capture.length > 0 && org.join() !== capture.join();
  }),
);
const byKey = [...shared].map((key) => task(`Org: ${key}`, `'key:${key}'`, 'org', [key]));
const inFiles = manifest.commands
  .filter((command) => !openingIds.has(command.command))
  .map((command) => [command, keysIn(command, ['org', 'capture']).filter((key) => !shared.has(key))])
  .filter(([, keys]) => keys.length > 0)
  .map(([command, keys]) => task(`Org: ${command.title}`, `file:${command.command}`, 'org', keys));
const tasks = [...opening, ...inFiles, ...byKey, ...inAgenda].map((task) => ({
  label: task.label,
  command: `$HOME/.local/state/org-lsp/agenda-command ${task.run}`,
  tags: [task.tag],
  // Out of sight: a terminal shows only when the command fails.
  reveal: 'never',
  hide: 'on_success',
  show_summary: false,
  show_command: false,
  allow_concurrent_runs: true,
  use_new_terminal: false,
}));
const spawn = (task) => ['task::Spawn', { task_name: task.label }];
const bindAll = (bindings, tasks) => {
  for (const task of tasks) {
    for (const key of task.keys) {
      if (!(zedKey(key) in bindings)) bindings[zedKey(key)] = spawn(task);
    }
  }
  return bindings;
};
// Not while a rename is typed (`renaming`), the answer to what a command
// asks: the agenda's keys are letters, and its editor sits in the agenda's.
const keymap = [
  {
    context: 'Editor && !renaming && extension == org-agenda',
    bindings: bindAll(
      {
        // Zed's own: close the agenda, and go to a line's entry (the server
        // answers go to definition there) in this pane or in a split.
        q: 'pane::CloseActiveItem',
        enter: 'editor::GoToDefinition',
        tab: 'editor::GoToDefinitionSplit',
        ...Object.fromEntries(
          keysIn(
            manifest.commands.find((command) => command.command === SAVE_ALL),
            ['agenda'],
          ).map((key) => [zedKey(key), 'workspace::SaveAll']),
        ),
      },
      [...inAgenda, ...opening],
    ),
  },
  // In Org files and capture buffers: Org's keys.
  { context: 'Editor && !renaming && extension == org', bindings: bindAll({}, [...byKey, ...inFiles, ...opening]) },
];
const files = {
  [join(here, 'languages', 'org', 'semantic_token_rules.json')]: text,
  [join(here, 'tasks.json')]: JSON.stringify(tasks, null, 2) + '\n',
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
console.log(check ? 'the Zed extension matches the server' : `generated ${rules.length} semantic token rules, ${tasks.length} tasks`);
