// The shared end-to-end scenarios (tests/e2e/scenarios/*.json) in VS Code,
// by pressing the keys package.json binds for each step's command, with
// `xdotool`: VS Code's own key dispatch finds the command, through the
// `when` clauses of the agenda, the date prompt and capture buffers and the
// chords (`Ctrl+C A`). What a scenario expects is what the server's own
// runner saw (server/org-lsp/tests/e2e_scenarios.rs,
// tests/e2e/expected/NAME.txt); tests/nvim/scenarios.lua does the same in
// Neovim.
//
// Everything is typed into VS Code's window, as the user types it: the
// keys of each step, the text a step types, and the answers to what a
// command asks, in VS Code's own quick picks and input boxes (a key as its
// choice shows it, then Enter) and in the date prompt (`i`, the date,
// Enter). Nothing is answered for the user: the test only watches what VS
// Code shows, and checks that what is typed is taken as meant before it
// presses Enter. Opening a file, going to a tab and putting the cursor on
// a line are done as a click does them.
//
// `ORG_LSP_TEST_PRESET` is the preset (`org-lsp.keys`) run.mjs started VS
// Code with. A command with no key in it fails, unless
// tests/e2e/exceptions.json says why under `editors.vscode` (the emacs
// preset) or `editors.vscode-vim`: run.mjs binds it to a key of the
// user's then, as a step with a count presses a key run.mjs bound with
// the count, as a user binds one (docs/keys/vscode.org).
// `snapshots.vscode` there names what VS Code shows otherwise on purpose.
const assert = require('node:assert/strict');
const { execFile, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vscode = require('vscode');

const root = path.resolve(__dirname, '..', '..', '..');
const e2e = path.join(root, 'tests', 'e2e');
const preset = process.env.ORG_LSP_TEST_PRESET || 'emacs';
const exceptions = JSON.parse(fs.readFileSync(path.join(e2e, 'exceptions.json'), 'utf8'));
/** The snapshots VS Code differs in, whatever the keys: `NAME STEP KIND`. */
const differences = exceptions.snapshots?.vscode ?? {};

const AGENDA = 'org-agenda:/Org%20Agenda';
const DATE_PROMPT = 'org-agenda:/Org%20Date';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Wait until `probe` returns something truthy, and return it. */
async function until(what, probe, timeout = 10000) {
  const start = Date.now();
  for (;;) {
    const value = await probe();
    if (value) {
      return value;
    }
    if (Date.now() - start > timeout) {
      throw new Error(`timed out waiting for ${what}`);
    }
    await sleep(20);
  }
}

let api;

// ----- what the user sees and answers

/** What was said, as the user sees it: notifications. */
let messages = [];
/**
 * The prompts shown, the last one last: `kind` `pick` (a quick pick),
 * `input` (an input box) or `text` (text typed with completions offered),
 * with what VS Code tells of it as the user types: the item `active`, the
 * text typed (`value`). Each is `shown` once VS Code shows it, and `done`
 * once it is answered or gone.
 */
let prompts = [];

/** The prompt that waits for an answer now. */
function openPrompt() {
  return prompts.find((prompt) => prompt.shown && !prompt.done);
}

/**
 * Watch what VS Code shows, without answering anything: the prompts are
 * VS Code's own, answered by keys typed into its window; their options
 * are passed on with listeners added, that tell what VS Code shows.
 */
function watchPrompts() {
  for (const kind of ['showInformationMessage', 'showWarningMessage', 'showErrorMessage']) {
    const show = vscode.window[kind];
    vscode.window[kind] = (message, ...rest) => {
      messages.push(String(message).replace(/^org-lsp: /, ''));
      return show.call(vscode.window, message, ...rest);
    };
  }
  const showQuickPick = vscode.window.showQuickPick;
  vscode.window.showQuickPick = (items, options = {}, token) => {
    const prompt = { kind: 'pick', title: options.placeHolder, items: [], shown: false, done: false };
    prompts.push(prompt);
    Promise.resolve(items).then((list) => (prompt.items = list));
    // VS Code tells of the item it makes active, the first one once it shows.
    const selected = options.onDidSelectItem;
    const onDidSelectItem = (item) => {
      prompt.active = item;
      prompt.shown = true;
      return selected?.(item);
    };
    const answered = showQuickPick.call(vscode.window, items, { ...options, onDidSelectItem }, token);
    answered.then(
      () => (prompt.done = true),
      () => (prompt.done = true),
    );
    return answered;
  };
  const showInputBox = vscode.window.showInputBox;
  vscode.window.showInputBox = (options = {}, token) => {
    const prompt = { kind: 'input', title: options.prompt, value: options.value ?? '', shown: false, done: false };
    prompts.push(prompt);
    // VS Code validates the text once it shows, and at each change.
    const validate = options.validateInput;
    const validateInput = (value) => {
      prompt.value = value;
      prompt.shown = true;
      return validate?.(value);
    };
    const answered = showInputBox.call(vscode.window, { ...options, validateInput }, token);
    answered.then(
      () => (prompt.done = true),
      () => (prompt.done = true),
    );
    return answered;
  };
  const createQuickPick = vscode.window.createQuickPick;
  vscode.window.createQuickPick = (...args) => {
    const quickPick = createQuickPick.call(vscode.window, ...args);
    const prompt = { shown: false, done: false, value: '' };
    // What it shows is given after it is made: keys to pick (items with a
    // `key`), or text typed with completions.
    Object.defineProperties(prompt, {
      kind: { get: () => (quickPick.items.some((item) => 'key' in item) ? 'pick' : 'text') },
      title: { get: () => quickPick.title ?? quickPick.placeholder },
      items: { get: () => quickPick.items },
    });
    prompts.push(prompt);
    quickPick.onDidChangeActive((items) => {
      prompt.active = items[0];
      prompt.shown = true;
    });
    quickPick.onDidChangeValue((value) => (prompt.value = value));
    quickPick.onDidHide(() => (prompt.done = true));
    return quickPick;
  };
}

/** The item of `items` an answer picks: a key, or `{ choice: label }`. */
function pick(items, answer) {
  if (typeof answer === 'string') {
    return items.find((item) => item.key === answer);
  }
  // A key's choice shows with its key in front (`[t] TODO`).
  const label = (item) => item.label.replace(/^\[[^\]]*\] /, '');
  return items.find((item) => label(item) === answer.choice) ?? items.find((item) => label(item).includes(answer.choice));
}

