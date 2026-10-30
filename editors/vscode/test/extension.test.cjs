// The extension inside VS Code, against the real server.
const assert = require('node:assert/strict');
const path = require('node:path');
const vscode = require('vscode');

// A key is picked from a quick pick the extension makes itself
// (`pickKey`), which follows the key typed; the scenarios type into it.
// Here it is answered as any list is, through `showQuickPick`, which the
// tests answer in their place.
const createQuickPick = vscode.window.createQuickPick;
vscode.window.createQuickPick = (...args) => {
  const quickPick = createQuickPick.call(vscode.window, ...args);
  const listeners = { accept: [], hide: [] };
  const show = quickPick.show.bind(quickPick);
  return new Proxy(quickPick, {
    get(target, name) {
      if (name === 'onDidAccept' || name === 'onDidHide') {
        return (listener) => {
          listeners[name === 'onDidAccept' ? 'accept' : 'hide'].push(listener);
          return { dispose() {} };
        };
      }
      if (name === 'show') {
        return () => {
          if (!target.items.some((item) => 'key' in item)) {
            // Text with completions: VS Code's own.
            listeners.accept.forEach((listener) => target.onDidAccept(listener));
            listeners.hide.forEach((listener) => target.onDidHide(listener));
            return show();
          }
          void vscode.window.showQuickPick(target.items, { placeHolder: target.placeholder }).then((picked) => {
            if (picked) {
              Object.defineProperty(target, 'selectedItems', { value: [picked] });
              listeners.accept.forEach((listener) => listener());
            }
            listeners.hide.forEach((listener) => listener());
          });
        };
      }
      if (name === 'hide') {
        return () => (target.items.some((item) => 'key' in item) ? undefined : target.hide());
      }
      const value = Reflect.get(target, name);
      return typeof value === 'function' ? value.bind(target) : value;
    },
    set(target, name, value) {
      return Reflect.set(target, name, value);
    },
  });
};

const workspace = process.env.ORG_LSP_TEST_WORKSPACE;
const workUri = vscode.Uri.file(path.join(workspace, 'work.org'));

/** Wait until `probe` returns something truthy, and return it. */
async function until(what, probe, timeout = 15000) {
  const start = Date.now();
  for (;;) {
    const value = await probe();
    if (value) {
      return value;
    }
    if (Date.now() - start > timeout) {
      throw new Error(`timed out waiting for ${what}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

const activeText = () => vscode.window.activeTextEditor?.document.getText() ?? '';
const activeLine = () => {
  const editor = vscode.window.activeTextEditor;
  return editor ? editor.document.lineAt(editor.selection.active.line).text : undefined;
};

/** Wait until the agenda is the active editor and its first line is `title`. */
function agenda(title) {
  return until(`the agenda \`${title}\``, () => {
    const editor = vscode.window.activeTextEditor;
    return editor?.document.uri.scheme === 'org-agenda' && editor.document.lineAt(0).text === title && editor;
  });
}

function moveTo(line, character = 0) {
  const position = new vscode.Position(line, character);
  vscode.window.activeTextEditor.selection = new vscode.Selection(position, position);
}

/** One editor group, showing the Org file: the layout the window tests start from. */
async function oneGroup() {
  await vscode.commands.executeCommand('workbench.action.joinAllGroups');
  for (const tab of vscode.window.tabGroups.all.flatMap((group) => group.tabs)) {
    if (tab.input instanceof vscode.TabInputText && tab.input.uri.scheme === 'org-agenda') {
      await vscode.window.tabGroups.close(tab);
    }
  }
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(workUri));
}

/** How the editor groups are laid out: `[orientation, number of groups]`. */
async function layout() {
  const found = await vscode.commands.executeCommand('vscode.getEditorLayout');
  return [found.orientation, found.groups.length];
}
const BELOW = 1;
const BESIDE = 0;

/** Semantic tokens of `uri` as `[line, text, type, modifiers]`. */
async function tokens(uri) {
  const legend = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokensLegend', uri);
  const found = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri);
  if (!legend || !found) {
    return undefined;
  }
  const document = await vscode.workspace.openTextDocument(uri);
  const out = [];
  let line = 0;
  let column = 0;
  for (let i = 0; i < found.data.length; i += 5) {
    const [deltaLine, deltaStart, length, type, bits] = found.data.slice(i, i + 5);
    line += deltaLine;
    column = deltaLine === 0 ? column + deltaStart : deltaStart;
    const modifiers = legend.tokenModifiers.filter((_, bit) => bits & (1 << bit));
    out.push([line, document.lineAt(line).text.slice(column, column + length), legend.tokenTypes[type], modifiers]);
  }
  return out;
}

