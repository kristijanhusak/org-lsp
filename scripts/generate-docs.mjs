// Write the generated parts of the documentation from what the server says
// about itself, so that it lists the keys the editors really bind and the
// options and defaults the server really has:
//
//   docs/<editor>/keys.md            the key tables
//   docs/<editor>/configuration.md   the default options, in the editor's syntax
//   docs/protocol.md                 the table of options
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
      const shown = keys.map((key) => code(key.key) + (key.native ? '¹' : '')).join(', ');
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
      'cursor.',
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

/** The VS Code extension's keys, as its package.json binds them. */
function vscodeKeys() {
  const where = (when) =>
    when.includes(`'${manifest.agenda.languageId}'`)
      ? 'agenda'
      : manifest.datePrompt && when.includes(`'${manifest.datePrompt.languageId}'`)
        ? 'calendar'
        : when.includes(manifest.capture.prefix)
          ? 'capture'
          : 'other';
  const titles = Object.fromEntries(manifest.commands.map((command) => [command.command, command.title]));
  const sections = [];
  for (const [scope, heading] of SCOPES) {
    const byCommand = new Map();
    for (const binding of vscode.contributes.keybindings) {
      if (where(binding.when) !== scope) continue;
      if (!byCommand.has(binding.command)) byCommand.set(binding.command, []);
      byCommand.get(binding.command).push(code(vscodeKey(binding.key)));
    }
    if (byCommand.size === 0) continue;
    const rows = [...byCommand].map(([command, keys]) => [keys.join(', '), code(command), cell(titles[command] ?? '')]);
    sections.push(`### ${heading}\n\n` + table(['Keys', 'Command', 'What it does'], rows));
  }
  const every = manifest.commands.map((command) => [code(command.command), cell(`Org: ${command.title}`)]);
  sections.push('### Every command\n\n' + table(['Command', 'In the Command Palette'], every));
  return sections.join('\n\n');
}

// ----- options

/** The options as a tree, in the table's order: `{ name, setting?, children? }`. */
function optionTree() {
  const top = { children: [] };
  for (const setting of settings.filter((setting) => !setting.integration)) {
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

/** The same in TOML, as Helix's languages.toml takes it. */
function tomlOptions(node, table) {
  const lines = [`[${table}]`];
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
  for (const child of tables) lines.push(...tomlOptions(child, `${table}.${child.name}`));
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

// ----- the pages

const PAGES = {
  'docs/neovim/keys.md': { keys: neovimKeys },
  'docs/vscode/keys.md': { keys: vscodeKeys },
  'docs/neovim/configuration.md': { options: neovimOptions },
  'docs/vscode/configuration.md': { options: vscodeOptions },
  'docs/helix/configuration.md': { options: helixOptions },
  'docs/protocol.md': { options: protocolOptions },
};

const END = '<!-- END generated -->';
let stale = false;
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
