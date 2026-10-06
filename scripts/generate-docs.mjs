// Write the generated parts of the documentation from what the server says
// about itself, so that it lists the keys the editors really bind and the
// options and defaults the server really has:
//
//   docs/<editor>/keys.md            the key tables
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

/** The Neovim plugin's keys: the `vim` preset of every command. */
function neovimKeys() {
  const sections = [];
  for (const [scope, heading] of SCOPES) {
    const rows = manifest.commands.flatMap((command) => {
      const keys = command.keys.filter((key) => key.preset === 'vim' && key.in === scope);
      if (keys.length === 0) return [];
      const shown = keys.map((key) => code(key.key) + (key.native ? '¹' : '') + (key.mode === 'insert' ? ' (insert mode)' : '')).join(', ');
      return [[shown, code(command.command), cell(command.title)]];
    });
    if (rows.length > 0) sections.push(`### ${heading}\n\n` + table(['Keys', 'Command', 'What it does'], rows));
  }
  const keyless = manifest.commands
    .filter((command) => !command.keys.some((key) => key.preset === 'vim'))
    .map((command) => [code(command.command), cell(command.title)]);
  sections.push('### Without a default key\n\n' + table(['Command', 'What it does'], keyless));
  sections.push(
    '¹ Where the command does not apply, the key does what it does in any\n' +
      'buffer: `<C-a>` off a timestamp still increments the number under the\n' +
      'cursor, and `<CR>` in a source block\'s code is Vim\'s own.',
  );
  return sections.join('\n\n');
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
      .map((key) => ({ before: ['<leader>', ...[...key.key.slice('<Leader>'.length)]], commands: [command.command] })),
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

// ----- options

/**
 * The options as a tree, in the table's order: `{ name, setting?, children? }`.
 * The options only an editor gives (`configFile`) are left out for the
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

// ----- the pages

const PAGES = {
  'docs/neovim/keys.md': { keys: neovimKeys },
  'docs/vscode/keys.md': { keys: vscodeKeys },
  'docs/neovim/configuration.md': { options: neovimOptions },
  'docs/vscode/configuration.md': { options: vscodeOptions },
  'docs/helix/configuration.md': { options: helixOptions },
  'docs/protocol.md': { options: protocolOptions },
  'docs/configuration.md': { options: fileOptions, 'options-json': fileOptionsJson },
};

const END = '<!-- END generated -->';
/** Files written whole. */
const FILES = {
  'lua/org-lsp/types.lua': luaTypes,
  'docs/config.schema.json': configSchema,
};

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
