// Write the key tables in docs/keys/*.md from what the server announces and
// what the VS Code extension binds, so that the documentation lists the
// keys the editors really set. Only the text between the markers is
// rewritten; the rest of each page is written by hand.
//
//   node scripts/generate-key-docs.mjs [path/to/org-lsp]   rewrite the pages
//   node scripts/generate-key-docs.mjs --check             fail if out of date
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
const manifest = JSON.parse(execFileSync(resolve(binary), ['manifest'], { encoding: 'utf8' }));
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

/** The Neovim plugin's keys: the `vim` preset of every command. */
function neovim() {
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
        .map((part) => names[part] ?? (part.length === 1 ? (modified ? part.toUpperCase() : part) : part[0].toUpperCase() + part.slice(1)))
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

const BEGIN = '<!-- BEGIN generated by scripts/generate-key-docs.mjs -->';
const END = '<!-- END generated -->';

let stale = false;
for (const [page, body] of [
  ['neovim.md', neovim()],
  ['vscode.md', vscodeKeys()],
]) {
  const path = join(root, 'docs', 'keys', page);
  const before = readFileSync(path, 'utf8');
  const start = before.indexOf(BEGIN);
  const end = before.indexOf(END);
  if (start < 0 || end < start) {
    console.error(`${path} has no generated section`);
    process.exit(1);
  }
  const after = before.slice(0, start + BEGIN.length) + '\n\n' + body + '\n\n' + before.slice(end);
  if (after === before) continue;
  if (check) {
    console.error(`docs/keys/${page} is out of date with the server. Run: node scripts/generate-key-docs.mjs`);
    stale = true;
  } else {
    writeFileSync(path, after);
    console.log(`wrote docs/keys/${page}`);
  }
}
if (stale) process.exit(1);
if (check) console.log('docs/keys match the server');
