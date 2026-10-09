// Write README.md, which VS Code and the Marketplace show on the
// extension's page and which they read only as Markdown, from the
// extension's page in the docs, docs/vscode.org. vsce runs it before it
// packages the extension (`vscode:prepublish`); README.md is not kept in
// the repository.
//
//   node scripts/readme.mjs            write README.md
//   node scripts/readme.mjs --check    write nothing; fail if what it would
//                                      write has Org left in it
//
// It knows the Org the docs are written in: headings, paragraphs, lists
// (description lists among them), tables, source and example blocks,
// links, and code, verbatim, bold and italic text. Comments, keywords and
// property drawers are left out. A link to another file of the repository
// becomes a link to it on GitHub, where Org files are shown as pages.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..', '..');
const source = join(root, 'docs', 'vscode.org');
const BLOB = 'https://github.com/kristijanhusak/org-lsp/blob/HEAD/';

// Where Org lets emphasis start and end (`org-emphasis-regexp-components`).
const PRE = "\\s\\-('\"{";
const POST = "\\s\\-.,:!?;'\")}\\[";

/** A link's target, as a URL. */
function url(target) {
  if (/^[a-z]+:\/\//.test(target)) return target;
  if (target.startsWith('#')) return target;
  const [path, search] = target.replace(/^file:/, '').split('::');
  const file = relative(root, resolve(dirname(source), path));
  return BLOB + file + (search?.startsWith('#') ? search : '');
}

/** Org's inline markup in `text` as Markdown's. */
function inline(text) {
  const kept = [];
  const keep = (markdown) => `\u0000${kept.push(markdown) - 1}\u0000`;
  let out = text
    // Links, with their descriptions converted in turn.
    .replace(/\[\[([^\]]+)\](?:\[([^\]]+)\])?\]/g, (match, target, description) =>
      keep(`[${description ? inline(description) : target}](${url(target)})`),
    )
    // Code and verbatim text, as it is.
    .replace(new RegExp(`(^|[${PRE}])([~=])([^\\s~=]|[^\\s][^\\n]*?[^\\s])\\2(?=[${POST}]|$)`, 'g'), (match, before, marker, code) => {
      const fence = code.includes('`') ? '``' : '`';
      const pad = code.includes('`') ? ' ' : '';
      return before + keep(`${fence}${pad}${code}${pad}${fence}`);
    });
  out = out
    // Bold and italic, which may go over a line's end, as in Org.
    .replace(new RegExp(`(^|[${PRE}])\\*([^\\s*]|[^\\s][^*]*?[^\\s])\\*(?=[${POST}]|$)`, 'gm'), '$1**$2**')
    .replace(new RegExp(`(^|[${PRE}])/([^\\s/]|[^\\s][^/]*?[^\\s])/(?=[${POST}]|$)`, 'gm'), '$1*$2*');
  return out.replace(/\u0000(\d+)\u0000/g, (match, index) => kept[Number(index)]);
}

/** The cells of a row of an Org table. */
const cells = (line) =>
  line
    .trim()
    .replace(/^\||\|$/g, '')
    .split('|')
    .map((cell) => inline(cell.trim()));

/** Whether `line` starts something other than text. */
const special = (line) =>
  /^\s*#(\+| |$)/.test(line) || /^\s*:PROPERTIES:/.test(line) || /^\*+ /.test(line) || /^\s*\|/.test(line);

function markdown(org) {
  const lines = org.split('\n');
  const out = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const title = /^#\+title:\s*(.*)$/i.exec(line);
    if (title) {
      out.push(`# ${inline(title[1])}`);
      continue;
    }
    // Keywords, comments and property drawers: not shown.
    if (/^\s*#\+(?!begin_|end_)/i.test(line) || /^\s*#( |$)/.test(line)) continue;
    if (/^\s*:PROPERTIES:\s*$/.test(line)) {
      while (i < lines.length && !/^\s*:END:\s*$/.test(lines[i])) i += 1;
      continue;
    }
    const heading = /^(\*+) (.*)$/.exec(line);
    if (heading) {
      out.push(`${'#'.repeat(heading[1].length + 1)} ${inline(heading[2])}`);
      continue;
    }
    // Blocks: their lines as they are, without Org's commas.
    const block = /^(\s*)#\+begin_(src|example)\s*(\S*)/i.exec(line);
    if (block) {
      const [, indent, kind, language] = block;
      out.push(`${indent}\`\`\`${kind.toLowerCase() === 'src' ? language : ''}`);
      for (i += 1; i < lines.length && !/^\s*#\+end_/i.test(lines[i]); i += 1) {
        out.push(lines[i].replace(/^(\s*),(\*|#\+)/, '$1$2'));
      }
      out.push(`${indent}\`\`\``);
      continue;
    }
    // Tables: a rule under the header, the others left out.
    if (/^\s*\|/.test(line)) {
      const rows = [];
      for (; i < lines.length && /^\s*\|/.test(lines[i]); i += 1) rows.push(lines[i]);
      i -= 1;
      const rule = (row) => /^\s*\|[-+]+\|?\s*$/.test(row);
      const body = rows.filter((row) => !rule(row)).map(cells);
      const header = rule(rows[1] ?? '') ? body.shift() : body[0].map(() => '');
      out.push(`| ${header.join(' | ')} |`, `| ${header.map(() => '---').join(' | ')} |`);
      for (const row of body) out.push(`| ${row.join(' | ')} |`);
      continue;
    }
    if (line.trim() === '') {
      out.push('');
      continue;
    }
    // Text: the lines up to a blank one or one of the above, at once, as
    // markup may go over a line's end.
    const run = [line];
    while (i + 1 < lines.length && lines[i + 1].trim() !== '' && !special(lines[i + 1])) run.push(lines[(i += 1)]);
    for (const converted of inline(run.join('\n')).split('\n')) {
      // A description list's item: its term in bold.
      out.push(converted.replace(/^(\s*)- (.+?) :: /, '$1- **$2**: '));
    }
  }
  return out.join('\n').replace(/\n{3,}/g, '\n\n');
}

const written =
  `<!-- Written from docs/vscode.org by scripts/readme.mjs when the extension is packaged. -->\n\n` +
  markdown(readFileSync(source, 'utf8')).trim() +
  '\n';
if (process.argv.includes('--check')) {
  // Org that was not turned into Markdown.
  const outside = written.replace(/```[\s\S]*?```/g, '').replace(/`[^`]*`/g, '');
  const left = outside.split('\n').filter((line) => /#\+|\[\[|(^|\s)[~=]\S|\S[~=](\s|$)|^\*+ /.test(line));
  if (left.length > 0) {
    console.error(`README.md would have Org left in it:\n${left.join('\n')}`);
    process.exit(1);
  }
  console.log('README.md can be written from docs/vscode.org');
} else {
  writeFileSync(join(here, '..', 'README.md'), written);
}
