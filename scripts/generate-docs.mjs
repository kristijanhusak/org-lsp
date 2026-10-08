// Write the generated parts of the documentation from what the server says
// about itself, so that it lists the keys the editors really bind and the
// options and defaults the server really has:
//
//   docs/<editor>/keys.md            the key tables (Sublime Text's from its
//                                    keymap, Zed's from keymap.json and
//                                    tasks.json, which their own generators
//                                    write from the server's manifest)
//   docs/<editor>/configuration.md   the default options, in the editor's syntax
//   docs/configuration.md            the shared configuration file, at its defaults
//   docs/protocol.md                 the table of options
//   docs/config.schema.json          the JSON Schema of the shared file
//   lua/org-lsp/types.lua            the options' types, for lua-language-server
//
// Only the text between `<!-- BEGIN generated: NAME -->` and
// `<!-- END generated -->` is rewritten; the rest is written by hand.
//
//   node scripts/generate-docs.mjs [path/to/org-lsp]   rewrite the pages
//   node scripts/generate-docs.mjs --check             fail if out of date
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
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

const SCOPES = [
  ['global', 'Anywhere'],
  ['org', 'In Org files'],
  ['capture', 'In a capture buffer'],
  ['agenda', 'In the agenda'],
  ['calendar', 'In the date prompt'],
  ['columns', 'In column view'],
];

const code = (text) => '`' + text.replaceAll('|', '\\|') + '`';
const cell = (text) => text.replaceAll('|', '\\|');

function table(header, rows) {
  return [
    `| ${header.join(' | ')} |`,
    `| ${header.map(() => '---').join(' | ')} |`,
    ...rows.map((row) => `| ${row.join(' | ')} |`),
  ].join('\n');
}

// ----- keys

/** The keys the Neovim and Vim plugins keep for their own commands, by
 * scope: a command of the server's goes without them there. */
const OWN_KEYS = { org: ['<Tab>', '<S-Tab>', 'g?'], agenda: ['q', 'K', 'g?'], columns: ['h', 'l', '<Left>', '<Right>'] };

/** The Neovim plugin's keys: the `vim` preset of every command, in the
 * scopes the plugin has (all of them for Neovim). */