function xdotool(...args) {
  return new Promise((resolve, reject) => execFile('xdotool', args, (error) => (error ? reject(error) : resolve())));
}

/** Type `text` into VS Code's window, as the user types it. */
function typeText(text) {
  return xdotool('type', '--delay', '40', '--', text);
}

/** How a key shows in its choice (`[SPC] …`), as the extension names it. */
function keyName(key) {
  const named = { ' ': 'SPC', '\r': 'RET', '\t': 'TAB' }[key];
  if (named) return named;
  const code = key.length === 1 ? key.charCodeAt(0) : -1;
  return code > 0 && code < 32 ? `C-${String.fromCharCode(code + 96)}` : key;
}

/**
 * Answer `prompt` with `given` as the user does: type it, see that VS
 * Code takes it as meant (the item it makes active, the text it has),
 * then press Enter. A key is typed as its choice shows it, a choice from
 * a list as its label, text as it is.
 */
async function answer(prompt, given) {
  const what = `\`${prompt.title}\``;
  if (prompt.kind === 'pick') {
    const wanted = pick(prompt.items, given);
    assert.ok(wanted, `${what}: no choice ${JSON.stringify(given)} in ${JSON.stringify(prompt.items.map((item) => item.label))}`);
    const typed = typeof given === 'string' ? keyName(given) : given.choice;
    await typeText(typed);
    await until(`${what} to take ${JSON.stringify(typed)}`, () => prompt.active === wanted, 3000).catch(() =>
      assert.fail(`${what}: typed ${JSON.stringify(typed)}, Enter would pick ${JSON.stringify(prompt.active?.label)}, not ${JSON.stringify(wanted.label)}`),
    );
  } else {
    assert.equal(typeof given, 'string', `${what} asks for text, not ${JSON.stringify(given)}`);
    await typeText(given);
    await until(`${what} to take ${JSON.stringify(given)}`, () => prompt.value === given, 3000).catch(() =>
      assert.fail(`${what}: typed ${JSON.stringify(given)}, VS Code has ${JSON.stringify(prompt.value)}`),
    );
    // Enter takes the item active: the text typed, once the items follow it.
    if (prompt.kind === 'text') {
      await until(`${what} to offer ${JSON.stringify(given)}`, () => prompt.active?.label === given, 3000).catch(() =>
        assert.fail(`${what}: typed ${JSON.stringify(given)}, Enter would take ${JSON.stringify(prompt.active?.label)}`),
      );
    }
  }
  await xdotool('key', 'Return');
  await until(`${what} to take the answer`, () => prompt.done, 5000);
}

