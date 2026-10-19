// Pictures of the highlighting of the shared sample (tests/e2e/highlight),
// for a person to look at: the Org file, the week's agenda, the TODO list
// and the date prompt, each taken of the whole screen with ImageMagick's
// `import` into `ORG_LSP_TEST_SHOTS`. scripts/highlight-screenshots.py runs
// it (`npm test -- screenshots`, under `xvfb-run`); it checks nothing.
const { execFileSync } = require('node:child_process');
const path = require('node:path');
const vscode = require('vscode');

const directory = process.env.ORG_LSP_TEST_HIGHLIGHT;
const shots = process.env.ORG_LSP_TEST_SHOTS;
const sampleUri = vscode.Uri.file(path.join(directory, 'sample.org'));

async function until(what, probe, timeout = 20000) {
  const start = Date.now();
  for (;;) {
    const value = await probe();
    if (value) {
      return value;
    }
    if (Date.now() - start > timeout) {
      throw new Error(`timed out waiting for ${what}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** The whole screen, once what shows has settled, as NAME.png. */
async function shoot(name) {
  await pause(2500);
  execFileSync('import', ['-window', 'root', path.join(shots, `vscode-${name}.png`)]);
}

async function tokens(uri) {
  const found = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri);
  return found && found.data.length > 0;
}

describe('Screenshots', () => {
  it('takes the Org file, the agenda, the TODO list and the date prompt', async () => {
    for (const command of ['workbench.action.closeSidebar', 'workbench.action.closePanel', 'workbench.action.closeAuxiliaryBar']) {
      await vscode.commands.executeCommand(command);
    }
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(sampleUri));
    await until('the tokens of the sample', () => tokens(sampleUri));
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await shoot('sample');
    // The lines below the first screen.
    const lines = editor.document.lineCount;
    editor.revealRange(new vscode.Range(lines - 1, 0, lines - 1, 0), vscode.TextEditorRevealType.AtTop);
    editor.selection = new vscode.Selection(lines - 1, 0, lines - 1, 0);
    await shoot('sample-end');

    editor.selection = new vscode.Selection(0, 0, 0, 0);
    await vscode.commands.executeCommand('org.agenda.list');
    const agenda = await until('the agenda', () => {
      const shown = vscode.window.activeTextEditor;
      return shown?.document.languageId === 'org-agenda' && shown.document.lineAt(0).text.startsWith('Week-agenda') && shown;
    });
    await until('the tokens of the agenda', () => tokens(agenda.document.uri));
    await vscode.commands.executeCommand('workbench.action.joinAllGroups');
    await vscode.window.showTextDocument(agenda.document);
    await shoot('agenda');
    await vscode.commands.executeCommand('org.agenda.todos');
    await until('the TODO list', () => vscode.window.activeTextEditor?.document.lineAt(0).text.startsWith('Global list'));
    await shoot('todo');

    const file = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(sampleUri));
    const row = file.document.getText().split('\n').indexOf('* Headline level one');
    file.selection = new vscode.Selection(row, 0, row, 0);
    await vscode.commands.executeCommand('org.schedule');
    await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date');
    await shoot('calendar');
    await vscode.commands.executeCommand('org.calendar.cancel');
  }).timeout(120000);
});
