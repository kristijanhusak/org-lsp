// Write the generated parts of the documentation, in Org, from what the
// server says about itself, so that it lists the keys the editors really
// bind and the options and defaults the server really has:
//
//   docs/keys/<editor>.org           the key tables (Sublime Text's from its
//                                    keymap, Zed's from keymap.json and
//                                    tasks.json, which their own generators
//                                    write from the server's manifest)
//   docs/configuration.org           a heading for each option, with its
//                                    default
//   docs/protocol.org                the table of options
//   docs/<editor>.org, README.org    what works, from scripts/features.mjs
//   docs/parity.org                  the same area by area (whole), and
//                                    each of Org's options and what
//                                    org-lsp does with it, from
//                                    scripts/org-options.json
//   docs/emacs-options.org           each option `emacs_options` takes,
//                                    with Org's documentation of it, from
//                                    scripts/emacs-options.json
//   editors/<editor>/TESTING.org     what to try by hand (whole)
//   docs/config.schema.json          the JSON Schema of the shared file
//   docs/config.toml                 the shared file, every option at its default
//   lua/org-lsp/types.lua            the options' types, for lua-language-server
//
// In a page written by hand, only the lines between `# BEGIN generated:
// NAME` and `# END generated` are rewritten.
//
//   node scripts/generate-docs.mjs [path/to/org-lsp]   rewrite the pages
//   node scripts/generate-docs.mjs --check             fail if out of date
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, schemaProperties } from './settings-schema.mjs';
import { AREAS, EDITORS, overall, parity, support } from './features.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
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
const run = (command) => JSON.parse(execFileSync(resolve(binary), [command], { encoding: 'utf8' }));
const manifest = run('manifest');
const settings = run('settings');
const vscode = JSON.parse(readFileSync(join(root, 'editors', 'vscode', 'package.json'), 'utf8'));

// ----- Org's options (scripts/org-options.json)

/** Every option of Org 9.8.10 (`defcustom`), by the file it is in, and what
 * org-lsp does with it. */
const OPTIONS = JSON.parse(readFileSync(join(root, 'scripts', 'org-options.json'), 'utf8'));
/** What an option's status means. */
const STATUSES = {
  setting: 'A setting of the same name',
  missing: 'org-lsp does what the option is about, but the option cannot be set yet',
  other: 'Done another way, as its note says',
  emacs: 'Read by Emacs, through which org-lsp does that part of Org (export, Babel, attachments, column view…): set it in ~emacs_options~ (or ~emacs_config~); [[file:emacs-options.org][emacs-options.org]] lists them',
  lisp: 'Its value is Lisp (a function, a hook), which the server does not run',
  absent: 'Part of something org-lsp does not do',
  'emacs-only': 'About Emacs itself (windows, faces, menus, the mouse, the mode line): nothing to do in an editor',
};
const statusOf = (value) => (Array.isArray(value) ? value[0] : value);
/** The libraries of Emacs, not of Org, that Org hands its work to
 * (`org-agenda-to-appt` gives `appt.el` its appointments): those of their
 * options org-lsp has as settings are listed too, and are not counted as
 * Org's. */
const EMACS_FILES = new Set(['appt.el']);
/** What Emacs says of each option marked `emacs`, by file: its docstring,
 * default, type and version (scripts/emacs_options_reference.py). */
const EMACS_OPTIONS = JSON.parse(readFileSync(join(root, 'scripts', 'emacs-options.json'), 'utf8'));

/** What is wrong with scripts/org-options.json: an option marked as a
 * setting that the server does not have, or the other way round, a status
 * it does not know, and, with `ORG_LISP_DIR`, an option of Org it does not
 * list or one Org no longer has. */