// ----- keys

/** The user's keys (run.mjs's, with counts). */
let userBindings = [];

/** The keys package.json binds, then the user's. */
function bindings() {
  const own = vscode.extensions.getExtension('kristijanhusak.org-lsp').packageJSON.contributes.keybindings;
  return [...own, ...userBindings];
}

/**
 * Whether a `when` clause holds in `context`: the clauses package.json has
 * (`&&`, `||`, `!`, parentheses, `==`, `!=`, `=~`). A key VS Code would
 * not know is false.
 */
function holds(when, context) {
  const tokens = [];
  const pattern = /\s*(&&|\|\||==|!=|=~|!|\(|\)|'[^']*'|\/(?:\\.|[^/])*\/[a-z]*|[\w.:-]+)/y;
  for (let found = pattern.exec(when); found; found = pattern.exec(when)) {
    tokens.push(found[1]);
  }
  let at = 0;
  const value = (token) => (token.startsWith("'") ? token.slice(1, -1) : token);
  const primary = () => {
    const token = tokens[at++];
    if (token === '(') {
      const inner = or();
      at++;
      return inner;
    }
    const known = context[token];
    if (tokens[at] === '==' || tokens[at] === '!=') {
      const equal = tokens[at] === '==';
      const other = value(tokens[at + 1]);
      at += 2;
      return equal === (String(known) === other);
    }
    if (tokens[at] === '=~') {
      const [, source, flags] = tokens[at + 1].match(/^\/(.*)\/([a-z]*)$/);
      at += 2;
      return new RegExp(source, flags).test(String(known ?? ''));
    }
    return Boolean(known);
  };
  const not = () => {
    if (tokens[at] === '!') {
      at++;
      return !not();
    }
    return primary();
  };
  const and = () => {
    let result = not();
    while (tokens[at] === '&&') {
      at++;
      result = not() && result;
    }
    return result;
  };
  const or = () => {
    let result = and();
    while (tokens[at] === '||') {
      at++;
      result = and() || result;
    }
    return result;
  };
  return or();
}

/** What the `when` clauses look at, where the cursor is. */
function whenContext() {
  const editor = vscode.window.activeTextEditor;
  return {
    editorTextFocus: editor !== undefined,
    editorLangId: editor?.document.languageId,
    editorHasSelection: editor !== undefined && !editor.selection.isEmpty,
    resourceFilename: editor && path.basename(editor.document.uri.path),
    'config.org-lsp.keys': preset,
    'org-lsp.columns': editor !== undefined && api.columnsShown(editor).length > 0,
  };
}

/**
 * The key bound to `command` where the cursor is, with `count`: the first
 * binding whose `when` holds and whose count is the step's.
 */
