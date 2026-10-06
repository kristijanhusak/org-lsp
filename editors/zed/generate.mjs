// Write the semantic token rules of the Zed extension
// (languages/*/semantic_token_rules.json) from the server's manifest: how
// each of its token types and modifiers looks, in the names of Zed's
// themes.
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
const rank = (selector) => (selector.startsWith('*.') ? 2 : selector.includes('.') ? 0 : 1);
const rules = selectors.sort((a, b) => rank(a) - rank(b)).map((selector) => rule(selector, looks[selector]));
const text = JSON.stringify(rules, null, 2) + '\n';

let stale = false;
for (const language of ['org', 'org-agenda']) {
  const path = join(here, 'languages', language, 'semantic_token_rules.json');
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
console.log(check ? 'the Zed extension matches the server' : `generated ${rules.length} semantic token rules`);