describe('Org files', () => {
  let document;

  before(async () => {
    document = await vscode.workspace.openTextDocument(workUri);
    await vscode.window.showTextDocument(document);
  });

  it('knows Org files and starts the server for them', async () => {
    assert.equal(document.languageId, 'org');
    await until('the server', async () => (await tokens(workUri)) !== undefined);
  });

  // In restricted mode VS Code reads these from the user's settings only:
  // a folder's settings could name a program for the server to run.
  it('declares the settings that name programs restricted in untrusted workspaces', () => {
    const declared = vscode.extensions.getExtension('kristijanhusak.org-lsp').packageJSON.capabilities.untrustedWorkspaces;
    assert.equal(declared.supported, 'limited');
    assert.deepEqual(declared.restrictedConfigurations, ['org-lsp.server.path', 'org-lsp.settings']);
  });

  it('highlights with the server\'s tokens', async () => {
    const found = await tokens(workUri);
    assert.deepEqual(found.slice(0, 7), [
      [0, '#+TITLE:', 'keyword', []],
      [0, 'Work', 'documentTitle', []],
      [1, '*', 'stars', ['level1']],
      [1, 'TODO', 'todoKeyword', []],
      [1, '[#A]', 'priority', []],
      [1, 'Write report', 'headline', ['level1']],
      [1, ':office:', 'tag', []],
    ]);
    assert.ok(found.some(([, text, type, modifiers]) => text === '*bold*' && type === 'text' && modifiers.includes('bold')));
  });

  it('gives keywords, priorities and tags their faces', async () => {
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    const editor = await vscode.window.showTextDocument(document);
    const shown = await until('the faces', () => {
      const found = api.faces(editor);
      return found.length >= 4 && found;
    });
    assert.deepEqual(shown, [[1, 'TODO'], [1, '[#A]'], [1, 'office'], [5, 'TODO']]);
  });

  it('folds and outlines', async () => {
    const folds = await vscode.commands.executeCommand('vscode.executeFoldingRangeProvider', workUri);
    assert.deepEqual(folds.map((fold) => [fold.start, fold.end]), [[1, 4], [5, 6]]);
    const symbols = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', workUri);
    assert.deepEqual(symbols.map((symbol) => symbol.name), ['Write report', 'Call the client', 'Meeting <2026-05-19 Tue 14:00-15:00>']);
    assert.equal(symbols[0].children[0].name, 'Outline of the report');
  });

  it('changes a TODO state at the cursor', async () => {
    moveTo(6, 3); // in the body of "Call the client"
    await vscode.commands.executeCommand('org.todo.next');
    await until('TODO -> DONE', () => document.lineAt(5).text === '* DONE Call the client');
    await vscode.commands.executeCommand('org.todo.previous');
    await until('DONE -> TODO', () => document.lineAt(5).text === '* TODO Call the client');
    // It is also offered as a code action, which carries the edit itself.
    const actions = await vscode.commands.executeCommand('vscode.executeCodeActionProvider', workUri, new vscode.Range(6, 3, 6, 3));
    assert.deepEqual(actions.map((action) => action.kind.value), [
      'org.todo.next',
      'org.todo.previous',
      'org.priority.up',
      'org.priority.down',
      'org.clock.in',
      'org.archive.subtree',
      'org.archive.sibling',
      'org.archive.tag',
    ]);
    assert.ok(actions[0].edit.has(workUri));
    // Clocking in runs as a command: it may stop a clock in another file.
    assert.equal(actions[4].command.command, 'org.clock.in');
    await document.save();
  });

  it('asks for a priority and sets it', async () => {
    const asked = [];
    const answers = ['c', ' '];
    const showInputBox = vscode.window.showInputBox;
    vscode.window.showInputBox = async (options) => {
      asked.push(options.prompt);
      return answers.shift();
    };
    try {
      moveTo(5, 3); // on "Call the client"
      await vscode.commands.executeCommand('org.priority');
      await until('the priority typed', () => document.lineAt(5).text === '* TODO [#C] Call the client');
      await vscode.commands.executeCommand('org.priority');
      await until('a space removes it', () => document.lineAt(5).text === '* TODO Call the client');
      assert.deepEqual(asked, ['Priority A-C, SPC to remove', 'Priority A-C, SPC to remove']);
    } finally {
      vscode.window.showInputBox = showInputBox;
    }
    await document.save();
  });

  it('picks a TODO state by key and asks for its note', async () => {
    const keyed = await vscode.workspace.openTextDocument({
      language: 'org',
      content: '#+TODO: TODO(t) WAIT(w@/!) | DONE(d!)\n* TODO Call them\n',
    });
    const editor = await vscode.window.showTextDocument(keyed);
    const offered = [];
    let icons;
    const showQuickPick = vscode.window.showQuickPick;
    vscode.window.showQuickPick = async (items) => {
      offered.push(...items.map((item) => item.label));
      icons = items.map((item) => item.iconPath);
      return items.find((item) => item.key === 'w');
    };
    try {
      editor.selection = new vscode.Selection(1, 2, 1, 2);
      await vscode.commands.executeCommand('org.todo');
      // The note is written in a buffer of its own, as Emacs's `*Org Note*`.
      const note = await until('the note buffer', () => {
        const active = vscode.window.activeTextEditor;
        return active && path.basename(active.document.uri.fsPath) === 'CAPTURE-Note.org' && active;
      });
      assert.equal(note.document.lineAt(0).text, '# Insert note for state change from "TODO" to "WAIT".');
      await until('the cursor on the third line', () => note.selection.active.line === 2);
      await note.edit((edit) => edit.insert(note.selection.active, 'Waiting for Bob'));
      await vscode.commands.executeCommand('org.capture.finish');
      await until('the state and its note', () => keyed.lineCount > 3 && keyed.lineAt(3).text === '  Waiting for Bob');
      assert.equal(keyed.lineAt(1).text, '* WAIT Call them');
      assert.match(keyed.lineAt(2).text, /^- State "WAIT" {7}from "TODO" {7}\[.*\] \\\\$/);
      assert.deepEqual(offered, ['[t] TODO', '[w] WAIT', '[d] DONE', '[SPC] clear']);
      // Each keyword in its color, as `org-fast-todo-selection` shows it:
      // TODO in its face (the tests give it one), the others as TODO and
      // done keywords.
      // In the colors of `org-todo` and `org-done`, for light and dark
      // themes: a quick pick shows an icon of the theme's in one color.
      const fill = (icon) => require('node:fs').readFileSync(icon.fsPath, 'utf8').match(/fill="([^"]*)"/)[1];
      assert.equal(fill(icons[0]), '#ff8800');
      // Files: a quick pick shows no icon of a `vscode-userdata:` URI.
      assert.deepEqual([icons[0].scheme, icons[1].light.scheme, icons[1].dark.scheme], ['file', 'file', 'file']);
      assert.deepEqual(
        icons.slice(1).map((icon) => icon && [fill(icon.light), fill(icon.dark)]),
        [['red', 'Pink'], ['ForestGreen', 'PaleGreen'], undefined],
      );
      await until('the note buffer to close', () =>
        vscode.window.tabGroups.all.flatMap((group) => group.tabs).every((tab) => tab.label !== 'CAPTURE-Note.org'),
      );
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.window.showTextDocument(keyed);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.window.showTextDocument(document);
  });

  it('refiles a subtree under a target picked from a list', async () => {
    const notes = await vscode.workspace.openTextDocument({
      language: 'org',
      content: '* Inbox\n** Call Bob\nNotes.\n* Projects\n** Alpha\n',
    });
    const editor = await vscode.window.showTextDocument(notes);
    const offered = [];
    const showQuickPick = vscode.window.showQuickPick;
    vscode.window.showQuickPick = async (items, options) => {
      offered.push(options.placeHolder, ...items.map((item) => item.label));
      return items.find((item) => item.key === 'Projects');
    };
    try {
      editor.selection = new vscode.Selection(2, 0, 2, 0);
      await vscode.commands.executeCommand('org.refile');
      await until('the subtree under its new parent', () => notes.getText() === '* Inbox\n* Projects\n** Alpha\n** Call Bob\nNotes.\n');
      assert.deepEqual(offered, ['Refile subtree "Call Bob" to', 'Inbox', 'Projects']);
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.window.showTextDocument(document);
  });

  it('captures into a file of its own, through a capture buffer', async () => {
    const captured = process.env.ORG_LSP_TEST_CAPTURED;
    const showQuickPick = vscode.window.showQuickPick;
    vscode.window.showQuickPick = async (items) => items.find((item) => item.key === 't');
    try {
      await oneGroup();
      await vscode.commands.executeCommand('org.capture');
      const editor = await until('the capture buffer', () => {
        const active = vscode.window.activeTextEditor;
        return active && path.basename(active.document.uri.fsPath) === 'CAPTURE-inbox.org' && active;
      });
      assert.deepEqual(await layout(), [BELOW, 2], 'in a group below');
      assert.equal(editor.document.getText(), '* TODO \n');
      await until('the cursor where %? was', () => editor.selection.active.isEqual(new vscode.Position(0, 7)));
      await editor.edit((edit) => edit.insert(editor.selection.active, 'Call mom'));
      await vscode.commands.executeCommand('org.capture.finish');
      // The file did not exist: the server writes it.
      const fs = require('node:fs');
      await until('the entry in its file', () => fs.existsSync(captured) && fs.readFileSync(captured, 'utf8') === '* TODO Call mom\n');
      await until('the capture buffer to close', () =>
        vscode.window.tabGroups.all.flatMap((group) => group.tabs).every((tab) => !tab.label.startsWith('CAPTURE-')),
      );
      await until('its group to close', async () => (await layout())[1] === 1);
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.window.showTextDocument(document);
  });

  it('captures from an org-protocol URL', async () => {
    // `org-lsp protocol URL`, as the system runs it for a bookmarklet:
    // the server asks VS Code to start the capture (`org-lsp/runCommand`).
    const { execFileSync } = require('node:child_process');
    const fs = require('node:fs');
    const listening = path.join(process.env.XDG_STATE_HOME, 'org-lsp', 'protocol');
    await until('the server to listen', () => fs.existsSync(listening) && fs.readdirSync(listening).some((name) => name.endsWith('.json')));
    await oneGroup();
    const url = 'org-protocol://capture?template=w&url=https%3A%2F%2Forgmode.org%2F&title=Org+mode&body=Selected';
    execFileSync(process.env.ORG_LSP_TEST_SERVER, ['protocol', url], { env: process.env });
    const editor = await until('the capture buffer', () => {
      const active = vscode.window.activeTextEditor;
      return active && path.basename(active.document.uri.fsPath) === 'CAPTURE-inbox.org' && active;
    });
    assert.equal(editor.document.getText(), '* Org mode\nhttps://orgmode.org/\nSelected\n');
    await vscode.commands.executeCommand('org.capture.abort');
    await until('the capture buffer to close', () =>
      vscode.window.tabGroups.all.flatMap((group) => group.tabs).every((tab) => !tab.label.startsWith('CAPTURE-')),
    );
    await vscode.window.showTextDocument(document);
  });

  it('captures with no file open', async () => {
    const showQuickPick = vscode.window.showQuickPick;
    vscode.window.showQuickPick = async (items) => items.find((item) => item.key === 't');
    try {
      await oneGroup();
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      await vscode.commands.executeCommand('org.capture');
      await until('the capture buffer', () => {
        const active = vscode.window.activeTextEditor;
        return active && path.basename(active.document.uri.fsPath) === 'CAPTURE-inbox.org';
      });
      await vscode.commands.executeCommand('org.capture.abort');
      await until('the capture buffer to close', () =>
        vscode.window.tabGroups.all.flatMap((group) => group.tabs).every((tab) => !tab.label.startsWith('CAPTURE-')),
      );
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.window.showTextDocument(document);
  });

  it('asks for a date, shows what it means, and uses it', async () => {
    const asked = [];
    const showInputBox = vscode.window.showInputBox;
    vscode.window.showInputBox = async (options) => {
      asked.push({ prompt: options.prompt, value: options.value, preview: await options.validateInput('fri 9:00') });
      return '2026-06-03';
    };
    try {
      moveTo(5, 3); // on "Call the client"
      // The calendar opens; the answer is asked for from there.
      await vscode.commands.executeCommand('org.schedule');
      await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date');
      assert.deepEqual(asked, []);
      await vscode.commands.executeCommand('org.calendar.type');
      await until('the entry to be scheduled', () => document.lineAt(6).text === 'SCHEDULED: <2026-06-03 Wed> DEADLINE: <2026-05-20 Wed>');
      assert.deepEqual(asked, [
        {
          prompt: 'SCHEDULED Date+time [2026-05-18]',
          value: '',
          preview: { message: '=> <2026-05-22 Fri 09:00>', severity: vscode.InputBoxValidationSeverity.Info },
        },
      ]);
    } finally {
      vscode.window.showInputBox = showInputBox;
    }
    await vscode.commands.executeCommand('undo');
    await until('the schedule undone', () => document.lineAt(6).text === 'DEADLINE: <2026-05-20 Wed>');
    await document.save();
  });

  it('shows a calendar with the date prompt, as Emacs does', async () => {
    const answers = ['9:00'];
    const showInputBox = vscode.window.showInputBox;
    vscode.window.showInputBox = async () => answers.shift();
    const calendar = () => vscode.window.visibleTextEditors.find((editor) => editor.document.languageId === 'org-date');
    try {
      await vscode.window.showTextDocument(document);
      moveTo(5, 3); // on "Call the client"
      // The calendar opens beside the file, without asking anything yet.
      await vscode.commands.executeCommand('org.schedule');
      const editor = await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date' && vscode.window.activeTextEditor);
      const lines = () => editor.document.getText().trimEnd().split('\n');
      await until('the prompt of this command', () => lines().at(-1) === 'SCHEDULED Date+time [2026-05-18]:    => <2026-05-18 Mon>', 5000);
      assert.equal(lines()[0], '          April 2026                May 2026                June 2026');
      assert.notEqual(editor.viewColumn, vscode.ViewColumn.One);
      const day = () => {
        const cursor = editor.selection.active;
        return editor.document.lineAt(cursor.line).text.slice(cursor.character - 1, cursor.character + 1);
      };
      // The cursor follows once the editor has the new text.
      await until('the cursor on the day', () => day() === '18');
      // Its keys move the day; the cursor goes with it.
      await vscode.commands.executeCommand('org.calendar.day.later');
      await until('a day later', () => lines().at(-1).endsWith('=> <2026-05-19 Tue>') && day() === '19');
      // Typing gives an answer that ends with that day.
      await vscode.commands.executeCommand('org.calendar.type');
      await until('the entry to be scheduled', () => document.lineAt(6).text === 'SCHEDULED: <2026-05-19 Tue 09:00> DEADLINE: <2026-05-20 Wed>');
      await until('the calendar to close', () => !calendar());
    } finally {
      vscode.window.showInputBox = showInputBox;
    }
    await vscode.window.showTextDocument(document);
    await vscode.commands.executeCommand('undo');
    await until('the schedule undone', () => document.lineAt(6).text === 'DEADLINE: <2026-05-20 Wed>');
    await document.save();
  });

  it('keeps the text a date prompt starts with, typing after it, as Emacs does', async () => {
    const fs = require('node:fs');
    // Outside the workspace, so that the agenda does not see it.
    const file = path.join(fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'org-lsp-change-')), 'change.org');
    fs.writeFileSync(file, '* Meeting <2026-05-18 Mon 10:00>\n');
    const changed = await vscode.workspace.openTextDocument(file);
    const editor = await vscode.window.showTextDocument(changed);
    const started = [];
    const showInputBox = vscode.window.showInputBox;
    // ` fri` typed as VS Code types it: in place of the text selected,
    // all of it unless the extension says otherwise.
    vscode.window.showInputBox = async (options) => {
      started.push(options.value);
      const [start, end] = options.valueSelection ?? [0, options.value.length];
      return `${options.value.slice(0, start)} fri${options.value.slice(end)}`;
    };
    try {
      editor.selection = new vscode.Selection(0, 14, 0, 14);
      await vscode.commands.executeCommand('org.timestamp.change');
      await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date');
      await vscode.commands.executeCommand('org.calendar.type');
      // Emacs's minibuffer has the time in it, the cursor after it: `10:00 fri`.
      await until('the timestamp changed', () => changed.lineAt(0).text === '* Meeting <2026-05-22 Fri 10:00>');
      assert.deepEqual(started, ['10:00']);
    } finally {
      vscode.window.showInputBox = showInputBox;
    }
    await vscode.window.showTextDocument(changed);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.window.showTextDocument(document);
  });

  it('hides what Org hides, and shows it on the line being edited', async () => {
    const fs = require('node:fs');
    const file = path.join(workspace, 'links.org');
    fs.writeFileSync(file, '* Links\nRead [[https://example.org][the site]] now\n');
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await until('the link to be concealed', () =>
      JSON.stringify(api.concealed(editor)) === JSON.stringify(['[[https://example.org][', ']]']),
    );
    // On its line the link shows whole.
    editor.selection = new vscode.Selection(1, 3, 1, 3);
    await until('the link to show', () => api.concealed(editor).length === 0);
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await until('the link to be concealed again', () => api.concealed(editor).length === 2);
    await vscode.window.showTextDocument(document);
  });

  it('shows an entity as its character, and as it is on the line being edited', async () => {
    const fs = require('node:fs');
    const file = path.join(workspace, 'entities.org');
    fs.writeFileSync(file, '#+STARTUP: entitiespretty\n* Entities\nAn \\alpha here\n');
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await until('the entity shown as its character', () =>
      JSON.stringify(api.standIns(editor)) === JSON.stringify(['α']) && api.concealed(editor).includes('\\alpha'),
    );
    // On its line, the entity as it is, without its character.
    editor.selection = new vscode.Selection(2, 0, 2, 0);
    await until('the entity as it is', () => api.standIns(editor).length === 0 && api.concealed(editor).length === 0);
    // Gone with the entity.
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await editor.edit((edit) => edit.replace(new vscode.Range(2, 3, 2, 9), 'alpha'));
    await until('nothing in its place', () => api.standIns(editor).length === 0 && api.concealed(editor).length === 0);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.window.showTextDocument(document);
  });

  it('clocks in and out, and shows the clock in the status bar', async () => {
    const fs = require('node:fs');
    const file = path.join(workspace, 'clock.org');
    fs.writeFileSync(file, '* Clocked\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n');
    const clockDocument = await vscode.workspace.openTextDocument(file);
    const editor = await vscode.window.showTextDocument(clockDocument);
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    const lineOf = (number) => (number < clockDocument.lineCount ? clockDocument.lineAt(number).text : '');
    assert.equal(api.clock(), '');
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await vscode.commands.executeCommand('org.clock.in');
    await until('the clock line', () => lineOf(5) === 'CLOCK: [2026-05-18 Mon 10:30]');
    assert.equal(lineOf(4), ':LOGBOOK:');
    await until('the clock in the status bar', () => api.clock() === '[0:00/1:00] (Clocked)');
    await vscode.commands.executeCommand('org.clock.out');
    await until('the clock to stop', () => lineOf(5).endsWith('=>  0:00'));
    await until('the status bar without the clock', () => api.clock() === '');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    fs.unlinkSync(file);
    await vscode.window.showTextDocument(document);
  });

  it('indents a new line as Org does on RET, and a selection as org-indent-region', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'indent.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* Head\n- item\n      stray\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(1, 6);
    await vscode.commands.executeCommand('type', { text: '\n' });
    await until('the new line at the text of the item', () => editor.document.lineAt(2).text === '  ');
    await vscode.commands.executeCommand('type', { text: 'body' });
    editor.selection = new vscode.Selection(3, 0, 3, 5);
    await vscode.commands.executeCommand('editor.action.formatSelection');
    await until('the stray line re-indented', () => editor.document.lineAt(3).text === '  stray');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
  });

  it('shows a file with org-indent-mode indented by level', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'virtual.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('#+STARTUP: indent\n* A\ntext\n** B\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    await until('the lines indented', () =>
      JSON.stringify(api.virtualIndent(editor)) === JSON.stringify([[2, 2, 0], [3, 1, 1]]),
    );
    assert.equal(editor.document.lineAt(2).text, 'text', 'the file is not changed');
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await vscode.workspace.fs.delete(file);
  });

  it('colours the lines of headlines from edge to edge with org_fontify_whole_heading_line', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'whole.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A\ntext\n** B\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    await until('the headlines highlighted', () => api.lineHighlights(editor).length === 2);
    assert.deepEqual(api.lineHighlights(editor), [[0, 'headline.level1'], [2, 'headline.level2']]);
    const colors = vscode.extensions.getExtension('kristijanhusak.org-lsp').packageJSON.contributes.colors.map((color) => color.id);
    assert.ok(colors.includes('org.headlineLevel8Background'), 'a colour for each level');
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await vscode.workspace.fs.delete(file);
  });

  it('shows images in place of links to them, and the image whole on hover', async () => {
    // A red picture four pixels wide and two high.
    const png = 'iVBORw0KGgoAAAANSUhEUgAAAAQAAAACCAIAAADwyuo0AAAAEElEQVR4nGP4z8AARwzIHABvqgf5gNwAKAAAAABJRU5ErkJggg==';
    await vscode.workspace.fs.writeFile(vscode.Uri.file(path.join(workspace, 'red.png')), Buffer.from(png, 'base64'));
    const file = vscode.Uri.file(path.join(workspace, 'images.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* Pictures\nA [[file:red.png]] here.\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    moveTo(1, 5);
    await vscode.commands.executeCommand('org.link.preview');
    // As high as it is: smaller than a line.
    await until('the image shown', () => JSON.stringify(api.images(editor)) === JSON.stringify([['link', 1, 'red.png', 4, 2]]));
    const hovers = await vscode.commands.executeCommand('vscode.executeHoverProvider', file, new vscode.Position(1, 5));
    const shown = hovers.flatMap((hover) => hover.contents.map((content) => content.value ?? String(content)));
    const red = vscode.Uri.file(path.join(workspace, 'red.png')).toString();
    assert.ok(shown.includes(`![](${red}|width=4)`), `the image on hover: ${shown}`);
    // Again at the link: taken away.
    await vscode.commands.executeCommand('org.link.preview');
    await until('the image taken away', () => api.images(editor).length === 0);
    // Shown again, it follows the text before it, and goes with its own text.
    await vscode.commands.executeCommand('org.link.preview');
    await until('the image shown again', () => api.images(editor).length === 1);
    await editor.edit((edit) => edit.insert(new vscode.Position(0, 0), 'Intro\n'));
    assert.deepEqual(api.images(editor), [['link', 2, 'red.png', 4, 2]]);
    // Counts from a key's arguments: C-u C-u C-u takes every image away, C-u C-u shows the file's.
    await vscode.commands.executeCommand('org.link.preview', { count: 64 });
    await until('every image taken away', () => api.images(editor).length === 0);
    await vscode.commands.executeCommand('org.link.preview', { count: 16 });
    await until('the images of the file', () => JSON.stringify(api.images(editor)) === JSON.stringify([['link', 2, 'red.png', 4, 2]]));
    await editor.edit((edit) => edit.insert(new vscode.Position(2, 10), 'x'));
    assert.deepEqual(api.images(editor), []);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    // A file that opens with its images.
    const startup = vscode.Uri.file(path.join(workspace, 'startup-images.org'));
    await vscode.workspace.fs.writeFile(startup, Buffer.from('#+STARTUP: inlineimages\n[[file:red.png]]\n'));
    const opened = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(startup));
    await until('the image it opens with', () => JSON.stringify(api.images(opened)) === JSON.stringify([['link', 1, 'red.png', 4, 2]]));
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await vscode.workspace.fs.delete(startup);
  });

  it('opens a file as its startup says', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'startup.org'));
    const lines = ['#+STARTUP: content', '* A', ':PROPERTIES:', ':ID: 1', ':END:', 'text', '** B', 'b text', '* C', 'c text', ''];
    await vscode.workspace.fs.writeFile(file, Buffer.from(lines.join('\n')));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const hidden = () =>
      [...Array(editor.document.lineCount - 1).keys()].filter(
        (line) => !editor.visibleRanges.some((range) => range.start.line <= line && line <= range.end.line),
      );
    // The drawer and the headlines without children folded.
    await until('the folds it opens with', () => JSON.stringify(hidden()) === JSON.stringify([3, 4, 7, 9]));
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await vscode.workspace.fs.delete(file);
  });

  it('cycles folds as Org\'s TAB and S-TAB do', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'cycle.org'));
    const text = ['* A', 'text', '** B', 'b text', '** C', 'c text', '* D', 'd text', ''].join('\n');
    await vscode.workspace.fs.writeFile(file, Buffer.from(text));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the folds', async () => {
      const folds = await vscode.commands.executeCommand('vscode.executeFoldingRangeProvider', file);
      return folds?.length === 4;
    });
    const hidden = () =>
      [...Array(editor.document.lineCount - 1).keys()].filter(
        (line) => !editor.visibleRanges.some((range) => range.start.line <= line && line <= range.end.line),
      );
    const shows = (what, lines) => until(what, () => JSON.stringify(hidden()) === JSON.stringify(lines));
    moveTo(0);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('FOLDED', [1, 2, 3, 4, 5]);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('CHILDREN', [3, 5]);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('SUBTREE', []);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('FOLDED again', [1, 2, 3, 4, 5]);
    await vscode.commands.executeCommand('org-lsp.cycleGlobal');
    await shows('OVERVIEW', [1, 2, 3, 4, 5, 7]);
    await vscode.commands.executeCommand('org-lsp.cycleGlobal');
    await shows('CONTENTS', [3, 5, 7]);
    await vscode.commands.executeCommand('org-lsp.cycleGlobal');
    await shows('SHOW ALL', []);
    // Off a fold, Tab is Tab; and Shift+Up off a timestamp or headline selects.
    moveTo(1);
    await vscode.commands.executeCommand('org-lsp.cycle', { fallback: ['tab'] });
    await until('a tab typed', () => editor.document.lineAt(1).text !== 'text');
    moveTo(3, 2);
    await vscode.commands.executeCommand('org.shift.up', { fallback: ['cursorUpSelect'] });
    await until('the selection', () => editor.selection.active.line === 2 && !editor.selection.isEmpty);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
  });

  it('keeps archived subtrees closed as TAB and S-TAB open the rest', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'archived.org'));
    const text = ['* A', 'text', '** B :ARCHIVE:', 'b text', '** C', 'c text', ''].join('\n');
    await vscode.workspace.fs.writeFile(file, Buffer.from(text));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the folds', async () => {
      const folds = await vscode.commands.executeCommand('vscode.executeFoldingRangeProvider', file);
      return folds?.length === 3;
    });
    const hidden = () =>
      [...Array(editor.document.lineCount - 1).keys()].filter(
        (line) => !editor.visibleRanges.some((range) => range.start.line <= line && line <= range.end.line),
      );
    const shows = (what, lines) => until(what, () => JSON.stringify(hidden()) === JSON.stringify(lines));
    moveTo(0);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('FOLDED', [1, 2, 3, 4, 5]);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('CHILDREN', [3, 5]);
    await vscode.commands.executeCommand('org-lsp.cycle');
    await shows('SUBTREE, but the archived one', [3]);
    await vscode.commands.executeCommand('org-lsp.cycleGlobal');
    await shows('OVERVIEW', [1, 2, 3, 4, 5]);
    await vscode.commands.executeCommand('org-lsp.cycleGlobal');
    await shows('CONTENTS', [3, 5]);
    await vscode.commands.executeCommand('org-lsp.cycleGlobal');
    await shows('SHOW ALL, but the archived one', [3]);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
  });

  it('lists the keys of the editor at hand', async () => {
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    const shown = api.help(vscode.window.activeTextEditor);
    assert.ok(shown.some(([key, title]) => key === 'Ctrl+C Ctrl+T' && title === 'Change the TODO state'), JSON.stringify(shown));
    assert.ok(shown.some(([key]) => key === 'Tab'));
    assert.ok(!shown.some(([, title]) => title === 'Capture: finish'), 'the capture keys only in a capture buffer');
    // `emacs`, the default, leaves VS Code the keys it uses for text;
    // `emacs_full` has them too.
    const left = ['Shift+Right', 'Ctrl+A', 'Ctrl+X n s', 'Alt+Shift+Left', 'Ctrl+Enter'];
    for (const key of left) {
      assert.ok(!shown.some(([bound]) => bound === key), key);
    }
    await vscode.workspace.getConfiguration('org-lsp').update('keys', 'emacs_full', vscode.ConfigurationTarget.Workspace);
    const full = api.help(vscode.window.activeTextEditor);
    assert.ok(full.some(([key, title]) => key === 'Ctrl+C Ctrl+T' && title === 'Change the TODO state'), JSON.stringify(full));
    for (const key of left) {
      assert.ok(full.some(([bound]) => bound === key), key);
    }
    await vscode.workspace.getConfiguration('org-lsp').update('keys', 'custom', vscode.ConfigurationTarget.Workspace);
    assert.deepEqual(api.help(vscode.window.activeTextEditor), []);
    await vscode.workspace.getConfiguration('org-lsp').update('keys', undefined, vscode.ConfigurationTarget.Workspace);
  });

  it('colors the code of source, export and inline source blocks in its language', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'code.org'));
    const text = '* Code\n#+begin_src python\nimport os\n#+end_src\n#+BEGIN_EXPORT python\nimport sys\n#+END_EXPORT\n'
      + 'Run src_python[:results none]{import re} now src_python{f({1})} and.\n';
    await vscode.workspace.fs.writeFile(file, Buffer.from(text));
    await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const captured = await vscode.commands.executeCommand('_workbench.captureSyntaxTokens', file);
    const scopes = (piece) => (captured.find((token) => token.c.trim() === piece) || { t: '' }).t;
    for (const name of ['os', 'sys', 're']) {
      assert.match(scopes(name), /meta\.embedded\.(block|inline)\.python/, `${name}: ${scopes(name)}`);
    }
    assert.match(scopes('import'), /keyword\.control\.import\.python/);
    assert.doesNotMatch(scopes('now'), /python/, scopes('now'));
    assert.doesNotMatch(scopes('and.'), /python/, scopes('and.'));
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('makes a new line as Org\'s RET does, and Enter is Enter in code', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'return.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* TODO Write the report :work:\n#+begin_src sh\necho\n#+end_src\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(0, 12);
    await vscode.commands.executeCommand('org.return', { fallback: [{ command: 'type', args: { text: '\n' } }] });
    await until('the split', () => editor.document.lineAt(1).text === 'the report');
    assert.equal(editor.document.lineAt(0).text, '* TODO Write' + ' '.repeat(59) + ':work:');
    assert.deepEqual([editor.selection.active.line, editor.selection.active.character], [1, 0]);
    moveTo(3, 4);
    await vscode.commands.executeCommand('org.return', { fallback: [{ command: 'type', args: { text: '\n' } }] });
    await until('a plain new line in code', () => editor.document.lineAt(4).text.trim() === '' && editor.document.lineAt(5).text === '#+end_src');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('inserts items and headings as M-RET and its neighbours do', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'meta.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* TODO Write :work:\n- one\n** Sub\n* Next\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(1, 5);
    await vscode.commands.executeCommand('org.meta.return');
    await until('the new item', () => editor.document.lineAt(2).text === '- ');
    assert.deepEqual([editor.selection.active.line, editor.selection.active.character], [2, 2]);
    moveTo(0, 0);
    await vscode.commands.executeCommand('org.insert.todo.heading.after');
    await until('the TODO heading after the subtree', () => editor.document.lineAt(4).text === '* TODO ');
    assert.deepEqual([editor.selection.active.line, editor.selection.active.character], [4, 7]);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('promotes, demotes and moves subtrees with the meta keys', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'structure.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A\n** B\ntext\n*** C\n** D\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(1, 0);
    await vscode.commands.executeCommand('org.shiftmeta.right');
    await until('the subtree demoted', () => editor.document.lineAt(1).text === '*** B' && editor.document.lineAt(3).text === '**** C');
    await vscode.commands.executeCommand('org.meta.left');
    await until('the headline promoted', () => editor.document.lineAt(1).text === '** B');
    await vscode.commands.executeCommand('org.meta.down');
    await until('the subtree moved down', () => editor.document.getText() === '* A\n** D\n** B\ntext\n**** C\n');
    assert.equal(editor.selection.active.line, 2, 'the cursor goes with it');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('indents, moves and cycles list items with the meta and shifted keys', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'items.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('- a\n- b\n  - c\n- d\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(1, 2);
    await vscode.commands.executeCommand('org.shiftmeta.right');
    await until('the item indented with its child', () => editor.document.getText() === '- a\n  - b\n    - c\n- d\n');
    await vscode.commands.executeCommand('org.shiftmeta.left');
    await until('and outdented back', () => editor.document.getText() === '- a\n- b\n  - c\n- d\n');
    await vscode.commands.executeCommand('org.meta.down');
    await until('the item moved down', () => editor.document.getText() === '- a\n- d\n- b\n  - c\n');
    assert.equal(editor.selection.active.line, 2, 'the cursor goes with it');
    await vscode.commands.executeCommand('org.shift.right');
    await until('the bullets cycled', () => editor.document.getText() === '+ a\n+ d\n+ b\n  - c\n');
    await vscode.commands.executeCommand('org.shift.up');
    await until('the cursor on the item above', () => editor.selection.active.line === 1);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('cuts, copies and pastes subtrees through the clipboard', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'clipboard.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A\n** B\ntext\n** C\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(1, 0);
    await vscode.commands.executeCommand('org.copy.subtree');
    await until('the subtree copied', async () => (await vscode.env.clipboard.readText()) === '** B\ntext\n');
    await vscode.commands.executeCommand('org.cut.subtree');
    await until('the subtree cut', () => editor.document.getText() === '* A\n** C\n');
    await vscode.env.clipboard.writeText('* X\n');
    moveTo(0, 0);
    await vscode.commands.executeCommand('org.paste.subtree');
    await until('the clipboard pasted', () => editor.document.getText() === '* X\n* A\n** C\n');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('sets tags a key at a time until RET', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'tags.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('#+TAGS: work(w) home(h)\n* Task\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    const showQuickPick = vscode.window.showQuickPick;
    const keys = ['w', 'h', '\r'];
    const asked = [];
    vscode.window.showQuickPick = async (items, options) => {
      asked.push(options.placeHolder);
      const key = keys.shift();
      return items.find((item) => item.key === key);
    };
    try {
      moveTo(1, 0);
      await vscode.commands.executeCommand('org.tags.set');
      await until('the tags set', () => editor.document.lineAt(1).text === '* Task' + ' '.repeat(60) + ':work:home:');
      assert.deepEqual(asked, ['Tags', 'Tags :work:', 'Tags :work:home:']);
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('keeps the tags a headline has as tags are typed after them, as Emacs does', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'typed-tags.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A :zeta:\n* B :old:\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    // Typed into VS Code's own box, once it shows.
    let shown = false;
    const made = vscode.window.createQuickPick;
    vscode.window.createQuickPick = (...args) => {
      const pick = made(...args);
      pick.onDidChangeActive(() => (shown = true));
      return pick;
    };
    const type = (...args) => require('node:child_process').execFileSync('xdotool', args);
    try {
      moveTo(1, 2);
      void vscode.commands.executeCommand('org.tags.set');
      await until('the tags asked for', () => shown);
      // Emacs's minibuffer has `:old:` in it, the cursor after it.
      type('type', '--delay', '40', 'new');
      await new Promise((resolve) => setTimeout(resolve, 300));
      type('key', 'Return');
      await until('the tags set', () => editor.document.lineAt(1).text === '* B' + ' '.repeat(65) + ':old:new:');
    } finally {
      vscode.window.createQuickPick = made;
    }
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('works on the selection as Emacs on its region', async function () {
    this.timeout(90000);
    // Through the server's Emacs.
    if (!require('node:child_process').spawnSync('emacs', ['--version']).stdout) {
      this.skip();
    }
    const file = vscode.Uri.file(path.join(workspace, 'region.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A\n* B\none\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    editor.selection = new vscode.Selection(0, 0, 2, 0);
    await vscode.commands.executeCommand('org.meta.right');
    await until('both headlines demoted', () => editor.document.getText() === '** A\n** B\none\n', 60000);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('shows column view over the headlines', async function () {
    this.timeout(90000);
    // Through the server's Emacs.
    if (!require('node:child_process').spawnSync('emacs', ['--version']).stdout) {
      this.skip();
    }
    const file = vscode.Uri.file(path.join(workspace, 'columns.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('#+COLUMNS: %25ITEM %TODO\n* Project\n** Task\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    moveTo(0);
    await vscode.commands.executeCommand('org.columns');
    await until('column view', () => api.columnsShown(editor).length === 2, 60000);
    assert.deepEqual(api.columnsShown(editor)[1], [2, '** Task                   |      |']);
    assert.equal(api.columns(), '$(table) [ITEM] | TODO');
    // Right picks the TODO column; `n` its next state.
    moveTo(2);
    await vscode.commands.executeCommand('org-lsp.columns.right');
    assert.equal(api.columns(), '$(table) ITEM | [TODO]');
    await vscode.commands.executeCommand('org.columns.next');
    await until('the next state', () => editor.document.lineAt(2).text === '** TODO Task', 60000);
    await until('the view following', () => api.columnsShown(editor)[1]?.[1] === '** Task                   | TODO |', 60000);
    await vscode.commands.executeCommand('org.columns.quit');
    await until('column view off', () => api.columnsShown(editor).length === 0);
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('sorts the children of an entry by the key picked', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'sorting.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* Parent\n** b\n** C\ntext\n** a\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    const showQuickPick = vscode.window.showQuickPick;
    const asked = [];
    vscode.window.showQuickPick = async (items, options) => {
      asked.push(options.placeHolder);
      return items.find((item) => item.key === 'A');
    };
    try {
      moveTo(0, 3);
      await vscode.commands.executeCommand('org.sort');
      await until('the children sorted', () => editor.document.getText() === '* Parent\n** C\ntext\n** b\n** a\n');
      assert.deepEqual(asked, ['Sort children']);
      assert.equal(editor.selection.active.line, 0);
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('folds a subtree yanked at the start of a line', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'yank.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A\na body\n* B\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    await vscode.env.clipboard.writeText('* N\nnew body\n');
    moveTo(2, 0);
    await vscode.commands.executeCommand('org.yank');
    await until('the subtree yanked', () => editor.document.getText() === '* A\na body\n* N\nnew body\n* B\n');
    assert.equal(editor.selection.active.line, 4, 'the cursor on the line after it');
    const visible = (line) => editor.visibleRanges.some((range) => range.start.line <= line && line <= range.end.line);
    await until('the subtree folded', () => visible(2) && !visible(3) && visible(4));
    await vscode.commands.executeCommand('editor.unfoldAll');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('makes a sparse tree: everything folded but the matches', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'sparse.org'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('* A\nan apple\n* B :work:\nnothing\n** C\nApple pie\n* D\nnone\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    const showQuickPick = vscode.window.showQuickPick;
    const showInputBox = vscode.window.showInputBox;
    vscode.window.showQuickPick = async (items) => items.find((item) => item.key === 'r');
    vscode.window.showInputBox = async () => 'apple';
    try {
      moveTo(0, 0);
      await vscode.commands.executeCommand('org.sparse.tree');
      await until('the matches highlighted', () => api.sparse(editor).length === 2);
      assert.deepEqual(api.sparse(editor), ['apple', 'Apple']);
      const visible = (line) => editor.visibleRanges.some((range) => range.start.line <= line && line <= range.end.line);
      await until('the rest folded', () => visible(1) && visible(5) && !visible(7));
      await editor.edit((builder) => builder.insert(new vscode.Position(0, 0), ''));
      await editor.edit((builder) => builder.insert(new vscode.Position(7, 0), 'x'));
      await until('the highlights gone with the change', () => api.sparse(editor).length === 0);
      // A sparse tree by tags keeps its highlights through changes, as
      // `org-scan-tags` does, until `C-c C-c` takes them away.
      vscode.window.showQuickPick = async (items) => items.find((item) => item.key === 'm');
      vscode.window.showInputBox = async () => 'work';
      moveTo(0, 0);
      await vscode.commands.executeCommand('org.sparse.tree');
      await until('the tagged headlines highlighted', () => api.sparse(editor).length === 2);
      await editor.edit((builder) => builder.insert(new vscode.Position(7, 0), 'y'));
      await new Promise((resolve) => setTimeout(resolve, 300));
      assert.deepEqual(api.sparse(editor), ['*', '**'], 'the highlights kept through the change');
      await vscode.commands.executeCommand('org.checkbox.toggle');
      await until('the highlights taken away by C-c C-c', () => api.sparse(editor).length === 0);
      assert.equal(editor.document.lineAt(2).text, '* B :work:');
    } finally {
      vscode.window.showQuickPick = showQuickPick;
      vscode.window.showInputBox = showInputBox;
    }
    await vscode.commands.executeCommand('editor.unfoldAll');
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.window.showTextDocument(document);
  });

  it('archives a subtree into the archive file, and asks about each done one', async () => {
    const file = vscode.Uri.file(path.join(workspace, 'archive.org'));
    const archive = vscode.Uri.file(path.join(workspace, 'archive.org_archive'));
    await vscode.workspace.fs.writeFile(file, Buffer.from('Intro\n* DONE Old\nNotes.\n* DONE Older\n* TODO New\n'));
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    await until('the server', async () => (await tokens(file)) !== undefined);
    moveTo(1);
    await vscode.commands.executeCommand('org.archive.subtree');
    await until('the subtree archived', () => editor.document.lineAt(1).text === '* DONE Older');
    const archived = Buffer.from(await vscode.workspace.fs.readFile(archive)).toString();
    assert.ok(archived.includes('\n* DONE Old\n:PROPERTIES:\n'), archived);
    const asked = [];
    const showQuickPick = vscode.window.showQuickPick;
    vscode.window.showQuickPick = async (items, options) => {
      asked.push(options.placeHolder);
      return items.find((item) => item.key === 'y');
    };
    try {
      moveTo(0);
      await vscode.commands.executeCommand('org.archive.done');
      await until('the done subtree archived', () => editor.document.getText() === 'Intro\n* TODO New\n');
      assert.deepEqual(asked, ['Move subtree to archive? (no open TODO items)']);
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
    await vscode.workspace.fs.delete(file);
    await vscode.workspace.fs.delete(archive);
    await vscode.window.showTextDocument(document);
  });

  it('changes the part of a timestamp at the cursor', async () => {
    moveTo(6, 19); // on the day of "DEADLINE: <2026-05-20 Wed>"
    await vscode.commands.executeCommand('org.timestamp.up');
    await until('a day later', () => document.lineAt(6).text === 'DEADLINE: <2026-05-21 Thu>');
    moveTo(6, 16); // on the month
    await vscode.commands.executeCommand('org.timestamp.down');
    await until('a month earlier', () => document.lineAt(6).text === 'DEADLINE: <2026-04-21 Tue>');
    moveTo(6, 16);
    await vscode.commands.executeCommand('org.timestamp.up');
    await until('the old month', () => document.lineAt(6).text === 'DEADLINE: <2026-05-21 Thu>');
    moveTo(6, 19);
    await vscode.commands.executeCommand('org.timestamp.down');
    await until('the old date', () => document.lineAt(6).text === 'DEADLINE: <2026-05-20 Wed>');
    await document.save();
  });
});

describe('The agenda', () => {
  it('opens as a read-only document of the server', async () => {
    await oneGroup();
    await vscode.commands.executeCommand('org.agenda.list');
    const editor = await agenda('Week-agenda (W21):');
    assert.deepEqual(await layout(), [BELOW, 2], 'in a group below');
    assert.equal(editor.document.uri.toString(), 'org-agenda:/Org%20Agenda');
    assert.equal(editor.document.languageId, 'org-agenda');
    assert.deepEqual(editor.document.getText().split('\n').slice(0, 5), [
      'Week-agenda (W21):',
      'Monday     18 May 2026 W21',
      '               8:00...... ----------------',
      '  work:        9:00...... Scheduled:  TODO [#A] Write report            :office:',
      '              10:00...... ----------------',
    ]);
    await until('the cursor on today', () => activeLine() === 'Monday     18 May 2026 W21');
  });

  it('is highlighted by the server', async () => {
    const uri = vscode.window.activeTextEditor.document.uri;
    const found = await until('agenda tokens', () => tokens(uri));
    assert.deepEqual(found[0], [0, 'Week-agenda (W21):', 'agendaStructure', []]);
    assert.deepEqual(found[1], [1, 'Monday     18 May 2026 W21', 'agendaDate', ['today']]);
    assert.ok(found.some(([line, text, type]) => line === 3 && text === 'TODO' && type === 'todoKeyword'));
  });

  it('shows the entry of a line on hover', async () => {
    const uri = vscode.window.activeTextEditor.document.uri;
    const hovers = await until('a hover', async () => {
      const found = await vscode.commands.executeCommand('vscode.executeHoverProvider', uri, new vscode.Position(3, 30));
      return found && found.length > 0 && found;
    });
    const text = hovers[0].contents.map((content) => content.value ?? content).join('\n');
    assert.ok(text.includes('* TODO [#A] Write report :office:'), text);
    assert.ok(text.includes('SCHEDULED: <2026-05-18 Mon 09:00>'), text);
  });

  it('highlights the line of the clocked entry from edge to edge', async () => {
    const editor = vscode.window.activeTextEditor;
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    moveTo(3);
    await vscode.commands.executeCommand('org.clock.in');
    await until('the clocked line highlighted', () => api.lineHighlights(editor).length > 0);
    assert.deepEqual(api.lineHighlights(editor), [[3, 'agendaClocking']]);
    // Canceled, the clock leaves nothing behind in the file.
    await vscode.commands.executeCommand('org.clock.cancel');
    await until('the highlight to go', () => api.lineHighlights(editor).length === 0);
    await agenda('Week-agenda (W21):');
  });

  it('moves through time, with the cursor following the new text', async () => {
    await vscode.commands.executeCommand('org.agenda.later');
    await agenda('Week-agenda (W22):');
    await until('the cursor on the first day', () => activeLine() === 'Monday     25 May 2026 W22');
    await vscode.commands.executeCommand('org.agenda.today');
    await agenda('Week-agenda (W21):');
    // From a short view to a long one: the day under the cursor is far below
    // the old last line.
    const wednesday = activeText().split('\n').findIndex((line) => line.startsWith('Wednesday'));
    moveTo(wednesday + 1);
    await vscode.commands.executeCommand('org.agenda.day');
    await agenda('Day-agenda (W21):');
    assert.equal(vscode.window.activeTextEditor.document.lineCount, 4);
    await vscode.commands.executeCommand('org.agenda.month');
    await agenda('Month-agenda (W18-W22):');
    await until('the cursor on the same day in the month', () => activeLine() === 'Wednesday  20 May 2026');
    await vscode.commands.executeCommand('org.agenda.week');
    await agenda('Week-agenda (W21):');
  });

  it('goes to a date picked in the calendar', async () => {
    const calendar = () => vscode.window.visibleTextEditors.find((editor) => editor.document.languageId === 'org-date');
    await vscode.commands.executeCommand('org.agenda.goto.date');
    const editor = await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date' && vscode.window.activeTextEditor);
    await until('the prompt on today', () => editor.document.getText().trimEnd().endsWith('Date+time [2026-05-18]:    => <2026-05-18 Mon>'));
    await vscode.commands.executeCommand('org.calendar.week.later');
    await until('a week later', () => editor.document.getText().trimEnd().endsWith('=> <2026-05-25 Mon>'));
    await vscode.commands.executeCommand('org.calendar.day.later');
    await until('a day later', () => editor.document.getText().trimEnd().endsWith('=> <2026-05-26 Tue>'));
    await vscode.commands.executeCommand('org.calendar.select');
    await until('the calendar to close', () => !calendar());
    await agenda('Week-agenda (W22):');
    await until('the cursor on the day', () => activeLine() === 'Tuesday    26 May 2026');
    await vscode.commands.executeCommand('org.agenda.today');
    await agenda('Week-agenda (W21):');
    await until('the cursor on today', () => activeLine() === 'Monday     18 May 2026 W21');
  });

  it('changes an entry from its line and follows the file', async () => {
    const lines = activeText().split('\n');
    const entry = lines.findIndex((line) => line.endsWith('Sched. 2x:  TODO Water plants'));
    assert.ok(entry > 0, 'the repeating entry is listed');
    moveTo(entry);
    await vscode.commands.executeCommand('org.todo.next');
    // Done once, a repeating entry moves on to its next date and is open again.
    const home = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(workspace, 'home.org')));
    await until('the entry to repeat', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-23 Sat +1w>');
    assert.equal(home.lineAt(0).text, '* TODO Water plants');
    // `auto_save`: a change made from the agenda is saved.
    const fs = require('node:fs');
    await until('the change to be saved', () => !home.isDirty && fs.readFileSync(home.uri.fsPath, 'utf8').includes('<2026-05-23 Sat +1w>'));
    // `s`, Org's `org-save-all-org-buffers`, saves what is left unsaved.
    const edit = new vscode.WorkspaceEdit();
    edit.insert(home.uri, home.lineAt(home.lineCount - 1).range.end, '\n');
    await vscode.workspace.applyEdit(edit);
    assert.ok(home.isDirty);
    await vscode.commands.executeCommand('org.agenda.save');
    await until('the file to be saved', () => !home.isDirty);
    await until('the agenda to drop the old date', () => !activeText().includes('Sched. 2x:'));
  });

  it('changes priority and date from a line, which stays where it is with the cursor', async () => {
    const home = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(workspace, 'home.org')));
    const lines = activeText().split('\n');
    const row = lines.findIndex((line) => line.endsWith('Scheduled:  TODO Water plants'));
    moveTo(row, 14);
    // What Emacs says comes as a notification, or with
    // `org-lsp.messages` set to `statusBar` in the status bar.
    const said = [];
    const shown = [];
    const showInformationMessage = vscode.window.showInformationMessage;
    const setStatusBarMessage = vscode.window.setStatusBarMessage;
    vscode.window.showInformationMessage = (text) => {
      said.push(text);
      return Promise.resolve(undefined);
    };
    vscode.window.setStatusBarMessage = (text, ...rest) => {
      shown.push(text);
      return setStatusBarMessage(text, ...rest);
    };
    const settings = vscode.workspace.getConfiguration('org-lsp');
    try {
      await vscode.commands.executeCommand('org.priority.up');
      await until('the priority in the agenda', () => activeLine()?.endsWith('Scheduled:  TODO [#B] Water plants'));
      await settings.update('messages', 'statusBar', vscode.ConfigurationTarget.Global);
      await vscode.commands.executeCommand('org.priority.up');
      await until('the higher priority in the agenda', () => activeLine()?.endsWith('Scheduled:  TODO [#A] Water plants'));
      await vscode.commands.executeCommand('org.priority.down');
    } finally {
      vscode.window.showInformationMessage = showInformationMessage;
      vscode.window.setStatusBarMessage = setStatusBarMessage;
      await settings.update('messages', undefined, vscode.ConfigurationTarget.Global);
    }
    assert.deepEqual(said, ['Priority of current item set to B']);
    // The status bar also shows the line's outline path, when the agenda
    // hears where the cursor went (`org-agenda-show-outline-path`), at a
    // time of its own.
    assert.deepEqual(shown.filter((text) => text !== 'home.org'), ['Priority of current item set to A', 'Priority of current item set to B']);
    await until('the priority in the file', () => home.lineAt(0).text === '* TODO [#B] Water plants');
    await until('the priority in the agenda', () => activeLine()?.endsWith('Scheduled:  TODO [#B] Water plants'));
    // The entry moves to the next day. As in Emacs
    // (`org-agenda-show-new-time`), its line stays with the new date at its
    // end, and so does the cursor.
    const cursor = () => vscode.window.activeTextEditor.selection.active;
    await vscode.commands.executeCommand('org.date.later');
    await until('the date in the file', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-24 Sun +1w>');
    await until('the new date on the line', () => activeLine()?.endsWith(' => <2026-05-24 Sun +1w> '));
    assert.ok(activeLine().includes('Scheduled:  TODO [#B] Water'), activeLine());
    assert.deepEqual([cursor().line, cursor().character], [row, 14], 'the cursor stays');
    // Back as it was.
    await vscode.commands.executeCommand('org.date.earlier');
    await until('the old date', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-23 Sat +1w>');
    await until('the old date on the line', () => activeLine()?.endsWith(' => <2026-05-23 Sat +1w> '));
    assert.equal(cursor().line, row, 'the cursor stays');
    // A new priority writes the line anew, without the date.
    await vscode.commands.executeCommand('org.priority.down');
    await until('a lower priority', () => home.lineAt(0).text === '* TODO [#C] Water plants');
    await until('the lower priority in the agenda', () => activeLine()?.endsWith('TODO [#C] Water plants'));
    await vscode.commands.executeCommand('org.priority.down');
    await until('no priority', () => home.lineAt(0).text === '* TODO Water plants');
  });

  it('schedules the entry of a line with the calendar', async () => {
    const home = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(workspace, 'home.org')));
    const calendar = () => vscode.window.visibleTextEditors.find((editor) => editor.document.languageId === 'org-date');
    const showInputBox = vscode.window.showInputBox;
    vscode.window.showInputBox = async () => '2026-05-23';
    try {
      // The agenda may not show the last change of the test before yet.
      const entry = await until('the entry in the agenda', () => {
        const line = activeText().split('\n').findIndex((text) => text.endsWith('Scheduled:  TODO Water plants'));
        return line >= 0 && line + 1;
      });
      moveTo(entry - 1);
      // The calendar opens on the entry's date; RET takes the day a week
      // earlier.
      await vscode.commands.executeCommand('org.schedule');
      const editor = await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date' && vscode.window.activeTextEditor);
      const day = () => {
        const cursor = editor.selection.active;
        return editor.document.lineAt(cursor.line).text.slice(cursor.character - 1, cursor.character + 1);
      };
      await until('the cursor on the scheduled day', () => editor.document.getText().includes('[2026-05-23]') && day() === '23');
      await vscode.commands.executeCommand('org.calendar.week.earlier');
      await until('a week earlier', () => day() === '16');
      await vscode.commands.executeCommand('org.calendar.select');
      await until('the date in the file', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-16 Sat +1w>');
      await until('the calendar to close', () => !calendar());
      await agenda('Week-agenda (W21):');
      // The line stays, saying what was done, with the cursor on it.
      await until('the new date on the line', () => activeLine()?.endsWith(' S => Scheduled to <2026-05-16 Sat +1w> '));
      assert.equal(vscode.window.activeTextEditor.selection.active.line, entry - 1, 'the cursor stays');
      // Typed in the minibuffer, back to Saturday.
      await vscode.commands.executeCommand('org.schedule');
      await until('the calendar again', () => vscode.window.activeTextEditor?.document.languageId === 'org-date');
      await vscode.commands.executeCommand('org.calendar.type');
      await until('the old date in the file', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-23 Sat +1w>');
      await until('the calendar to close again', () => !calendar());
      await agenda('Week-agenda (W21):');
      await until('the old date on the line', () => activeLine()?.endsWith(' S => Scheduled to <2026-05-23 Sat +1w> '));
      assert.equal(vscode.window.activeTextEditor.selection.active.line, entry - 1, 'the cursor stays');
    } finally {
      vscode.window.showInputBox = showInputBox;
    }
    // Built again, the agenda shows the entry where it belongs.
    await vscode.commands.executeCommand('org.agenda.redo');
    await until('the agenda built again', () => !activeText().includes(' => '));
    assert.ok(activeText().includes('Scheduled:  TODO Water plants'), activeText());
  });

  it('goes to the entry of a line', async () => {
    const lines = activeText().split('\n');
    moveTo(lines.findIndex((line) => line.includes('Write report')));
    await vscode.commands.executeCommand('org.agenda.goto');
    // The editor may show the file before it moves the cursor there.
    await until('the cursor on the entry in its file', () => {
      const active = vscode.window.activeTextEditor;
      return active?.document.uri.toString() === workUri.toString() && active.selection.active.line === 1;
    });
  });

  it('goes to the entry in another group with TAB', async () => {
    await vscode.commands.executeCommand('org.agenda.list');
    await agenda('Week-agenda (W21):');
    const agendaGroup = vscode.window.tabGroups.activeTabGroup;
    const lines = activeText().split('\n');
    moveTo(lines.findIndex((line) => line.includes('Write report')));
    await vscode.commands.executeCommand('org.agenda.goto.other');
    await until('the cursor on the entry in another group', () => {
      const active = vscode.window.activeTextEditor;
      return (
        active?.document.uri.toString() === workUri.toString() &&
        active.selection.active.line === 1 &&
        vscode.window.tabGroups.activeTabGroup !== agendaGroup
      );
    });
    assert.ok(
      agendaGroup.tabs.some((tab) => tab.input instanceof vscode.TabInputText && tab.input.uri.scheme === 'org-agenda'),
      'the agenda keeps its group',
    );
  });

  it('shows the entry of the cursor\'s line in another group in follow mode', async () => {
    await vscode.commands.executeCommand('org.agenda.list');
    await agenda('Week-agenda (W21):');
    const agendaEditor = vscode.window.activeTextEditor;
    const lines = activeText().split('\n');
    const shownLine = () =>
      vscode.window.visibleTextEditors.find((editor) => editor.document.uri.toString() === workUri.toString())
        ?.selection.active.line;
    moveTo(lines.findIndex((line) => line.includes('Write report')));
    await vscode.commands.executeCommand('org.agenda.follow');
    await until('the entry in another group', () => shownLine() === 1);
    assert.equal(vscode.window.activeTextEditor, agendaEditor, 'the cursor stays in the agenda');
    const work = vscode.workspace.textDocuments.find((document) => document.uri.toString() === workUri.toString());
    const call = work.getText().split('\n').findIndex((line) => line.includes('Call the client'));
    moveTo(lines.findIndex((line) => line.includes('Call the client')));
    await until('the entry of the new line', () => shownLine() === call);
    assert.equal(vscode.window.activeTextEditor, agendaEditor, 'still in the agenda');
    // The outline path of its entry in the status bar (`org-agenda-show-outline-path`).
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    await until('the outline path', () => api.echoed() === 'work.org');
    await vscode.commands.executeCommand('org.agenda.follow');
  });

  it('lists TODO entries and refuses what makes no sense there', async () => {
    await vscode.commands.executeCommand('org.agenda.todos');
    await agenda('Global list of TODO items of type: ALL');
    const before = activeText();
    await vscode.commands.executeCommand('org.agenda.later');
    assert.equal(activeText(), before);
  });

  it('asks which view by key, as Emacs\'s agenda menu does', async () => {
    const showQuickPick = vscode.window.showQuickPick;
    let offered;
    vscode.window.showQuickPick = async (items, options) => {
      offered = [options.placeHolder, ...items.map((item) => item.label)];
      return items.find((item) => item.key === 't');
    };
    try {
      await vscode.commands.executeCommand('org.agenda.dispatch');
    } finally {
      vscode.window.showQuickPick = showQuickPick;
    }
    assert.deepEqual(offered, [
      'Press key for agenda command',
      '[a] Agenda for the current week or day',
      '[t] List of all TODO entries',
      '[m] Match a TAGS/PROP/TODO query',
      '[M] Like m, but only TODO entries',
      '[s] Search for keywords',
      '[S] Like s, but only TODO entries',
      '[#] List stuck projects',
    ]);
    await agenda('Global list of TODO items of type: ALL');
  });

  it('opens where org-lsp.winSplitMode says', async () => {
    const settings = vscode.workspace.getConfiguration('org-lsp');
    const open = async (mode) => {
      await settings.update('winSplitMode', mode, vscode.ConfigurationTarget.Global);
      await oneGroup();
      await vscode.commands.executeCommand('org.agenda.todos');
      await agenda('Global list of TODO items of type: ALL');
      return layout();
    };
    try {
      assert.deepEqual(await open('vsplit'), [BESIDE, 2], 'vsplit: beside');
      assert.deepEqual((await open('current'))[1], 1, 'current: in the same group');
      // Going back to the agenda uses the group it is in.
      await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(workUri));
      await vscode.commands.executeCommand('org.agenda.todos');
      await agenda('Global list of TODO items of type: ALL');
      assert.deepEqual((await layout())[1], 1, 'no second agenda');
      // float: a window of its own, which the main window's layout does not
      // count, though its group is one of VS Code's.
      await settings.update('winSplitMode', 'float', vscode.ConfigurationTarget.Global);
      await oneGroup();
      await vscode.commands.executeCommand('org.agenda.list');
      await until('the agenda in a window of its own', async () =>
        vscode.window.tabGroups.all.length === 2 && (await layout())[1] === 1,
      );
      await oneGroup();
    } finally {
      await settings.update('winSplitMode', undefined, vscode.ConfigurationTarget.Global);
    }
  });

  it('the org-roam buffer opens beside the note and follows the cursor', async () => {
    await oneGroup();
    const roam = path.join(workspace, 'roam');
    const roamShown = () =>
      vscode.window.visibleTextEditors.find((editor) => path.basename(editor.document.uri.fsPath) === 'org-roam-buffer.org');
    const a = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(path.join(roam, 'a.org')));
    await vscode.commands.executeCommand('org.roam.buffer.toggle');
    const shown = await until('the org-roam buffer', roamShown);
    assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, a.document.uri.fsPath, 'the cursor stays in the note');
    assert.equal(await layout().then((found) => found[0]), BESIDE);
    await until('its backlinks', () => shown.document.getText().includes('* Backlinks (1)'));
    // Another node: the buffer follows it.
    await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(path.join(roam, 'b.org')), {
      viewColumn: a.viewColumn,
    });
    moveTo(5);
    await until('the other node', () => roamShown()?.document.getText().startsWith('#+title: B note'));
    // The command again hides it.
    await vscode.commands.executeCommand('org.roam.buffer.toggle');
    await until('the buffer hidden', () => !roamShown());
    await oneGroup();
  });

  it('org-journal makes an entry in a file without the extension', async () => {
    await oneGroup();
    const journal = process.env.ORG_LSP_TEST_JOURNAL;
    await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(path.join(workspace, 'work.org')));
    await vscode.commands.executeCommand('org.journal.new.entry');
    // An Org file, though its name does not say so. Shown, once the
    // extension is done showing it: VS Code makes the editor active at the
    // top of the file, and gives it the selection asked for a moment later,
    // before `showTextDocument` returns.
    const api = vscode.extensions.getExtension('kristijanhusak.org-lsp').exports;
    const today = await until("today's file", () => {
      const editor = vscode.window.activeTextEditor;
      const shown = editor?.document.uri.fsPath === path.join(journal, '20260518');
      return shown && editor.document.languageId === 'org' && api.commands().running === 0 && editor;
    });
    assert.equal(today.document.getText(), '* Monday, 05/18/26\n** TODO carried\na note\n** 10:30 ');
    assert.deepEqual([today.selection.active.line, today.selection.active.character], [3, 9]);
    // The TODO carried over, its text folded as org-journal folds it.
    await until('the TODO folded', () => !today.visibleRanges.some((range) => range.start.line <= 2 && 2 <= range.end.line));
    assert.ok(today.visibleRanges.some((range) => range.start.line <= 1 && 1 <= range.end.line), 'its headline shows');
    await oneGroup();
  });
});