function neovimKeys(scopes = SCOPES.map(([scope]) => scope)) {
  const sections = [];
  for (const [scope, heading] of SCOPES.filter(([scope]) => scopes.includes(scope))) {
    const rows = manifest.commands.flatMap((command) => {
      const keys = command.keys.filter(
        (key) => key.preset === 'vim' && key.in === scope && !(OWN_KEYS[scope] ?? []).includes(key.key),
      );
      if (keys.length === 0) return [];
      const shown = keys.map((key) => code(key.key) + (key.native ? '¹' : '') + (key.mode === 'insert' ? ' (insert mode)' : '')).join(', ');
      return [[shown, code(command.command), cell(command.title)]];
    });
    if (rows.length > 0) sections.push(`### ${heading}\n\n` + table(['Keys', 'Command', 'What it does'], rows));
  }
  const keyless = manifest.commands
    .filter(
      (command) =>
        !command.keys.some(
          (key) => key.preset === 'vim' && scopes.includes(key.in) && !(OWN_KEYS[key.in] ?? []).includes(key.key),
        ),
    )
    .map((command) => [code(command.command), cell(command.title)]);
  sections.push('### Without a default key\n\n' + table(['Command', 'What it does'], keyless));
  sections.push(
    '¹ Where the command does not apply, the key does what it does in any\n' +
      'buffer: `<C-a>` off a timestamp still increments the number under the\n' +
      'cursor, and `<CR>` in a source block\'s code is Vim\'s own.',
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
    ['emacs', 'Emacs (`"org-lsp.keys": "emacs"`, the default)'],
    ['vim', 'Vim (`"org-lsp.keys": "vim"`)'],
  ];
  const sections = [];
  for (const [preset, presetHeading] of presets) {
    sections.push(`### ${presetHeading}`);
    for (const [scope, heading] of SCOPES) {
      const byCommand = new Map();
      for (const binding of vscode.contributes.keybindings) {
        if (!binding.when.includes(`keys == '${preset}'`) || where(binding.when) !== scope) continue;
        if (!byCommand.has(binding.command)) byCommand.set(binding.command, []);
        byCommand.get(binding.command).push(code(vscodeKey(binding.key)) + (binding.args?.fallback ? '¹' : ''));
      }
      if (byCommand.size === 0) continue;
      const rows = [...byCommand].map(([command, keys]) => [keys.join(', '), code(command), cell(titles[command] ?? '')]);
      sections.push(`#### ${heading}\n\n` + table(['Keys', 'Command', 'What it does'], rows));
    }
  }
  sections.push(
    '¹ Where the command does not apply, the key does what it does without\n' +
      'the extension: `Shift+Up` off a timestamp or headline selects, `Tab` off\n' +
      'a headline, block or drawer indents, `Enter` in a source block\'s code\n' +
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
    '#### The leader keys, for VSCodeVim\n\n' +
      'VS Code cannot bind keys that start with the leader without taking\n' +
      "them from Vim. Add them to VSCodeVim's settings instead (they apply in\n" +
      'every file; outside Org files the commands say they do not apply):\n\n' +
      '```json\n"vim.normalModeKeyBindingsNonRecursive": [\n' +
      unique
        .map((entry) => {
          const list = (items) => items.map((item) => JSON.stringify(item)).join(', ');
          return `  { "before": [${list(entry.before)}], "commands": [${list(entry.commands)}] }`;
        })
        .join(',\n') +
      '\n]\n```',
  );
  const every = [...manifest.commands, ...vscode.contributes.commands.filter((command) => ['org-lsp.cycle', 'org-lsp.cycleGlobal', 'org-lsp.help'].includes(command.command))].map(
    (command) => [code(command.command), cell(`Org: ${command.title}`)],
  );
  sections.push('### Every command\n\n' + table(['Command', 'In the Command Palette'], every));
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
    ['emacs', 'Emacs (`"org_keys": "emacs"`, the default)'],
    ['vim', 'Vim (`"org_keys": "vim"`, in command mode unless it says otherwise)'],
  ];
  const sections = [];
  for (const [preset, presetHeading] of presets) {
    sections.push(`### ${presetHeading}`);
    for (const [scope, heading] of SCOPES) {
      const byCommand = new Map();
      for (const binding of keymap) {
        if (context(binding, 'org_lsp.keys') !== preset || where(binding) !== scope) continue;
        const command = name(binding);
        if (!byCommand.has(command)) byCommand.set(command, []);
        const insert = context(binding, 'setting.command_mode') === false ? ' (insert mode)' : '';
        byCommand.get(command).push(code(sublimeKey(binding.keys)) + (binding.args?.fallback ? '¹' : '') + insert);
      }
      if (byCommand.size === 0) continue;
      const rows = [...byCommand].map(([command, keys]) => [keys.join(', '), code(command), cell(titles[command] ?? '')]);
      sections.push(`#### ${heading}\n\n` + table(['Keys', 'Command', 'What it does'], rows));
    }
  }
  sections.push(
    '¹ Where the command does not apply, the key does what it does without\n' +
      'the package: `Shift+Up` off a timestamp or headline selects, `Tab` off\n' +
      'a headline, block or drawer indents or completes, `Enter` makes a new\n' +
      'line.',
  );
  const bound = new Set(keymap.map(name));
  const keyless = manifest.commands
    .filter((command) => !bound.has(command.command))
    .map((command) => [code(command.command), cell(`Org: ${command.title}`)]);
  sections.push('### Without a key\n\n' + table(['Command', 'In the Command Palette'], keyless));
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
    .map((block) => {
      const heading = Object.entries(headings).find(([context]) => block.context.endsWith(context))?.[1] ?? block.context;
      const byAction = new Map();
      for (const [key, action] of Object.entries(block.bindings)) {
        const [command, title] = Array.isArray(action)
          ? [commandOf[action[1].task_name], titles[commandOf[action[1].task_name]]]
          : [action, zedActions[action]];
        if (!byAction.has(command)) byAction.set(command, { keys: [], title });
        byAction.get(command).keys.push(code(key));
      }
      const rows = [...byAction].map(([command, { keys, title }]) => [keys.join(', '), code(command), cell(title ?? '')]);
      return `### ${heading}\n\n` + table(['Keys', 'Command or Zed action', 'What it does'], rows);
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

/** `text` as comment lines of at most 78 columns. */
function comment(text, indent, marker) {
  const lines = [];
  let line = '';
  for (const word of text.split(/\s+/)) {
    if (line && indent.length + marker.length + 1 + line.length + 1 + word.length > 78) {
      lines.push(line);
      line = word;
    } else {
      line = line ? `${line} ${word}` : word;
    }
  }
  if (line) lines.push(line);
  return lines.map((text) => `${indent}${marker} ${text}`);
}

/** What a comment says about an option: its meaning and Org's name for it. */
function about(setting) {
  const org = setting.org ? ` (${setting.org})` : '';
  return `${setting.doc}${org}`;
}

// The server's JSON has its keys in alphabetical order; these read better
// first, in this order.
const FIRST = ['key', 'description', 'type', 'target', 'template', 'files'];
const entries = (object) =>
  Object.entries(object).sort(([a], [b]) => {
    const rank = (key) => (FIRST.includes(key) ? FIRST.indexOf(key) : FIRST.length);
    return rank(a) - rank(b) || a.localeCompare(b);
  });

function lua(value) {
  if (Array.isArray(value)) return value.length === 0 ? '{}' : `{ ${value.map(lua).join(', ')} }`;
  if (value && typeof value === 'object') {
    return `{ ${entries(value).map(([key, item]) => `${key} = ${lua(item)}`).join(', ')} }`;
  }
  if (typeof value === 'string') return `'${value.replaceAll('\\', '\\\\').replaceAll("'", "\\'").replaceAll('\n', '\\n')}'`;
  return String(value);
}

function json(value) {
  if (Array.isArray(value)) return value.length === 0 ? '[]' : `[${value.map(json).join(', ')}]`;
  if (value && typeof value === 'object') {
    return `{ ${entries(value).map(([key, item]) => `${JSON.stringify(key)}: ${json(item)}`).join(', ')} }`;
  }
  return JSON.stringify(value);
}

/** `value` as a Vim script expression. */
function vimValue(value) {
  if (Array.isArray(value)) return `[${value.map(vimValue).join(', ')}]`;
  if (value && typeof value === 'object') {
    return `{${entries(value).map(([key, item]) => `${vimValue(key)}: ${vimValue(item)}`).join(', ')}}`;
  }
  // A string with a line break in double quotes, whose escapes are JSON's.
  if (typeof value === 'string') return /[\n\t]/.test(value) ? JSON.stringify(value) : `'${value.replaceAll("'", "''")}'`;
  if (typeof value === 'boolean') return value ? 'v:true' : 'v:false';
  if (value === null) return 'v:null';
  return String(value);
}

function toml(value) {
  if (Array.isArray(value)) return `[${value.map(toml).join(', ')}]`;
  if (value && typeof value === 'object') {
    return `{ ${entries(value).map(([key, item]) => `${key} = ${toml(item)}`).join(', ')} }`;
  }
  return JSON.stringify(value);
}

/** The options with their defaults, as Lua for `settings = { … }`. */
function luaOptions(node, indent) {
  const lines = [];
  for (const [index, child] of node.children.entries()) {
    if (index > 0) lines.push('');
    if (child.setting) {
      const setting = child.setting;
      lines.push(...comment(about(setting), indent, '--'));
      lines.push(
        setting.default === undefined
          ? `${indent}-- ${child.name} = nil, -- unset: ${setting.unset}`
          : `${indent}${child.name} = ${lua(setting.default)},`,
      );
    } else {
      lines.push(`${indent}${child.name} = {`, ...luaOptions(child, indent + '  '), `${indent}},`);
    }
  }
  return lines;
}

/** The same in JSON with comments, as VS Code's settings.json takes it. */
function jsonOptions(node, indent) {
  const lines = [];
  const children = node.children;
  for (const [index, child] of children.entries()) {
    const comma = index < children.length - 1 ? ',' : '';
    if (index > 0) lines.push('');
    if (child.setting) {
      const setting = child.setting;
      lines.push(...comment(about(setting), indent, '//'));
      lines.push(
        setting.default === undefined
          ? `${indent}// ${JSON.stringify(child.name)}: unset: ${setting.unset}`
          : `${indent}${JSON.stringify(child.name)}: ${json(setting.default)}${comma}`,
      );
    } else {
      lines.push(`${indent}${JSON.stringify(child.name)}: {`, ...jsonOptions(child, indent + '  '), `${indent}}${comma}`);
    }
  }
  return lines;
}

/** The same as a Vim script dictionary, its lines continued with `\`
 * and its comments `"\ `, as `g:org_lsp`'s `settings` takes it. */
function vimDict(node, indent) {
  const lines = [];
  // No blank lines between the options: a comment line is `"\ ` with its
  // space, and a line of `"\` alone would end the dictionary.
  for (const child of node.children) {
    if (child.setting) {
      const setting = child.setting;
      // Each comment line is `      "\ ` and the indentation.
      lines.push(...comment(about(setting), '      "\\', ` ${indent.slice(0, -1)}`));
      lines.push(
        setting.default === undefined
          ? `      "\\ ${indent}'${child.name}': unset: ${setting.unset}`
          : `      \\ ${indent}'${child.name}': ${vimValue(setting.default)},`,
      );
    } else {
      lines.push(`      \\ ${indent}'${child.name}': {`, ...vimDict(child, indent + '  '), `      \\ ${indent}},`);
    }
  }
  return lines;
}

/** The same in TOML, as Helix's languages.toml and the shared file take it. */
function tomlOptions(node, table) {
  const lines = table ? [`[${table}]`] : [];
  const tables = [];
  for (const child of node.children) {
    if (!child.setting) {
      tables.push(child);
      continue;
    }
    const setting = child.setting;
    lines.push(...comment(about(setting), '', '#'));
    lines.push(
      setting.default === undefined
        ? `# ${child.name} = …  (unset: ${setting.unset})`
        : `${child.name} = ${toml(setting.default)}`,
      '',
    );
  }
  for (const child of tables) lines.push(...tomlOptions(child, table ? `${table}.${child.name}` : child.name));
  return lines;
}

function neovimOptions() {
  return ['```lua', "require('org-lsp').setup({", '  settings = {', ...luaOptions(optionTree(), '    '), '  },', '})', '```'].join(
    '\n',
  );
}

function vimOptions() {
  return ['```vim', 'let g:org_lsp = {', "      \\ 'settings': {", ...vimDict(optionTree(), '  '), '      \\ },', '      \\ }', '```'].join('\n');
}

function sublimeOptions() {
  return ['```jsonc', '{', '  "initializationOptions": {', ...jsonOptions(optionTree(), '    '), '  }', '}', '```'].join('\n');
}

function zedOptions() {
  return [
    '```jsonc',
    '{',
    '  "lsp": {',
    '    "org-lsp": {',
    '      "initialization_options": {',
    ...jsonOptions(optionTree(), '        '),
    '      }',
    '    }',
    '  }',
    '}',
    '```',
  ].join('\n');
}

function vscodeOptions() {
  return ['```jsonc', '{', '  "org-lsp.settings": {', ...jsonOptions(optionTree(), '    '), '  }', '}', '```'].join('\n');
}

function helixOptions() {
  const lines = tomlOptions(optionTree(), 'language-server.org-lsp.config');
  while (lines[lines.length - 1] === '') lines.pop();
  return ['```toml', ...lines, '```'].join('\n');
}

/** The shared configuration file with every option at its default. */
function fileOptions() {
  const lines = tomlOptions(optionTree({ editor: false }), '');
  while (lines[lines.length - 1] === '') lines.pop();
  return ['```toml', ...lines, '```'].join('\n');
}

/** The same as JSON with comments, for a `config.json`. */
function fileOptionsJson() {
  return ['```jsonc', '{', ...jsonOptions(optionTree({ editor: false }), '  '), '}', '```'].join('\n');
}

/** The table of options in docs/protocol.md. */
function protocolOptions() {
  const rows = settings.map((setting) => [
    code(setting.name),
    cell(setting.type),
    setting.default === undefined ? cell(setting.unset) : code(json(setting.default)),
    cell(setting.doc) + (setting.org ? ` (\`${setting.org}\`)` : ''),
  ]);
  return table(['Option', 'Type', 'Default', 'Meaning'], rows);
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
  group(optionTree(), 'org_lsp.Settings', "The server's options, as `settings` passes them; see docs/configuration.md.");
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
    description: "The options of the org-lsp server, in its shared configuration file. See docs/configuration.md.",
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
const LEGEND =
  '✅ all that org-lsp does works there, ❓ some does not (the editor\'s\n' +
  'README says what and why), ❌ none does, — nothing to support yet.\n' +
  'Parity is the share of what Emacs Org does in that area that org-lsp\n' +
  'does; [docs/parity.md](PARITY) lists both.';

/** The anchor GitHub gives a heading. */
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

/** Code in running text, outside a table. */
const inline = (text) => '`' + text + '`';

/** How a command is run in `editor`, from where `scope` says. */
function commandKey(editor, id, scope = 'org') {
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
      return found.length > 0 ? found.map((key) => inline(emacsKeyName(key))).join(' or ') : `*Org: ${command.title}*`;
    }
    default:
      return `the code action *${command.title}*`;
  }
}

/** Whether `check` applies to `editor`. */
const applies = (check, editor) => (!check.only || check.only.includes(editor)) && !check.not?.includes(editor);

/** The keys of a check in `editor`, or nothing. */
function checkKeys(check, editor) {
  if (check.keys) return check.keys[editor] ?? '';
  if (!check.cmd) return '';
  const ids = Array.isArray(check.cmd) ? check.cmd : [check.cmd];
  return ids.map((id) => commandKey(editor, id, check.in)).join(', ');
}

function featureTable(parityLink) {
  const header = ['Feature', 'Parity', EDITORS.map((editor) => editor.name).join(' / ')];
  const rows = AREAS.map((area) => [
    `[${area.name}](${parityLink}#${slug(area.name)})`,
    `${parity(area).percent}%`,
    EDITORS.map((editor) => MARK[support(area, editor.id)[0]]).join(' / '),
  ]);
  rows.push(['**All of Org**', `**${overall()}%**`, '']);
  return table(header, rows) + '\n\n' + LEGEND.replace('PARITY', parityLink);
}

function editorFeatureTable(id) {
  const editor = EDITORS.find((one) => one.id === id);
  const link = '../../docs/parity.md';
  const rows = AREAS.map((area) => {
    const [status, why] = support(area, id);
    return [`[${area.name}](${link}#${slug(area.name)})`, `${parity(area).percent}%`, MARK[status], cell(why)];
  });
  rows.push(['**All of Org**', `**${overall()}%**`, '', '']);
  return (
    table(['Feature', 'Parity', editor.name, 'What does not work, and why'], rows) +
    '\n\n' +
    LEGEND.replace('PARITY', link).replace("the editor's\nREADME says what and why", 'the last column says what\nand why') +
    '\n\n[TESTING.md](TESTING.md) lists what to try by hand.'
  );
}

function parityPage() {
  const lines = [
    '# Parity with Emacs Org',
    '',
    '<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs. -->',
    '',
    'What Emacs Org (9.8.10) does, area by area, and what org-lsp does of it.',
    'An area\'s parity is the share of its features done; all of Org is the',
    'mean of the areas. Every feature done is checked against Emacs where Org',
    'decides the outcome (see "How the work is checked" in PLAN.md).',
    '',
    `**All of Org: ${overall()}%**`,
    '',
    table(
      ['Feature', 'Parity', 'Done'],
      AREAS.map((area) => {
        const { done, total, percent } = parity(area);
        return [`[${area.name}](#${slug(area.name)})`, `${percent}%`, `${done} of ${total}`];
      }),
    ),
  ];
  for (const area of AREAS) {
    const { done, total, percent } = parity(area);
    lines.push('', `## ${area.name}`, '', `${percent}% (${done} of ${total})`, '');
    for (const [text, state, of] of area.emacs) {
      const mark = typeof state === 'number' ? (state === of ? '✅' : '❓') : state ? '✅' : '❌';
      lines.push(`- ${mark} ${text}${typeof state === 'number' ? ` (${state} of ${of})` : ''}`);
    }
    const short = EDITORS.map((editor) => [editor, ...support(area, editor.id)]).filter(([, status]) => status === 'part' || status === 'no');
    if (short.length > 0) {
      lines.push('', 'Where editors fall short:', '');
      for (const [editor, status, why] of short) lines.push(`- ${editor.name} ${MARK[status]}: ${why}`);
    }
  }
  return lines.join('\n') + '\n';
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
  const box = (text) => `- [${ticked.has(text) ? 'x' : ' '}] ${text}`;
  const lines = [
    `# Testing org-lsp in ${editor.name} by hand`,
    '',
    '<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs.',
    '     Ticked boxes are kept when it is written again. -->',
    '',
    `What to try in ${editor.name} to know that everything org-lsp does works`,
    'there. The keys are the defaults. Tick a box once it works; a check that',
    'fails is a bug to report. The [README](README.md) says what is left out',
    'here and why.',
    '',
    'To look at most of what a file shows at once (highlighting, concealing,',
    'entities, folding when it opens, indentation, shrunk columns,',
    'diagnostics), open [tests/visual.org](../../tests/visual.org): each of',
    'its sections says what it should look like.',
    '',
    '## Setup',
    '',
    ...editor.setup.map(box),
  ];
  for (const area of AREAS) {
    const [status, why] = support(area, id);
    if (status === 'none') continue;
    lines.push('', `## ${area.name}`, '');
    if (status === 'no') {
      lines.push(`**Not in ${editor.name}:** ${why}`);
      continue;
    }
    if (status === 'part') lines.push(`**Partly:** ${why}`, '');
    const checks = (area.checks ?? []).filter((check) => applies(check, id));
    for (const check of checks) {
      const keys = checkKeys(check, id);
      lines.push(box(keys ? `${check.text}: ${keys}` : check.text));
    }
  }
  return lines.join('\n') + '\n';
}

/** `text` with every box unticked, to compare what is generated. */
const unticked = (text) => text.replace(/^- \[[xX]\] /gm, '- [ ] ');

// ----- the pages

const PAGES = {
  'docs/neovim/keys.md': { keys: () => neovimKeys() },
  'docs/vim/keys.md': { keys: vimKeys },
  'docs/vscode/keys.md': { keys: vscodeKeys },
  'docs/sublime/keys.md': { keys: sublimeKeys },
  'docs/zed/keys.md': { keys: zedKeys },
  'docs/neovim/configuration.md': { options: neovimOptions },
  'docs/vim/configuration.md': { options: vimOptions },
  'docs/vscode/configuration.md': { options: vscodeOptions },
  'docs/sublime/configuration.md': { options: sublimeOptions },
  'docs/zed/configuration.md': { options: zedOptions },
  'docs/helix/configuration.md': { options: helixOptions },
  'docs/protocol.md': { options: protocolOptions },
  'docs/configuration.md': { options: fileOptions, 'options-json': fileOptionsJson },
  'README.md': { features: () => featureTable('docs/parity.md') },
  ...Object.fromEntries(EDITORS.map((editor) => [`${editor.dir}/README.md`, { features: () => editorFeatureTable(editor.id) }])),
};

const END = '<!-- END generated -->';
/** Files written whole. */
const FILES = {
  'lua/org-lsp/types.lua': luaTypes,
  'docs/config.schema.json': configSchema,
  'docs/parity.md': parityPage,
};
/** Files written whole that keep what a person ticked. */
const TESTS = Object.fromEntries(EDITORS.map((editor) => [`${editor.dir}/TESTING.md`, editor.id]));

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
    const begin = `<!-- BEGIN generated: ${name} -->`;
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
