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
    ]);
    assert.ok(actions[0].edit.has(workUri));
    await document.save();
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
    await vscode.commands.executeCommand('org.agenda.list');
    const editor = await agenda('Week-agenda (W21):');
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
    // From a short view to a long one: today is far below the old last line.
    const wednesday = activeText().split('\n').findIndex((line) => line.startsWith('Wednesday'));
    moveTo(wednesday + 1);
    await vscode.commands.executeCommand('org.agenda.day');
    await agenda('Day-agenda (W21):');
    assert.equal(vscode.window.activeTextEditor.document.lineCount, 4);
    await vscode.commands.executeCommand('org.agenda.month');
    await agenda('Month-agenda (W18-W22):');
    await until('the cursor on today in the month', () => activeLine() === 'Monday     18 May 2026 W21');
    await vscode.commands.executeCommand('org.agenda.week');
    await agenda('Week-agenda (W21):');
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
    assert.ok(home.isDirty, 'the change is left for the user to save');
    // `s`, Org's `org-save-all-org-buffers`, saves it.
    await vscode.commands.executeCommand('org.agenda.save');
    await until('the file to be saved', () => !home.isDirty);
    await until('the agenda to drop the old date', () => !activeText().includes('Sched. 2x:'));
  });

  it('changes priority and date from a line, and the cursor goes with the entry', async () => {
    const home = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(workspace, 'home.org')));
    const lines = activeText().split('\n');
    moveTo(lines.findIndex((line) => line.endsWith('Scheduled:  TODO Water plants')), 14);
    await vscode.commands.executeCommand('org.priority.up');
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
      moveTo(activeText().split('\n').findIndex((line) => line.endsWith('Scheduled:  TODO Water plants')));
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
      moveTo(activeText().split('\n').findIndex((line) => line.endsWith('Sched. 2x:  TODO Water plants')));
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
});
