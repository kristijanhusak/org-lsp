// The extension inside VS Code, against the real server.
const assert = require('node:assert/strict');
const path = require('node:path');
const vscode = require('vscode');

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

  it('highlights with the server\'s tokens', async () => {
    const found = await tokens(workUri);
    assert.deepEqual(found.slice(0, 6), [
      [0, '#+TITLE:', 'keyword', []],
      [1, '*', 'stars', ['level1']],
      [1, 'TODO', 'todoKeyword', []],
      [1, '[#A]', 'priority', []],
      [1, 'Write report', 'headline', ['level1']],
      [1, ':office:', 'tag', []],
    ]);
    assert.ok(found.some(([, text, type, modifiers]) => text === '*bold*' && type === 'text' && modifiers.includes('bold')));
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
    const showQuickPick = vscode.window.showQuickPick;
    vscode.window.showQuickPick = async (items) => {
      offered.push(...items.map((item) => item.label));
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

  it('hides what Org hides, and shows it on the line being edited', async () => {
    const fs = require('node:fs');
    const file = path.join(workspace, 'links.org');
    fs.writeFileSync(file, '* Links\nRead [[https://example.org][the site]] now\n');
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
    const api = vscode.extensions.getExtension('org-lsp.org-lsp').exports;
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

  it('clocks in and out, and shows the clock in the status bar', async () => {
    const fs = require('node:fs');
    const file = path.join(workspace, 'clock.org');
    fs.writeFileSync(file, '* Clocked\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n');
    const clockDocument = await vscode.workspace.openTextDocument(file);
    const editor = await vscode.window.showTextDocument(clockDocument);
    const api = vscode.extensions.getExtension('org-lsp.org-lsp').exports;
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
    const api = vscode.extensions.getExtension('org-lsp.org-lsp').exports;
    await until('the lines indented', () =>
      JSON.stringify(api.virtualIndent(editor)) === JSON.stringify([[2, 2, 0], [3, 1, 1]]),
    );
    assert.equal(editor.document.lineAt(2).text, 'text', 'the file is not changed');
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
    const api = vscode.extensions.getExtension('org-lsp.org-lsp').exports;
    const shown = api.help(vscode.window.activeTextEditor);
    assert.ok(shown.some(([key, title]) => key === 'Ctrl+C Ctrl+T' && title === 'Change the TODO state'), JSON.stringify(shown));
    assert.ok(shown.some(([key]) => key === 'Tab'));
    assert.ok(!shown.some(([, title]) => title === 'Capture: finish'), 'the capture keys only in a capture buffer');
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
    // `autoSave`: a change made from the agenda is saved.
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

  it('changes priority and date from a line, and the cursor goes with the entry', async () => {
    const home = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(workspace, 'home.org')));
    const lines = activeText().split('\n');
    moveTo(lines.findIndex((line) => line.endsWith('Scheduled:  TODO Water plants')), 14);
    // What Emacs says goes to the status bar.
    const said = [];
    const setStatusBarMessage = vscode.window.setStatusBarMessage;
    vscode.window.setStatusBarMessage = (text, ...rest) => {
      said.push(text);
      return setStatusBarMessage(text, ...rest);
    };
    try {
      await vscode.commands.executeCommand('org.priority.up');
    } finally {
      vscode.window.setStatusBarMessage = setStatusBarMessage;
    }
    assert.deepEqual(said, ['Priority of current item set to B']);
    await until('the priority in the file', () => home.lineAt(0).text === '* TODO [#B] Water plants');
    await until('the priority in the agenda', () => activeLine()?.endsWith('Scheduled:  TODO [#B] Water plants'));
    // The entry moves to the next day, and the cursor with it.
    await vscode.commands.executeCommand('org.date.later');
    await until('the date in the file', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-24 Sun +1w>');
    await until('the cursor on Sunday', () => {
      const editor = vscode.window.activeTextEditor;
      const line = editor.selection.active.line;
      return (
        line > 0 &&
        editor.document.lineAt(line).text.endsWith('TODO [#B] Water plants') &&
        editor.document.lineAt(line - 1).text === 'Sunday     24 May 2026'
      );
    });
    assert.equal(vscode.window.activeTextEditor.selection.active.character, 14, 'the cursor keeps its column');
    // Back as it was.
    await vscode.commands.executeCommand('org.date.earlier');
    await until('the old date', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-23 Sat +1w>');
    await until('the cursor on Saturday', () => activeLine()?.endsWith('TODO [#B] Water plants') && activeText().includes('Saturday   23 May 2026\n  home:'));
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
      await until('the agenda to show it today', () => activeText().includes('Sched. 2x:  TODO Water plants'));
      // Typed in the minibuffer, back to Saturday.
      moveTo(activeText().split('\n').findIndex((text) => text.endsWith('Sched. 2x:  TODO Water plants')));
      await vscode.commands.executeCommand('org.schedule');
      await until('the calendar again', () => vscode.window.activeTextEditor?.document.languageId === 'org-date');
      await vscode.commands.executeCommand('org.calendar.type');
      await until('the old date in the file', () => home.lineAt(1).text === 'SCHEDULED: <2026-05-23 Sat +1w>');
      await until('the calendar to close again', () => !calendar());
    } finally {
      vscode.window.showInputBox = showInputBox;
    }
    await agenda('Week-agenda (W21):');
    await until('the agenda without the old date', () => !activeText().includes('Sched. 2x:'));
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
});