function optionProblems() {
  const problems = [];
  const names = new Set(settings.map((setting) => setting.name));
  const listed = new Set();
  for (const [file, options] of Object.entries(OPTIONS)) {
    for (const [name, value] of Object.entries(options)) {
      listed.add(name);
      const status = statusOf(value);
      const setting = names.has(name.replaceAll('-', '_'));
      if (!(status in STATUSES)) problems.push(`${name} (${file}): no such status \`${status}\``);
      else if (setting && status !== 'setting') problems.push(`${name} is a setting now: mark it \`setting\``);
      else if (!setting && status === 'setting') problems.push(`${name} is marked \`setting\`, but the server has no ${name.replaceAll('-', '_')}`);
    }
  }
  const lisp = process.env.ORG_LISP_DIR;
  if (lisp && existsSync(lisp)) {
    const found = new Set();
    for (const file of readdirSync(lisp).filter((name) => name.endsWith('.el'))) {
      for (const match of readFileSync(join(lisp, file), 'utf8').matchAll(/^\(defcustom\s+([^\s()]+)/gm)) {
        found.add(match[1]);
        if (!listed.has(match[1])) problems.push(`${match[1]} (${file}) is not in scripts/org-options.json`);
      }
    }
    for (const [file, options] of Object.entries(OPTIONS)) {
      if (EMACS_FILES.has(file)) continue;
      for (const name of Object.keys(options)) if (!found.has(name)) problems.push(`${name} is no longer an option of Org`);
    }
  }
  // scripts/emacs-options.json has the options marked `emacs`, no more.
  const recorded = new Map(
    Object.entries(EMACS_OPTIONS).flatMap(([file, options]) => Object.keys(options).map((name) => [`${file} ${name}`, options[name]])),
  );
  for (const [file, options] of Object.entries(OPTIONS)) {
    for (const [name, value] of Object.entries(options)) {
      if (statusOf(value) !== 'emacs') continue;
      const entry = recorded.get(`${file} ${name}`);
      if (!entry) problems.push(`${name} (${file}) is not in scripts/emacs-options.json: run scripts/emacs_options_reference.py`);
      else if (entry.error) problems.push(`${name} (${file}) has no documentation in scripts/emacs-options.json: ${entry.error}`);
      recorded.delete(`${file} ${name}`);
    }
  }
  for (const key of recorded.keys()) {
    const [file, name] = key.split(' ');
    problems.push(`${name} (${file}) is in scripts/emacs-options.json, not marked \`emacs\`: run scripts/emacs_options_reference.py`);
  }
  return problems;
}

const SCOPES = [
  ['global', 'Anywhere'],
  ['org', 'In Org files'],
  ['capture', 'In a capture buffer'],
  ['agenda', 'In the agenda'],
  ['calendar', 'In the date prompt'],
  ['columns', 'In column view'],
];

// ----- Org

// Where Org lets emphasis start and end (`org-emphasis-regexp-components`).
const PRE = /[\s\-('"{\u200b]/;
const POST = /[\s\-.,:!?;'")}\[\u200b]/;

/** `text` as Org code: `~text~`, or `=text=` where a `~` at its edge or
 * in it would end it early. */
const code = (text) => {
  const bad = (marker) =>
    text.startsWith(marker) ||
    text.endsWith(marker) ||
    text.split(marker).slice(1).some((after) => after === '' || POST.test(after[0]));
  const marker = bad('~') && !bad('=') ? '=' : '~';
  return marker + text + marker;
};

/**
 * Text of the server's descriptions and of scripts/features.mjs, whose
 * code is in backticks and whose names of menu items and buttons are in
 * `*stars*`, as Org: `~code~` and `/italic/`. A zero-width space goes
 * where the characters around would keep Org from seeing them.
 */
function org(text) {
  const parts = text.split(/(`[^`]+`)/);
  return parts
    .map((part, index) => {
      if (index % 2 === 1) {
        const before = index > 0 ? parts[index - 1].slice(-1) : '';
        const after = parts[index + 1]?.[0] ?? '';
        const open = before === '' || PRE.test(before) ? '' : '\u200b';
        const close = after === '' || POST.test(after) ? '' : '\u200b';
        return open + code(part.slice(1, -1)) + close;
      }
      return part.replace(/(^|[\s(])\*([^*\s][^*]*[^*\s]|[^*\s])\*(?=$|[\s.,:;)])/g, '$1/$2/');
    })
    .join('');
}

/** A table cell: Org has no `|` in a cell; `¦` stands in for it. */
const cell = (text) => text.replaceAll('|', '¦');

/** An Org table, its first row the header; with a note under it when a
 * cell had a `|`. */
function table(header, rows) {
  const lines = [
    `| ${header.join(' | ')} |`,
    `|${header.map(() => '---').join('+')}|`,
    ...rows.map((row) => `| ${row.map(cell).join(' | ')} |`),
  ];
  const piped = rows.some((row) => row.some((text) => text.includes('|')));
  return lines.join('\n') + (piped ? '\n\n=¦= stands for =|=, which an Org table cannot hold.' : '');
}

/** A heading of `level`. */
const heading = (level, text) => `${'*'.repeat(level)} ${text}`;

/** A source block, its lines escaped as Org wants them. */
function block(language, lines) {
  const escaped = lines.map((line) => line.replace(/^(\s*)(\*|#\+|,\*|,#\+)/, '$1,$2'));
  return [`#+begin_src ${language}`, ...escaped, '#+end_src'].join('\n');
}

/** An Org link. */
const link = (target, text) => `[[${target}][${text}]]`;

// ----- keys

/** The keys the Neovim and Vim plugins keep for their own commands, by
 * scope: a command of the server's goes without them there. */
const OWN_KEYS = { org: ['<Tab>', '<S-Tab>', 'g?'], agenda: ['q', 'K', 'g?'], columns: ['h', 'l', '<Left>', '<Right>'] };

/** The Neovim plugin's keys: the `vim` preset of every command, in the
 * scopes the plugin has (all of them for Neovim). */
function neovimKeys(scopes = SCOPES.map(([scope]) => scope)) {
  const sections = [];
  for (const [scope, title] of SCOPES.filter(([scope]) => scopes.includes(scope))) {
    const rows = manifest.commands.flatMap((command) => {
      const keys = command.keys.filter(
        (key) => key.preset === 'vim' && key.in === scope && !(OWN_KEYS[scope] ?? []).includes(key.key),
      );
      if (keys.length === 0) return [];
      const shown = keys.map((key) => code(key.key) + (key.native ? ' ¹' : '') + (key.mode === 'insert' ? ' (insert mode)' : '')).join(', ');
      return [[shown, code(command.command), org(command.title)]];
    });
    if (rows.length > 0) sections.push(heading(2, title) + '\n\n' + table(['Keys', 'Command', 'What it does'], rows));
  }
  const keyless = manifest.commands
    .filter(
      (command) =>
        !command.keys.some(
          (key) => key.preset === 'vim' && scopes.includes(key.in) && !(OWN_KEYS[key.in] ?? []).includes(key.key),
        ),
    )
    .map((command) => [code(command.command), org(command.title)]);
  sections.push(heading(2, 'Without a default key') + '\n\n' + table(['Command', 'What it does'], keyless));
  sections.push(
    '¹ Where the command does not apply, the key does what it does in any\n' +
      'buffer: ~<C-a>~ off a timestamp still increments the number under the\n' +
      'cursor, ~<S-Right>~ off a timestamp goes a word right (and on a\n' +
      'headline or an item too, as ~org_support_shift_select~ says), and\n' +
      '~<CR>~ in a source block\'s code is Vim\'s own.',
  );
  return sections.join('\n\n');
}

/** The Vim plugin's keys: the Neovim plugin's. */
function vimKeys() {
  return neovimKeys(['global', 'org', 'capture', 'agenda', 'calendar', 'columns']);
}

/** A key of VS Code's `keybindings` (`ctrl+c ctrl+w`) as its UI writes it. */
function vscodeKey(key) {
  const symbols = { '=': '+', ',': '<', '.': '>', '-': '_', '/': '?', ';': ':', "'": '"', '1': '!' };
  const names = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', enter: 'Enter', tab: 'Tab', escape: 'Escape' };
  return key
    .split(' ')
    .map((stroke) => {
      const parts = stroke.split('+');
      // `shift+=` is how VS Code names `+`.
      if (parts.length === 2 && parts[0] === 'shift' && symbols[parts[1]]) return symbols[parts[1]];
      const modified = parts.length > 1;
      return parts
        .map(
          (part) =>
            names[part] ??
            (part.length === 1 ? (modified ? part.toUpperCase() : part) : part[0].toUpperCase() + part.slice(1)),
        )
        .join('+');
    })
    .join(' ');
}

/** The VS Code extension's keys, as its package.json binds them, by preset. */
function vscodeKeys() {
  const where = (when) =>
    when.includes(`'${manifest.agenda.languageId}'`)
      ? 'agenda'
      : manifest.datePrompt && when.includes(`'${manifest.datePrompt.languageId}'`)
        ? 'calendar'
        : when.includes(manifest.capture.prefix)
          ? 'capture'
          : 'org';
  const titles = Object.fromEntries(vscode.contributes.commands.map((command) => [command.command, command.title]));
  const presets = [
    ['emacs', 'Emacs (~"org-lsp.keys": "emacs"~, the default)'],
    ['vim', 'Vim (~"org-lsp.keys": "vim"~)'],
  ];
  const sections = [];
  for (const [preset, presetHeading] of presets) {
    sections.push(heading(2, presetHeading));
    for (const [scope, title] of SCOPES) {
      const byCommand = new Map();
      for (const binding of vscode.contributes.keybindings) {
        if (!binding.when.includes(`keys == '${preset}'`) || where(binding.when) !== scope) continue;
        if (!byCommand.has(binding.command)) byCommand.set(binding.command, []);
        byCommand.get(binding.command).push(code(vscodeKey(binding.key)) + (binding.args?.fallback ? ' ¹' : ''));
      }
      if (byCommand.size === 0) continue;
      const rows = [...byCommand].map(([command, keys]) => [keys.join(', '), code(command), org(titles[command] ?? '')]);
      sections.push(heading(3, title) + '\n\n' + table(['Keys', 'Command', 'What it does'], rows));
    }
  }
  sections.push(
    '¹ Where the command does not apply, the key does what it does without\n' +
      'the extension: ~Shift+Up~ off a timestamp selects (and on a headline\n' +
      'too, as ~org_support_shift_select~ says), ~Tab~ off\n' +
      'a headline, block or drawer indents, ~Enter~ in a source block\'s code\n' +
      'is VS Code\'s own.',
  );
  // VSCodeVim's own key bindings, for the keys VS Code cannot bind: those
  // with the leader. They apply in every file.
  const leader = manifest.commands.flatMap((command) =>
    command.keys
      .filter((key) => key.preset === 'vim' && key.key.startsWith('<Leader>') && ['global', 'org'].includes(key.in))
      // `<CR>` is one key to VSCodeVim, as `<leader>` is.
      .map((key) => ({ before: ['<leader>', ...key.key.slice('<Leader>'.length).match(/<[^>]+>|./g)], commands: [command.command] })),
  );
  const unique = [...new Map(leader.map((entry) => [entry.before.join(''), entry])).values()];
  sections.push(
    heading(3, 'The leader keys, for VSCodeVim') +
      '\n\n' +
      'VS Code cannot bind keys that start with the leader without taking\n' +
      "them from Vim. Add them to VSCodeVim's settings instead (they apply in\n" +
      'every file; outside Org files the commands say they do not apply):\n\n' +
      block('json', [
        '"vim.normalModeKeyBindingsNonRecursive": [',
        ...unique.map((entry, index) => {
          const list = (items) => items.map((item) => JSON.stringify(item)).join(', ');
          const comma = index < unique.length - 1 ? ',' : '';
          return `  { "before": [${list(entry.before)}], "commands": [${list(entry.commands)}] }${comma}`;
        }),
        ']',
      ]),
  );
  const every = [...manifest.commands, ...vscode.contributes.commands.filter((command) => ['org-lsp.cycle', 'org-lsp.cycleGlobal', 'org-lsp.help'].includes(command.command))].map(
    (command) => [code(command.command), org(`Org: ${command.title}`)],
  );
  sections.push(heading(2, 'Every command') + '\n\n' + table(['Command', 'In the Command Palette'], every));
  return sections.join('\n\n');
}

/** A key of Sublime Text's key bindings (`["ctrl+c", "ctrl+t"]`) as its
 * menus write it (`Ctrl+C Ctrl+T`); a stroke that is one character (`+`,
 * `-`) is that character. */
function sublimeKey(keys) {
  const names = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', enter: 'Enter', tab: 'Tab', space: 'Space' };
  return keys
    .map((stroke) =>
      stroke.length === 1
        ? stroke
        : stroke
            .split('+')
            .map((part) => names[part] ?? (part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1)))
            .join('+'),
    )
    .join(' ');
}

/** The Sublime Text package's keys, as its Default.sublime-keymap binds
 * them, by preset (`org_keys`) and by where they apply. */
function sublimeKeys() {
  const dir = join(root, 'editors', 'sublime', 'LSP-org');
  const keymap = JSON.parse(readFileSync(join(dir, 'Default.sublime-keymap'), 'utf8'));
  const palette = JSON.parse(readFileSync(join(dir, 'LSP-org.sublime-commands'), 'utf8'));
  const titles = Object.fromEntries(manifest.commands.map((command) => [command.command, command.title]));
  // The package's own commands, by the caption of the Command Palette.
  for (const entry of palette) {
    if (entry.command !== 'org_lsp_run') titles[entry.command] ??= entry.caption.replace(/^Org: /, '');
  }
  titles.close ??= 'Close the agenda';
  const name = (binding) => (binding.command === 'org_lsp_run' ? binding.args.command : binding.command);
  const context = (binding, key) => binding.context?.find((entry) => entry.key === key)?.operand;
  const where = (binding) =>
    context(binding, 'selector') === 'text.org-agenda'
      ? 'agenda'
      : context(binding, 'org_lsp.capture') === true
        ? 'capture'
        : 'org';
  const presets = [
    ['emacs', 'Emacs (~"org_keys": "emacs"~, the default)'],
    ['vim', 'Vim (~"org_keys": "vim"~, in command mode unless it says otherwise)'],
  ];
  const sections = [];
  for (const [preset, presetHeading] of presets) {
    sections.push(heading(2, presetHeading));
    for (const [scope, title] of SCOPES) {
      const byCommand = new Map();
      for (const binding of keymap) {
        if (context(binding, 'org_lsp.keys') !== preset || where(binding) !== scope) continue;
        const command = name(binding);
        if (!byCommand.has(command)) byCommand.set(command, []);
        const insert = context(binding, 'setting.command_mode') === false ? ' (insert mode)' : '';
        byCommand.get(command).push(code(sublimeKey(binding.keys)) + (binding.args?.fallback ? ' ¹' : '') + insert);
      }
      if (byCommand.size === 0) continue;
      const rows = [...byCommand].map(([command, keys]) => [keys.join(', '), code(command), org(titles[command] ?? '')]);
      sections.push(heading(3, title) + '\n\n' + table(['Keys', 'Command', 'What it does'], rows));
    }
  }
  sections.push(
    '¹ Where the command does not apply, the key does what it does without\n' +
      'the package: ~Shift+Up~ off a timestamp selects (and on a headline\n' +
      'too, as ~org_support_shift_select~ says), ~Tab~ off\n' +
      'a headline, block or drawer indents or completes, ~Enter~ makes a new\n' +
      'line.',
  );
  const bound = new Set(keymap.map(name));
  const keyless = manifest.commands
    .filter((command) => !bound.has(command.command))
    .map((command) => [code(command.command), org(`Org: ${command.title}`)]);
  sections.push(heading(2, 'Without a key') + '\n\n' + table(['Command', 'In the Command Palette'], keyless));
  return sections.join('\n\n');
}

/** The keys of Zed's agenda and Org files: keymap.json binds them to the
 * tasks of tasks.json, each of which runs a command of the server. */
function zedKeys() {
  const dir = join(root, 'editors', 'zed');
  const keymap = JSON.parse(readFileSync(join(dir, 'keymap.json'), 'utf8'));
  const tasks = JSON.parse(readFileSync(join(dir, 'tasks.json'), 'utf8'));
  // The command a task runs: the word after the script, or `key:KEY`.
  const commandOf = Object.fromEntries(
    tasks.map((task) => [task.label, /agenda-command ('[^']+'|\S+)/.exec(task.command)[1].replaceAll("'", '').replace(/^file:/, '')]),
  );
  const titles = Object.fromEntries(manifest.commands.map((command) => [command.command, command.title]));
  // What a key's own task does: one command in a capture buffer, another
  // in an Org file.
  for (const id of Object.values(commandOf).filter((id) => id.startsWith('key:'))) {
    const key = id.slice('key:'.length);
    const by = (scope) =>
      manifest.commands.find((command) => command.keys.some((bound) => bound.preset === 'emacs' && bound.in === scope && bound.key === key));
    const what = (scope) => by(scope).title.replace(/^Capture: /, '').toLowerCase();
    titles[id] =
      what('capture') === what('org')
        ? by('org').title
        : `In a capture buffer: ${what('capture')}; elsewhere: ${what('org')}`;
  }
  const zedActions = {
    'pane::CloseActiveItem': 'Close the agenda',
    'editor::GoToDefinition': 'Go to the entry of the line',
    'editor::GoToDefinitionSplit': 'Show the entry of the line in a split',
  };
  const headings = { 'extension == org-agenda': 'In the agenda', 'extension == org': 'In Org files' };
  return keymap
    .map((bindings) => {
      const title = Object.entries(headings).find(([context]) => bindings.context.endsWith(context))?.[1] ?? bindings.context;
      const byAction = new Map();
      for (const [key, action] of Object.entries(bindings.bindings)) {
        const [command, title] = Array.isArray(action)
          ? [commandOf[action[1].task_name], titles[commandOf[action[1].task_name]]]
          : [action, zedActions[action]];
        if (!byAction.has(command)) byAction.set(command, { keys: [], title });
        byAction.get(command).keys.push(code(key));
      }
      const rows = [...byAction].map(([command, { keys, title: what }]) => [keys.join(', '), code(command), org(what ?? '')]);
      return heading(2, title) + '\n\n' + table(['Keys', 'Command or Zed action', 'What it does'], rows);
    })
    .join('\n\n');
}

// ----- options

/**
 * The options as a tree, in the table's order: `{ name, setting?, children? }`.
 * The options only an editor gives (`config_file`) are left out for the
 * shared configuration file.
 */
function optionTree({ editor = true } = {}) {
  const top = { children: [] };
  for (const setting of settings.filter((setting) => !setting.integration && (editor || !setting.editorOnly))) {
    let node = top;
    const parts = setting.name.split('.');
    for (const [index, part] of parts.entries()) {
      let child = node.children.find((other) => other.name === part);
      if (!child) {
        child = { name: part, children: [] };
        node.children.push(child);
      }
      if (index === parts.length - 1) child.setting = setting;
      node = child;
    }
  }
  return top;
}

// These keys read better first, in this order; the others keep the
// server's order (`org_agenda_sorting_strategy` lists the views as Org does).
const FIRST = ['key', 'description', 'type', 'target', 'template', 'files'];
const entries = (object) =>
  Object.entries(object).sort(([a], [b]) => {
    const rank = (key) => (FIRST.includes(key) ? FIRST.indexOf(key) : FIRST.length);
    return rank(a) - rank(b);
  });

function json(value) {
  if (Array.isArray(value)) return value.length === 0 ? '[]' : `[${value.map(json).join(', ')}]`;
  if (value && typeof value === 'object') {
    const fields = entries(value);
    return fields.length === 0 ? '{}' : `{ ${fields.map(([key, item]) => `${JSON.stringify(key)}: ${json(item)}`).join(', ')} }`;
  }
  return JSON.stringify(value);
}

/** `value` in TOML, or nothing for `null`, which TOML has no word for. */
function toml(value) {
  if (value === null) return undefined;
  if (Array.isArray(value)) {
    const items = value.map(toml);
    return items.includes(undefined) ? undefined : `[${items.join(', ')}]`;
  }
  if (value && typeof value === 'object') {
    const fields = entries(value).map(([key, item]) => [/^[A-Za-z0-9_-]+$/.test(key) ? key : JSON.stringify(key), toml(item)]);
    if (fields.some(([, item]) => item === undefined)) return undefined;
    return fields.length === 0 ? '{}' : `{ ${fields.map(([key, item]) => `${key} = ${item}`).join(', ')} }`;
  }
  return JSON.stringify(value);
}

/**
 * The groups of the options on docs/configuration.org, in the page's order:
 * an option goes in the first whose pattern its name matches, and in the
 * first of the last two, which take the rest, when none does.
 */
const OPTION_GROUPS = [
  ['TODO keywords and logging', /^org_(todo|log|enforce_todo|treat_|closed_keep|provide_todo|hierarchical_todo|use_fast_todo|track_ordered)/],
  ['Priorities', /^org_priority/],
  ['Tags and properties', /^org_(tag|tags|use_tag|use_property|property|global_properties|group_tags|use_fast_tag|fast_tag|auto_align_tags|complete_tags)/],
  ['Lists', /^org_(list|plain_list|checkbox)/],
  ['Tables', /^org_(table|calc)/],
  ['Links, IDs and citations', /^org_(link|id|file_apps|cite|bibtex|return_follows_link)/],
  ['Dates and times', /^org_(time_stamp|read_date|edit_timestamp|extend_today|use_effective|use_last_clock|duration|display_custom|timestamp_custom)|^calendar_/],
  ['Capture, refile and archive', /^org_(capture|directory|default_notes|refile|reverse_note_order|archive)/],
  ['org-protocol', /^org_protocol_/],
  ['Clocking', /^org_clock|^org_agenda_clock/],
  ['Habits', /^org_habit_|^org_modules$/],
  ['The agenda', /^org_agenda_|^agenda_|^org_(deadline|scheduled)_|^org_stuck_projects$|^org_tags_match_list_sublevels$/],
  ['Notifications', /^appt_|^notification_/],
  [
    'Display',
    /^org_(hide|fontify|emphasis|hidden_keywords|entities|pretty|use_sub|image|startup|adapt_indentation|indent|ellipsis|bullets|cycle_hide|cycle_open)/,
  ],
  ['Source blocks and tangling', /^org_(src|babel)/],
  ['org-super-agenda', /^org_super_agenda_/],
  ['org-roam', /^org_roam_/],
  ['org-journal', /^org_journal_/],
  ['Editing and motion', /^org_|^return_uses/],
  ['Emacs and the server', /./],
];
const FALLBACK = OPTION_GROUPS.slice(-2);

/** The options of the shared file by group, in the page's order. */
function optionGroups() {
  const groups = new Map(OPTION_GROUPS.map(([name]) => [name, []]));
  for (const setting of settings.filter((setting) => !setting.integration && !setting.editorOnly)) {
    const matches = ([, pattern]) => pattern.test(setting.name);
    const [name] = OPTION_GROUPS.slice(0, -2).find(matches) ?? FALLBACK.find(matches);
    groups.get(name).push(setting);
  }
  return groups;
}

/** `text` as comment lines of at most 78 columns. */
function comment(text) {
  const lines = [];
  let line = '';
  for (const word of text.split(/\s+/).filter(Boolean)) {
    if (line && 2 + line.length + 1 + word.length > 78) {
      lines.push(line);
      line = word;
    } else {
      line = line ? `${line} ${word}` : word;
    }
  }
  if (line) lines.push(line);
  return lines.map((text) => `# ${text}`);
}

/** docs/config.toml: the shared file with every option at its default,
 * each with what it means, in the groups of docs/configuration.org. */
function configToml() {
  const lines = [
    '#:schema https://raw.githubusercontent.com/kristijanhusak/org-lsp/main/docs/config.schema.json',
    '',
    ...comment(
      "The shared configuration file of org-lsp with every option at its default. Written by scripts/generate-docs.mjs from `org-lsp settings`: edit server/org-lsp/src/settings.rs, not this file.",
    ),
    '#',
    ...comment(
      'Copy it to ~/.config/org-lsp/config.toml and change what you want different; the options left as they are can as well go. An option that has no default here (it depends on something else) is a comment. docs/configuration.org says more about each.',
    ),
  ];
  for (const [name, group] of optionGroups()) {
    if (group.length === 0) continue;
    lines.push('', '', `# ----- ${name}`);
    for (const setting of group) {
      lines.push('', ...comment(setting.org ? `${setting.doc} (${setting.org})` : setting.doc));
      const value = setting.default === undefined ? undefined : toml(setting.default);
      if (value !== undefined) lines.push(`${setting.name} = ${value}`);
      else if (setting.default === undefined) lines.push(...comment(`${setting.name} = …  (unset: ${setting.unset})`));
      else lines.push(...comment(`${setting.name}: ${json(setting.default)} by default, which TOML has no word for`));
    }
  }
  return lines.join('\n') + '\n';
}

/** `text` (the server's, with code in backticks) as Org lines of at most 78
 * columns, a code span never broken over two; `convert` makes it Org. */
function filled(text, indent = '', convert = org) {
  const kept = text.replace(/`[^`]+`/g, (span) => span.replaceAll(' ', '\u0001'));
  const lines = [];
  let line = '';
  for (const word of convert(kept).split(/\s+/).filter(Boolean)) {
    if (line && indent.length + line.length + 1 + word.length > 78) {
      lines.push(line);
      line = word;
    } else {
      line = line ? `${line} ${word}` : word;
    }
  }
  if (line) lines.push(line);
  return lines.map((line, index) => (index > 0 ? indent : '') + line.replaceAll('\u0001', ' ')).join('\n');
}

/** What an option means, as Org on a page of docs/: a page of docs/ it
 * names (`docs/parity.org`) is a link to it. */
const optionDoc = (text) =>
  org(text).replace(/~docs\/([\w-]+\.org)~/g, (_, file) => link(`file:${file}`, `docs/${file}`));

/** The options on docs/configuration.org: a heading for each, under the
 * heading of its group, with what it means, its type and its default. */
function fileOptions() {
  const groups = optionGroups();
  const lines = [];
  for (const [name, group] of groups) {
    if (group.length === 0) continue;
    lines.push(heading(2, name), ':PROPERTIES:', `:CUSTOM_ID: ${slug(name)}`, ':END:', '');
    for (const setting of group) {
      // GitHub's anchor of the heading, which keeps `_`.
      lines.push(heading(3, setting.name), ':PROPERTIES:', `:CUSTOM_ID: ${setting.name.toLowerCase()}`, ':END:', '');
      lines.push(filled(setting.doc, '', optionDoc), '');
      if (setting.org) lines.push(`- Org's option :: ${code(setting.org)}`);
      lines.push(`- Type :: ${org(setting.type)}`);
      if (setting.default === undefined) {
        lines.push(filled(`- Default :: unset: ${setting.unset}`, '  '), '');
      } else {
        lines.push(`- Default :: ${code(json(setting.default))}`, '');
        const value = toml(setting.default);
        if (value !== undefined) lines.push(block('toml', [`${setting.name} = ${value}`]), '');
      }
    }
  }
  while (lines[lines.length - 1] === '') lines.pop();
  return lines.join('\n');
}

/** The options in docs/protocol.org: a description list, each option with
 * its type, its default and what it means. */
function protocolOptions() {
  return settings
    .map((setting) => {
      const fallback = setting.default === undefined ? org(setting.unset) : code(json(setting.default));
      const meaning = optionDoc(setting.doc) + (setting.org ? ` (${code(setting.org)})` : '');
      return `- ${code(setting.name)} :: ${setting.type}, ${fallback}. ${meaning}`;
    })
    .join('\n');
}

// ----- types

/** `value` as a Lua literal. */
const luaLiteral = (value) => (typeof value === 'string' ? `'${value}'` : String(value));

/** The Lua type of what `schema` allows; classes it names go into `classes`. */
function luaType(schema, classes) {
  if (schema.anyOf) return [...new Set(schema.anyOf.map((choice) => luaType(choice, classes)))].join('|');
  if ('const' in schema) return luaLiteral(schema.const);
  if (schema.enum) return schema.enum.map(luaLiteral).join('|');
  const types = Array.isArray(schema.type) ? schema.type : [schema.type];
  return types
    .map((type) => {
      switch (type) {
        case 'array': {
          const item = luaType(schema.items ?? {}, classes);
          return item.includes('|') ? `(${item})[]` : `${item}[]`;
        }
        case 'object': {
          const name = schema['x-lua-class'] ?? 'table';
          if (name !== 'table') classes.set(name, schema);
          return name;
        }
        case undefined:
          return 'any';
        default:
          return type;
      }
    })
    .join('|');
}

/** `text` on one line, as a field's description. */
const oneLine = (text) => text.replace(/\s+/g, ' ');

/** lua/org-lsp/types.lua: the classes of `settings`, for lua-language-server. */
function luaTypes() {
  const classes = new Map();
  const blocks = [];
  // The option groups, from the tree of the options.
  const group = (node, name, description) => {
    const lines = [`--- ${description}`, `---@class ${name}`];
    const nested = [];
    for (const child of node.children) {
      if (child.setting) {
        const type = luaType(child.setting.schema ?? {}, classes);
        lines.push(`---@field ${child.name}? ${type} ${oneLine(describe(child.setting))}`);
      } else {
        const childName = `${name}.${child.name[0].toUpperCase()}${child.name.slice(1)}`;
        lines.push(`---@field ${child.name}? ${childName}`);
        nested.push([child, childName]);
      }
    }
    blocks.push(lines.join('\n'));
    for (const [child, childName] of nested) group(child, childName, `The options under \`${child.name}\`.`);
  };
  group(optionTree(), 'org_lsp.Settings', "The server's options, as `settings` passes them; see docs/configuration.org.");
  for (const [name, schema] of classes) {
    const required = new Set(schema.required ?? []);
    const lines = [`---@class ${name}`];
    for (const [field, property] of Object.entries(schema.properties ?? {})) {
      const optional = required.has(field) ? '' : '?';
      lines.push(`---@field ${field}${optional} ${luaType(property, classes)} ${oneLine(property.description ?? '')}`.trimEnd());
    }
    blocks.push(lines.join('\n'));
  }
  return [
    '---@meta',
    '-- The types of the server\'s options, for lua-language-server. Written by',
    '-- scripts/generate-docs.mjs from `org-lsp settings`; edit',
    '-- server/org-lsp/src/settings.rs, not this file.',
    '',
    blocks.join('\n\n'),
    '',
  ].join('\n');
}

/** docs/config.schema.json: the JSON Schema of the shared configuration file. */
function configSchema() {
  const schema = {
    $schema: 'http://json-schema.org/draft-07/schema#',
    $id: 'https://raw.githubusercontent.com/kristijanhusak/org-lsp/main/docs/config.schema.json',
    title: 'org-lsp configuration',
    description: "The options of the org-lsp server, in its shared configuration file. See docs/configuration.org.",
    type: 'object',
    properties: { $schema: { type: 'string' }, ...schemaProperties(settings, { editor: false }) },
    additionalProperties: false,
  };
  // Typos in nested groups are caught too.
  const close = (properties) => {
    for (const property of Object.values(properties)) {
      if (property.type === 'object' && property.properties) {
        property.additionalProperties = false;
        close(property.properties);
      }
    }
  };
  close(schema.properties);
  return JSON.stringify(schema, null, 2) + '\n';
}


// ----- features, parity and the manual tests (scripts/features.mjs)

const MARK = { yes: '✅', part: '❓', no: '❌', none: '—' };
/** What the marks of a feature table mean; `parity` is the link to docs/parity.org. */
const legend = (parity, why = "the editor's page says what and why") =>
  `✅ all that org-lsp does works there, ❓ some does not (${why}),\n` +
  '❌ none does, — nothing to support yet. Parity is the share of what\n' +
  `Emacs Org does in that area that org-lsp does; ${link(parity, 'parity.org')} lists both.`;

/** The anchor GitHub gives a heading, and the CUSTOM_ID it has in
 * docs/parity.org, so that a link finds it in Emacs and on GitHub. */
const slug = (text) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9 -]/g, '')
    .replaceAll(' ', '-');

const commandsById = Object.fromEntries(manifest.commands.map((command) => [command.command, command]));

/** An Emacs key (`C-c C-x M-w`, `S-<up>`) as VS Code and Sublime Text write it. */
function emacsKeyName(key) {
  const modifiers = { C: 'Ctrl', M: 'Alt', S: 'Shift' };
  const bases = { RET: 'Enter', TAB: 'Tab', SPC: 'Space', '<up>': 'Up', '<down>': 'Down', '<left>': 'Left', '<right>': 'Right' };
  return key
    .split(' ')
    .map((stroke) => {
      const parts = [];
      let rest = stroke;
      while (/^[CMS]-./.test(rest)) {
        parts.push(modifiers[rest[0]]);
        rest = rest.slice(2);
      }
      const base = bases[rest] ?? (parts.length > 0 && rest.length === 1 ? rest.toUpperCase() : rest);
      return [...parts, base].join('+');
    })
    .join(' ');
}

/** Code in running text. */
const inline = code;

/**
 * The commands a client without keys for a server's commands finds among
 * its code actions (Helix, and Zed with `command_actions`), by where the
 * cursor is: as the server offers them (`code_actions_at` in
 * server/org-lsp/src/server.rs, the kinds of server/org-lsp/src/actions.rs).
 * Keep them in step.
 */
const ACTIONS_IN_FILES = new Set([
  // actions.rs `KINDS`
  'org.todo.next', 'org.todo.previous', 'org.priority.up', 'org.priority.down', 'org.timestamp.up',
  'org.timestamp.down', 'org.date.later', 'org.date.earlier', 'org.clock.in', 'org.clock.out', 'org.clock.cancel',
  'org.checkbox.toggle', 'org.table.recalculate',
  // On a headline, an item or in a table.
  'org.meta.left', 'org.meta.right', 'org.shiftmeta.left', 'org.shiftmeta.right', 'org.meta.up', 'org.meta.down',
  'org.shiftmeta.up', 'org.shiftmeta.down', 'org.table.insert.row.below', 'org.table.hline', 'org.table.hline.above',
  'org.table.blank.field',
  // As commands.
  'org.babel.tangle', 'org.table.toggle.column.width', 'org.babel.execute', 'org.edit.special', 'org.archive.subtree',
  'org.archive.sibling', 'org.archive.tag', 'org.todo', 'org.priority', 'org.refile', 'org.schedule', 'org.deadline',
  'org.tags.set', 'org.property.set', 'org.effort.set', 'org.sort', 'org.clock.goto', 'org.clock.in.last',
  'org.clock.report', 'org.indent.mode', 'org.dblock.update', 'org.agenda.list', 'org.agenda.todos', 'org.agenda.dispatch', 'org.capture',
  'org.meta.return', 'org.insert.todo.heading', 'org.insert.heading.after', 'org.insert.todo.heading.after',
  'org.cut.subtree', 'org.copy.subtree', 'org.paste.subtree',
]);
/** In a capture buffer. */
const ACTIONS_IN_CAPTURE = new Set(['org.capture.finish', 'org.capture.abort', 'org.capture.refile']);
/** On an agenda line (not in Zed, whose agenda runs its keys through
 * tasks): every command of the agenda's but those that need an editor's
 * code. */
const IN_AGENDA_TOO = [
  'org.todo.yesterday',
  'org.tags.set',
  'org.schedule.remove',
  'org.deadline.remove',
  'org.schedule.delay',
  'org.deadline.warning',
];
const ACTIONS_IN_AGENDA = new Set([
  ...manifest.commands
    .filter(
      (command) =>
        IN_AGENDA_TOO.includes(command.command) || command.keys.some((key) => key.preset === 'emacs' && key.in === 'agenda'),
    )
    .map((command) => command.command)
    .filter((id) => !['org.agenda.follow', 'org.agenda.columns', 'org.agenda.goto.other'].includes(id)),
  'org.agenda.dispatch',
]);

/** Whether the command `id` is a code action in `editor` where `scope`
 * says. */
function isAction(id, scope, editor) {
  // Nor saving the agenda's files for Helix, which does nothing with the
  // files a result names (`saves` in `code_actions_at`).
  if (scope === 'agenda') return editor !== 'zed' && ACTIONS_IN_AGENDA.has(id) && !(editor === 'helix' && id === 'org.agenda.save');
  if (scope === 'capture') return ACTIONS_IN_CAPTURE.has(id);
  return ACTIONS_IN_FILES.has(id);
}

/** Commands that are a request of the protocol's own too, by editor. */
const OWN_ROUTES = {
  helix: { 'org.open': '`gd`' },
  // In Zed's agenda, `enter` and `tab` are go to definition (keymap.json).
  zed: { 'org.open': '`F12`', 'org.agenda.goto': '`enter`', 'org.agenda.goto.other': '`tab`' },
};

/**
 * Zed's keys of each command, in Org files (`org`) and in the agenda,
 * from the keymap and tasks editors/zed/generate.mjs writes: a key runs a
 * task, the task the command (`key:KEY`: the command the key has where
 * the cursor is).
 */
const zedKeysOf = (() => {
  const dir = join(root, 'editors', 'zed');
  const keymap = JSON.parse(readFileSync(join(dir, 'keymap.json'), 'utf8'));
  const tasks = JSON.parse(readFileSync(join(dir, 'tasks.json'), 'utf8'));
  const runs = Object.fromEntries(
    tasks.map((task) => [task.label, /agenda-command ('[^']+'|\S+)/.exec(task.command)[1].replaceAll("'", '').replace(/^file:/, '')]),
  );
  const found = { org: new Map(), agenda: new Map() };
  for (const bindings of keymap) {
    const scope = bindings.context.endsWith('extension == org-agenda') ? 'agenda' : 'org';
    for (const [key, action] of Object.entries(bindings.bindings)) {
      if (!Array.isArray(action)) continue;
      const run = runs[action[1].task_name];
      const ids = run.startsWith('key:')
        ? manifest.commands
            .filter((command) => command.keys.some((bound) => bound.preset === 'emacs' && bound.key === run.slice('key:'.length)))
            .map((command) => command.command)
        : [run];
      for (const id of ids) {
        if (!found[scope].has(id)) found[scope].set(id, []);
        found[scope].get(id).push(key);
      }
    }
  }
  return (id, scope) => found[scope === 'agenda' ? 'agenda' : 'org'].get(id) ?? [];
})();

/** How a command is run in `editor`, from where `scope` says; nothing
 * where it cannot be. */
function commandRoute(editor, id, scope = 'org') {
  const command = commandsById[id];
  if (!command) throw new Error(`scripts/features.mjs names an unknown command: ${id}`);
  const keys = (preset) =>
    command.keys.filter((key) => key.preset === preset && (key.in === scope || key.in === 'global')).map((key) => key.key);
  switch (editor) {
    case 'neovim':
    case 'vim': {
      const found = keys('vim');
      return found.length > 0 ? found.map(inline).join(' or ') : inline(':Org ' + id.replace(/^org\./, '').replaceAll('.', ' '));
    }
    case 'vscode':
    case 'sublime': {
      const found = keys('emacs');
      return found.length > 0 ? found.map((key) => inline(emacsKeyName(key))).join(' or ') : `/Org: ${org(command.title)}/`;
    }
    default: {
      // Zed: its keys (tasks), else the code actions; Helix: the code
      // actions.
      const found = editor === 'zed' ? zedKeysOf(id, scope) : [];
      if (found.length > 0) return found.map(inline).join(' or ');
      if (isAction(id, scope, editor)) return `the code action /${org(command.title)}/`;
      return OWN_ROUTES[editor]?.[id] ? org(OWN_ROUTES[editor][id]) : '';
    }
  }
}

/** Whether `check` applies to `editor`. */
const applies = (check, editor) => (!check.only || check.only.includes(editor)) && !check.not?.includes(editor);

/** The keys of a check in `editor`, or nothing; `missing`, the titles of
 * its commands that cannot be run there. */
function checkKeys(check, editor) {
  if (check.keys) return { keys: org(check.keys[editor] ?? ''), missing: [] };
  if (!check.cmd) return { keys: '', missing: [] };
  const ids = Array.isArray(check.cmd) ? check.cmd : [check.cmd];
  const routes = ids.map((id) => [id, commandRoute(editor, id, check.in)]);
  return {
    keys: routes
      .map(([, route]) => route)
      .filter(Boolean)
      .join(', '),
    missing: routes.filter(([, route]) => !route).map(([id]) => `/${org(commandsById[id].title)}/`),
  };
}

/** Every area and every editor: README.org's table. */
function featureTable() {
  const parityFile = 'file:docs/parity.org';
  const header = ['Feature', 'Parity', EDITORS.map((editor) => editor.name).join(' / ')];
  const rows = AREAS.map((area) => [
    link(`${parityFile}::#${slug(area.name)}`, area.name),
    `${parity(area).percent}%`,
    EDITORS.map((editor) => MARK[support(area, editor.id)[0]]).join(' / '),
  ]);
  rows.push(['*All of Org*', `*${overall()}%*`, '']);
  return table(header, rows) + '\n\n' + legend(parityFile);
}

/** Every area in one editor, with what does not work there: docs/<editor>.org's table. */
function editorFeatureTable(id) {
  const editor = EDITORS.find((one) => one.id === id);
  const rows = AREAS.map((area) => {
    const [status, why] = support(area, id);
    return [link(`file:parity.org::#${slug(area.name)}`, area.name), `${parity(area).percent}%`, MARK[status], org(why)];
  });
  rows.push(['*All of Org*', `*${overall()}%*`, '', '']);
  return (
    table(['Feature', 'Parity', editor.name, 'What does not work, and why'], rows) +
    '\n\n' +
    legend('file:parity.org', 'the last column says what and why') +
    '\n\n' +
    `${link(`file:../${editor.dir}/TESTING.org`, 'TESTING.org')} lists what to try by hand.`
  );
}

function parityPage() {
  const lines = [
    '#+title: Parity with Emacs Org',
    '#+options: ^:{}',
    '',
    '# Written by scripts/generate-docs.mjs from scripts/features.mjs.',
    '',
    'What Emacs Org (9.8.10) does, area by area, and what org-lsp does of it.',
    'An area\'s parity is the share of its features done; all of Org is the',
    'mean of the areas. Every feature done is checked against Emacs where Org',
    `decides the outcome (see "How the work is checked" in ${link('file:development.org::#how-the-work-is-checked', 'development.org')}).`,
    '',
    `*All of Org: ${overall()}%*`,
    '',
    `Org's options, one by one, are under ${link('#options', 'Options')}.`,
    '',
    table(
      ['Feature', 'Parity', 'Done'],
      AREAS.map((area) => {
        const { done, total, percent } = parity(area);
        return [link(`#${slug(area.name)}`, area.name), `${percent}%`, `${done} of ${total}`];
      }),
    ),
  ];
  for (const area of AREAS) {
    const { done, total, percent } = parity(area);
    lines.push('', heading(1, area.name), ':PROPERTIES:', `:CUSTOM_ID: ${slug(area.name)}`, ':END:', '');
    lines.push(`${percent}% (${done} of ${total})`, '');
    for (const [text, state, of] of area.emacs) {
      const mark = typeof state === 'number' ? (state === of ? '✅' : '❓') : state ? '✅' : '❌';
      lines.push(`- ${mark} ${org(text)}${typeof state === 'number' ? ` (${state} of ${of})` : ''}`);
    }
    const short = EDITORS.map((editor) => [editor, ...support(area, editor.id)]).filter(([, status]) => status === 'part' || status === 'no');
    if (short.length > 0) {
      lines.push('', 'Where editors fall short:', '');
      for (const [editor, status, why] of short) lines.push(`- ${editor.name} ${MARK[status]}: ${org(why)}`);
    }
  }
  return lines.join('\n') + '\n' + optionsSection();
}

/** Every option of Org and what org-lsp does with it: how many there are
 * of each status, and those that are missing, file by file. */
function optionsSection() {
  const all = Object.entries(OPTIONS)
    .filter(([file]) => !EMACS_FILES.has(file))
    .flatMap(([file, options]) => Object.entries(options).map(([name, value]) => ({ file, name, value })));
  const count = (status) => all.filter((option) => statusOf(option.value) === status).length;
  const settable = count('setting') + count('missing') + count('other');
  const lines = [
    '',
    heading(1, 'Options'),
    ':PROPERTIES:',
    ':CUSTOM_ID: options',
    ':END:',
    '',
    `Each of the ${all.length} options of Org 9.8.10 (its ~defcustom~ forms), and what org-lsp does with`,
    `it. Of the ${settable} that change what org-lsp does itself, ${Math.round((100 * (count('setting') + count('other'))) / settable)}% can be set.`,
    `${link('file:../scripts/org-options.json', 'scripts/org-options.json')} says it for each; ~scripts/check.sh~ fails when the`,
    'server has a setting that file does not know of, or, with ~ORG_LISP_DIR~,',
    'when Org has an option it does not list.',
    '',
    table(
      ['Status', 'Options', 'Meaning'],
      Object.entries(STATUSES).map(([status, meaning]) => [status, String(count(status)), meaning]),
    ),
  ];
  const other = all.filter((option) => statusOf(option.value) === 'other');
  if (other.length > 0) {
    lines.push('', heading(2, 'Done another way'), '');
    for (const option of other) lines.push(`- ~${option.name}~ :: ${org(option.value[1])}`);
  }
  lines.push('', heading(2, 'Missing'), '', 'Options of what org-lsp does that cannot be set yet, by the file of Org they are in.', '');
  const files = [...new Set(all.filter((option) => statusOf(option.value) === 'missing').map((option) => option.file))];
  for (const file of files) {
    const names = all.filter((option) => option.file === file && statusOf(option.value) === 'missing').map((option) => `~${option.name}~`);
    lines.push(`- ${file} (${names.length}) :: ${names.join(', ')}`);
  }
  return lines.join('\n') + '\n';
}

// ----- the options of emacs_options (scripts/emacs-options.json)

/** A word of a docstring that Org would read as more than text (a link,
 * a footnote, emphasis, a LaTeX fragment or an entity, a macro, an export
 * snippet, inline code), as code; any other word as it is. */
function docWord(word) {
  const [, open, body, close] = word.match(/^([("']*)(.*?)([)"'.,;:!?]*)$/s);
  if (!body) return word;
  const markup =
    /\[\[|\]\]|\[fn:|@@|\{\{\{|<<|^\\|^(src|call)_/.test(body) ||
    (/^[*/_=~+]/.test(body) && body.slice(1).includes(body[0])) ||
    /^[~=]/.test(body);
  return markup ? open + code(body) + close : word;
}

/** A line of a docstring's text as Org: `foo' is ~foo~, and what Org
 * would read as markup is code. */
function docLine(line) {
  const parts = line.split(/(`[^`'\n]+')/);
  return parts
    .map((part, index) => {
      if (index % 2 === 1) {
        // As `org` does, a zero-width space where Org would not see it.
        const before = parts[index - 1].slice(-1);
        const after = parts[index + 1]?.[0] ?? '';
        const open = before === '' || PRE.test(before) ? '' : '​';
        const close = after === '' || POST.test(after) ? '' : '​';
        return open + code(part.slice(1, -1)) + close;
      }
      return part.replace(/[^\s]+/g, (word) => (word.includes('`') ? word : docWord(word)));
    })
    .join('');
}

/** Whether a line of a docstring is text rather than laid out by hand
 * (indented, a table of values) or what Org would read as more than a
 * line of text (a heading, a comment or keyword, a table, a drawer, a
 * footnote, a rule, a LaTeX environment) or a line of a template (`%t
 * is the title.`), which goes in an example block as it is. */
const docProse = (line) =>
  !/^(\s|\*|#|\||:|\[fn:|-{5}|\\begin\{|%|<-|\.\.\.)/.test(line) && !/^[^\s.:,;]+ {2,}\S/.test(line) && !/^\S+\s*:\s{2,}/.test(line);

/** An option's docstring as Org: its text with Org's markup, and the
 * lines laid out by hand as they are, in example blocks. */
function docOrg(doc) {
  const lines = doc.replace(/\s+$/, '').split('\n');
  const out = [];
  let literal = [];
  const flush = () => {
    while (literal.length > 0 && literal[literal.length - 1].trim() === '') literal.pop();
    if (literal.length === 0) return;
    const indent = Math.min(...literal.filter((line) => line.trim()).map((line) => line.match(/^ */)[0].length));
    const kept = literal.map((line) => line.slice(indent).replace(/^(\s*)(\*|#\+|,\*|,#\+)/, '$1,$2'));
    if (out.length > 0 && out[out.length - 1] !== '') out.push('');
    out.push('#+begin_example', ...kept, '#+end_example', '');
    literal = [];
  };
  let item = false;
  for (const raw of lines) {
    const line = raw.replace(/\t/g, '        ');
    if (line.trim() === '') {
      if (literal.length > 0) literal.push('');
      else if (out.length > 0 && out[out.length - 1] !== '') out.push('');
      item = false;
      continue;
    }
    // A line going on with an item of a list (`- `), indented under it.
    const more = item && literal.length === 0 && /^ {2,}\S/.test(line) && docProse(line.trim());
    if (more || (docProse(line) && !/^\s/.test(line))) {
      flush();
      if (!more) item = line.startsWith('- ');
      out.push(more ? `  ${docLine(line.trim())}` : docLine(line));
    } else {
      literal.push(line);
      item = false;
    }
  }
  flush();
  while (out[out.length - 1] === '') out.pop();
  return out.join('\n');
}

/** Lisp printed by Emacs, as an item's text when it is short, else as a
 * block under the item, of `most` lines at most. */
function lispItem(name, text, most) {
  if (!text.includes('\n') && text.length <= 60) return `- ${name} :: ${code(text)}`;
  let shown = text.split('\n');
  if (shown.length > most) shown = [...shown.slice(0, most - 1), '…'];
  const block = ['#+begin_src emacs-lisp', ...shown.map((line) => line.replace(/^(\s*)(\*|#\+|,\*|,#\+)/, '$1,$2')), '#+end_src'];
  return [`- ${name} ::`, ...block.map((line) => `  ${line}`)].join('\n');
}

/** The id of the option heading `name`, as GitHub makes it (lower case,
 * no punctuation but `-` and `_`), `-1`, `-2`… after the first that
 * `seen` has. */
function githubId(name, seen) {
  const base = name.toLowerCase().replace(/[^\p{L}\p{N}_ -]/gu, '').replaceAll(' ', '-');
  const count = seen.get(base) ?? 0;
  seen.set(base, count + 1);
  return count === 0 ? base : `${base}-${count}`;
}

/** The headings of docs/emacs-options.org: the ids of its options. */
function emacsOptionIds() {
  const seen = new Map();
  const ids = new Map();
  for (const [file, options] of Object.entries(EMACS_OPTIONS)) {
    githubId(file, seen);
    for (const name of Object.keys(options)) ids.set(name, githubId(name, seen));
  }
  return ids;
}

/** Each option of `emacs_options`, under the file of Org it is in: Org's
 * documentation of it, its default, its type, the version of Org it came
 * in, and its default in the shared file, when TOML can say it. */
function emacsOptions() {
  const ids = emacsOptionIds();
  const count = Object.values(EMACS_OPTIONS).reduce((sum, options) => sum + Object.keys(options).length, 0);
  const lines = [
    `The ${count} options, by the file of Org they are in. Each has Org's own`,
    'documentation of it, as ~C-h v~ shows it in Emacs, then its default as',
    'Emacs prints it, its type as Customize has it, and its default in the',
    'shared file, where TOML can say it.',
  ];
  for (const [file, options] of Object.entries(EMACS_OPTIONS)) {
    if (lines[lines.length - 1] !== '') lines.push('');
    lines.push(heading(2, file));
    for (const [name, entry] of Object.entries(options)) {
      if (lines[lines.length - 1] !== '') lines.push('');
      lines.push(heading(3, name), ':PROPERTIES:', `:CUSTOM_ID: ${ids.get(name)}`, ':END:', '');
      lines.push(docOrg(entry.doc), '');
      if (entry.default !== undefined) lines.push(lispItem('Default', entry.default, 20));
      else lines.push(lispItem('Default', entry.form, 20).replace(' ::', ' :: computed when Org loads:'));
      lines.push(lispItem('Type', entry.type, 12));
      if (entry.version) lines.push(`- Since :: ${entry.version}`);
      lines.push('');
      const value = entry.json === undefined ? undefined : toml(entry.json);
      if (value !== undefined) {
        const key = /^[A-Za-z0-9_-]+$/.test(name) ? name : JSON.stringify(name);
        lines.push(block('toml', ['[emacs_options]', `${key} = ${value}`]), '');
      }
    }
  }
  while (lines[lines.length - 1] === '') lines.pop();
  return lines.join('\n');
}

/** The manual tests of `id`, with the boxes ticked in `before` kept. */
function testingPage(id, before) {
  const editor = EDITORS.find((one) => one.id === id);
  const ticked = new Set(
    before
      .split('\n')
      .filter((line) => /^- \[[xX]\] /.test(line))
      .map((line) => line.slice(6)),
  );
  const box = (text) => `- [${ticked.has(text) ? 'X' : ' '}] ${text}`;
  const lines = [
    `#+title: Testing org-lsp in ${editor.name} by hand`,
    '#+options: ^:{}',
    '',
    '# Written by scripts/generate-docs.mjs from scripts/features.mjs.',
    '# Ticked boxes are kept when it is written again.',
    '',
    `What to try in ${editor.name} to know that everything org-lsp does works`,
    'there. The keys are the defaults. Tick a box once it works; a check that',
    `fails is a bug to report. ${link(`file:../../docs/${id}.org`, `docs/${id}.org`)} says what is left out`,
    'here and why.',
    '',
    'To look at most of what a file shows at once (highlighting, concealing,',
    'entities, folding when it opens, indentation, shrunk columns,',
    `diagnostics), open ${link('file:../../tests/visual.org', 'tests/visual.org')}: each of`,
    'its sections says what it should look like.',
    '',
    heading(1, 'Setup'),
    '',
    ...editor.setup.map((text) => box(org(text))),
  ];
  for (const area of AREAS) {
    const [status, why] = support(area, id);
    if (status === 'none') continue;
    lines.push('', heading(1, area.name), '');
    if (status === 'no') {
      lines.push(`*Not in ${editor.name}:* ${org(why)}`);
      continue;
    }
    if (status === 'part') lines.push(`*Partly:* ${org(why)}`, '');
    const checks = (area.checks ?? []).filter((check) => applies(check, id));
    // A check that needs a command the editor cannot run is no box to
    // tick: it is listed after the others, with what it lacks.
    const left = [];
    for (const check of checks) {
      const { keys, missing } = checkKeys(check, id);
      if (missing.length > 0) {
        const where = id === 'zed' ? 'its keys or code actions' : 'its code actions';
        left.push(`- ${org(check.text)}: ${missing.join(', ')} ${missing.length > 1 ? 'are' : 'is'} not among ${where}`);
      } else {
        lines.push(box(keys ? `${org(check.text)}: ${keys}` : org(check.text)));
      }
    }
    if (left.length > 0) lines.push('', `Not in ${editor.name}:`, '', ...left);
  }
  return lines.join('\n') + '\n';
}

/** `text` with every box unticked, to compare what is generated. */
const unticked = (text) => text.replace(/^- \[[xX]\] /gm, '- [ ] ');

// ----- the pages

const PAGES = {
  'docs/keys/neovim.org': { keys: () => neovimKeys() },
  'docs/keys/vim.org': { keys: vimKeys },
  'docs/keys/vscode.org': { keys: vscodeKeys },
  'docs/keys/sublime.org': { keys: sublimeKeys },
  'docs/keys/zed.org': { keys: zedKeys },
  'docs/protocol.org': { options: protocolOptions },
  'docs/configuration.org': { options: fileOptions },
  'docs/emacs-options.org': { options: emacsOptions },
  'README.org': { features: featureTable },
  ...Object.fromEntries(EDITORS.map((editor) => [`docs/${editor.id}.org`, { features: () => editorFeatureTable(editor.id) }])),
};

const END = '# END generated';
/** Files written whole. */
const FILES = {
  'lua/org-lsp/types.lua': luaTypes,
  'docs/config.schema.json': configSchema,
  'docs/config.toml': configToml,
  'docs/parity.org': parityPage,
};
/** Files written whole that keep what a person ticked. */
const TESTS = Object.fromEntries(EDITORS.map((editor) => [`${editor.dir}/TESTING.org`, editor.id]));

const problems = optionProblems();
if (problems.length > 0) {
  console.error(`scripts/org-options.json does not match:\n  ${problems.join('\n  ')}`);
  process.exit(1);
}

let stale = false;
for (const [file, render] of Object.entries(FILES)) {
  const path = join(root, file);
  const before = existsSync(path) ? readFileSync(path, 'utf8') : '';
  const after = render();
  if (after === before) continue;
  if (check) {
    console.error(`${file} is out of date with the server. Run: node scripts/generate-docs.mjs`);
    stale = true;
  } else {
    writeFileSync(path, after);
    console.log(`wrote ${file}`);
  }
}
for (const [file, id] of Object.entries(TESTS)) {
  const path = join(root, file);
  const before = existsSync(path) ? readFileSync(path, 'utf8') : '';
  const after = testingPage(id, before);
  if (unticked(after) === unticked(before)) continue;
  if (check) {
    console.error(`${file} is out of date with the server. Run: node scripts/generate-docs.mjs`);
    stale = true;
  } else {
    writeFileSync(path, after);
    console.log(`wrote ${file}`);
  }
}
for (const [page, sections] of Object.entries(PAGES)) {
  const path = join(root, page);
  const before = readFileSync(path, 'utf8');
  let after = before;
  for (const [name, render] of Object.entries(sections)) {
    const begin = `# BEGIN generated: ${name}`;
    const start = after.indexOf(begin);
    const end = after.indexOf(END, start);
    if (start < 0 || end < 0) {
      console.error(`${page} has no generated section \`${name}\``);
      process.exit(1);
    }
    after = after.slice(0, start + begin.length) + '\n\n' + render() + '\n\n' + after.slice(end);
  }
  if (after === before) continue;
  if (check) {
    console.error(`${page} is out of date with the server. Run: node scripts/generate-docs.mjs`);
    stale = true;
  } else {
    writeFileSync(path, after);
    console.log(`wrote ${page}`);
  }
}
if (stale) process.exit(1);
if (check) console.log('the documentation matches the server');