function keyOf(command, count) {
  const context = whenContext();
  const binding = bindings().find(
    (candidate) =>
      candidate.command === command && (candidate.args?.count ?? undefined) === count && (!candidate.when || holds(candidate.when, context)),
  );
  return binding?.key;
}

/** `xdotool`'s names for a key of VS Code's, chord by chord (`ctrl+c a`). */
function xdotoolKeys(key) {
  const names = {
    enter: 'Return', escape: 'Escape', tab: 'Tab', space: 'space', backspace: 'BackSpace', delete: 'Delete',
    insert: 'Insert', home: 'Home', end: 'End', pageup: 'Prior', pagedown: 'Next',
    up: 'Up', down: 'Down', left: 'Left', right: 'Right',
    ',': 'comma', '.': 'period', '/': 'slash', '\\': 'backslash', ';': 'semicolon', "'": 'apostrophe',
    '[': 'bracketleft', ']': 'bracketright', '-': 'minus', '=': 'equal', '`': 'grave',
    ctrl: 'ctrl', shift: 'shift', alt: 'alt', meta: 'super',
  };
  return key.split(' ').map((chord) => {
    const [, modifiers, last] = chord.match(/^((?:(?:ctrl|shift|alt|meta)\+)*)(.+)$/);
    const parts = [...modifiers.split('+').filter(Boolean), last];
    return parts.map((part) => names[part] ?? (/^f\d+$/.test(part) ? part.toUpperCase() : part)).join('+');
  });
}

/**
 * Press `key` in VS Code's window; with `command`, check that it ran,
 * and answer what it asks with `answers`, in turn, until it is done.
 */
async function press(key, command, answers = []) {
  // The keys go to the editor at hand.
  await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
  const before = api.commands().count;
  await xdotool('key', '--delay', '30', ...xdotoolKeys(key));
  if (command) {
    await until(`\`${key}\` to run ${command}`, () => api.commands().count > before, 5000);
    // The first command run since: the extension keeps the last ones.
    const { run, count } = api.commands();
    const ran = run[run.length - (count - before)];
    assert.equal(ran, command, `\`${key}\` ran ${ran}, not ${command}`);
  }
  await answered(answers);
}

/**
 * Answer each prompt VS Code shows with the next of `answers`, until
 * nothing is under way, nothing changes and nothing is asked.
 */
async function answered(answers) {
  for (;;) {
    const prompt = await until('the command to finish or ask', () => openPrompt() ?? (api.commands().running === 0 && 'done'), 60000);
    if (prompt === 'done') {
      await settle();
      if (!openPrompt()) {
        return;
      }
      continue;
    }
    if (answers.length === 0) {
      // Given up, as the user gives up: the step fails.
      await xdotool('key', 'Escape');
      assert.fail(`\`${prompt.title}\` is asked and the step has no answer left`);
    }
    try {
      await answer(prompt, answers.shift());
    } catch (error) {
      if (!prompt.done) {
        await xdotool('key', 'Escape');
      }
      throw error;
    }
  }
}

// ----- the editor's state

/** What changes as the server and the extension do what was asked. */
function state() {
  const editor = vscode.window.activeTextEditor;
  return JSON.stringify([
    editor?.document.uri.toString(),
    editor?.selection.active,
    messages.length,
    vscode.window.visibleTextEditors.map((shown) => shown.document.uri.toString()),
    vscode.workspace.textDocuments.map((document) => [document.uri.toString(), document.version, document.isDirty]),
    api.commands().running,
  ]);
}

/**
 * Whether the agenda and the date prompt VS Code shows have the server's
 * text: VS Code fetches it some time after the server says it changed.
 */
async function viewsCurrent() {
  for (const uri of [AGENDA, DATE_PROMPT]) {
    const document = tabOf(uri) && openDocument(uri);
    // Asked of a document it does not show, the server would make one.
    if (document && (await api.content(uri)) !== document.getText()) {
      return false;
    }
  }
  return true;
}

/**
 * Wait until the server has handled everything, and the extension has done
 * what it asked: nothing is under way and nothing changes for a few round
 * trips.
 */
async function settle() {
  let last;
  let same = 0;
  for (let round = 0; round < 300 && same < 3; round++) {
    await api.sync();
    await api.sync();
    await sleep(30);
    const now = state();
    same = now === last && api.commands().running === 0 && (await viewsCurrent()) ? same + 1 : 0;
    last = now;
  }
}

function tabOf(uri) {
  return vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .find((tab) => tab.input instanceof vscode.TabInputText && tab.input.uri.toString() === uri);
}

function openDocument(uri) {
  return vscode.workspace.textDocuments.find((document) => document.uri.toString() === uri && !document.isClosed);
}

const ended = (text) => `${text.replace(/\n*$/, '')}\n`;

// ----- one scenario

function emacsFound() {
  try {
    execFileSync('emacs', ['--version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

/** The expected snapshots, by step and kind. */
function expectedOf(name) {
  const expected = {};
  let current;
  for (const line of fs.readFileSync(path.join(e2e, 'expected', `${name}.txt`), 'utf8').split('\n').slice(0, -1)) {
    const header = line.match(/^=== (\d+) (.+)$/);
    if (header) {
      current = `${header[1]} ${header[2]}`;
      expected[current] = [];
    } else if (current) {
      expected[current].push(line);
    }
  }
  return expected;
}

class Scenario {
  constructor(name) {
    this.name = name;
    this.spec = JSON.parse(fs.readFileSync(path.join(e2e, 'scenarios', `${name}.json`), 'utf8'));
    this.expected = expectedOf(name);
  }

  /** The scenario's files, in a directory of its own. */
  writeFiles() {
    this.directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), `org-lsp-vscode-${this.name}-`)));
    const files = {};
    if (this.spec.files_from) {
      const from = path.join(root, 'tests', this.spec.files_from);
      for (const file of fs.readdirSync(from)) {
        if (file.endsWith('.org') || file.endsWith('.org_archive')) {
          files[file] = fs.readFileSync(path.join(from, file), 'utf8');
        }
      }
    }
    for (const [file, text] of Object.entries(this.spec.files ?? {})) {
      files[file] = Array.isArray(text) ? text.map((line) => `${line}\n`).join('') : text;
    }
    for (const [file, text] of Object.entries(files)) {
      fs.mkdirSync(path.dirname(path.join(this.directory, file)), { recursive: true });
      fs.writeFileSync(path.join(this.directory, file), text);
    }
  }

  /** `value` with `$DIR` in its strings replaced by the directory. */
  substitute(value) {
    if (typeof value === 'string') {
      return value.replaceAll('$DIR', this.directory);
    }
    if (Array.isArray(value)) {
      return value.map((item) => this.substitute(item));
    }
    if (value && typeof value === 'object') {
      return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, this.substitute(item)]));
    }
    return value;
  }

  /** The options of the server's runner: the clock and the agenda files, then the scenario's. */
  settings() {
    const settings = {
      config_file: false,
      clock: this.spec.clock ?? { today: '2026-05-18', now: '10:30' },
      org_agenda_files: [this.directory],
      org_directory: this.directory,
      org_default_notes_file: `${this.directory}/notes.org`,
    };
    for (const [key, value] of Object.entries(this.spec.options ?? {})) {
      settings[key] = this.substitute(value);
    }
    if (this.spec.emacs) {
      const lisp = process.env.ORG_LISP_DIR;
      settings.emacs_config = (lisp ? `(add-to-list 'load-path ${JSON.stringify(lisp)}) ` : '') + (settings.emacs_config ?? '');
    }
    return settings;
  }

  /** How a document is named in the snapshots, as the server's runner names them. */
  nameOf(uri) {
    if (uri.toString() === AGENDA) {
      return 'agenda';
    }
    if (uri.toString() === DATE_PROMPT) {
      return 'date prompt';
    }
    const relative = path.relative(this.directory, uri.fsPath);
    return relative.startsWith('..') ? path.basename(uri.fsPath) : relative;
  }

  snapshot(kind) {
    const editor = vscode.window.activeTextEditor;
    if (kind === 'agenda') {
      const document = tabOf(AGENDA) && openDocument(AGENDA);
      return document ? ended(document.getText()) : '(no agenda)\n';
    }
    if (kind === 'buffer') {
      return `[${this.nameOf(editor.document.uri)}]\n${ended(editor.document.getText())}`;
    }
    if (kind === 'cursor') {
      const line = editor.selection.active.line;
      return `${this.nameOf(editor.document.uri)}:${line + 1}: ${editor.document.lineAt(line).text}\n`;
    }
    if (kind === 'messages') {
      return messages.map((message) => `${message}\n`).join('');
    }
    if (kind === 'columns') {
      const columns = api.columnsReported();
      if (columns === this.columnsBefore) {
        return '(no column view)\n';
      }
      if (!columns.lines) {
        return '(column view off)\n';
      }
      const rows = columns.lines.map(([line, , values]) => `${line + 1}: ${(values ?? []).join(' | ').trimEnd()}`);
      return [(columns.header ?? '').trimEnd(), ...rows].join('\n') + '\n';
    }
    const file = kind.match(/^file:(.*)$/)?.[1];
    assert.ok(file, `unknown snapshot kind ${kind}`);
    const full = path.join(this.directory, file);
    const document = openDocument(vscode.Uri.file(full).toString());
    if (document) {
      return ended(document.getText());
    }
    return fs.existsSync(full) ? ended(fs.readFileSync(full, 'utf8')) : '(no file)\n';
  }

  /** Put the cursor where the step says, as a user moves it there. */
  async place(step) {
    const editor = vscode.window.activeTextEditor;
    let position;
    if (step.line) {
      position = new vscode.Position(step.line - 1, step.column ?? 0);
    } else if (step.at) {
      const lines = editor.document.getText().split('\n');
      const row = lines.findIndex((text) => text.includes(step.at));
      assert.ok(row >= 0, `\`${step.at}\` is not in ${this.nameOf(editor.document.uri)}`);
      position = new vscode.Position(row, lines[row].indexOf(step.at) + (step.column ?? 0));
    }
    if (position) {
      editor.selection = new vscode.Selection(position, position);
      await settle();
    }
  }

  async run(index, step) {
    if (step.open) {
      const document = await vscode.workspace.openTextDocument(path.join(this.directory, step.open));
      await vscode.window.showTextDocument(document, { preview: false });
      await settle();
    }
    if (step.focus) {
      const uri = step.focus === 'agenda' ? AGENDA : vscode.Uri.file(path.join(this.directory, step.focus)).toString();
      const document = openDocument(uri);
      assert.ok(document && tabOf(uri), `no buffer ${step.focus} to go to`);
      const group = vscode.window.tabGroups.all.find((candidate) => candidate.tabs.includes(tabOf(uri)));
      await vscode.window.showTextDocument(document, { viewColumn: group.viewColumn, preview: false });
      await settle();
    }
    if (step.close) {
      // The agenda's own key.
      const key = keyOf('org-lsp.agenda.close');
      assert.ok(key, `no key to close ${this.nameOf(vscode.window.activeTextEditor.document.uri)}`);
      await press(key);
      await until('the agenda to close', () => !tabOf(AGENDA));
      await settle();
    }
    await this.place(step);
    if (step.type) {
      // Typed into the editor; Enter is Org's (`org.return`), as in Emacs.
      const editor = vscode.window.activeTextEditor;
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
      await typeText(step.type);
      const first = step.type.split('\n')[0];
      await until(`${JSON.stringify(step.type)} to be typed`, () => editor.document.getText().includes(first));
      await settle();
    }
    if (step.command) {
      const answers = structuredClone(step.answers ?? []);
      if (step.wait === 'columns') {
        this.columnsBefore = api.columnsReported();
      }
      // VS Code's keys add where the cursor is, as Emacs's do: to add below
      // the line, as the keys of modal editors do (`<Leader><CR>`), the
      // cursor goes to its end first (`C-e M-RET`).
      if (step.below) {
        await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
        await xdotool('key', 'End');
        await settle();
      }
      const key = keyOf(step.command, step.count);
      if (!key) {
        const where = vscode.window.activeTextEditor?.document.languageId;
        assert.fail(`${step.command}${step.count ? ` with count ${step.count}` : ''} has no key in VS Code's ${preset} preset here (${where})`);
      }
      await press(key, step.command, answers);
      // Dates are typed in the date prompt's input box, which `i` opens.
      while (answers.length > 0 && vscode.window.activeTextEditor?.document.uri.toString() === DATE_PROMPT) {
        const type = keyOf('org.calendar.type');
        assert.ok(type, 'no key for org.calendar.type');
        await press(type, 'org.calendar.type', answers);
      }
      if (step.wait === 'columns') {
        await until('column view', () => api.columnsReported() !== this.columnsBefore, 30000);
        await settle();
      }
      assert.equal(answers.length, 0, `${step.command} did not ask for ${JSON.stringify(answers)}`);
    }
    for (const kind of step.expect ?? []) {
      // What VS Code shows otherwise, on purpose.
      if (differences[`${this.name} ${index} ${kind}`]) {
        continue;
      }
      const want = this.expected[`${index} ${kind}`];
      assert.ok(want, `no expected ${kind}; run the server's runner with ORG_LSP_UPDATE_E2E=1`);
      const wanted = want.length > 0 ? `${want.join('\n')}\n` : '';
      // What comes later than the server's last answer (an editor that
      // closes) is given a moment more.
      let got;
      const start = Date.now();
      do {
        got = this.snapshot(kind).replaceAll(this.directory, '$DIR');
        if (got === wanted) {
          break;
        }
        await settle();
      } while (Date.now() - start < 3000);
      if (got !== wanted) {
        assert.fail(`${kind}:\n--- expected\n${wanted}--- actual\n${got}`);
      }
    }
    if (step.expect?.includes('messages')) {
      messages = [];
    }
  }
}

// One scenario, `ORG_LSP_TEST_SCENARIO`: run.mjs starts a VS Code for
// each, since one that ran before would leave its documents behind, which
// VS Code keeps a long while after their editors closed and gives to each
// server it starts.
describe(`The scenarios, with the ${preset} keys`, function () {
  this.timeout(180000);

  before(async () => {
    // Started without an Org file, the extension is not active yet.
    api = await vscode.extensions.getExtension('kristijanhusak.org-lsp').activate();
    const keybindings = path.join(process.env.ORG_LSP_TEST_USER_DATA ?? '', 'User', 'keybindings.json');
    userBindings = fs.existsSync(keybindings) ? JSON.parse(fs.readFileSync(keybindings, 'utf8')) : [];
    watchPrompts();
  });

  const name = process.env.ORG_LSP_TEST_SCENARIO;
  it(name, async function () {
    const scenario = new Scenario(name);
    if (scenario.spec.emacs && !emacsFound()) {
      assert.ok(!process.env.ORG_LSP_REQUIRE_EMACS, 'Emacs is required but not found');
      this.skip();
    }
    scenario.writeFiles();
    // The server's options: it starts again with them.
    const settings = vscode.workspace.getConfiguration('org-lsp');
    await settings.update('settings', scenario.settings(), vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('org-lsp.restartServer');
    try {
      for (const [index, step] of scenario.spec.steps.entries()) {
        try {
          await scenario.run(index + 1, step);
        } catch (error) {
          throw new Error(`step ${index + 1} ${JSON.stringify(step)}: ${error.message}`, { cause: error });
        }
      }
    } finally {
      fs.rmSync(scenario.directory, { recursive: true, force: true });
    }
  });
});
