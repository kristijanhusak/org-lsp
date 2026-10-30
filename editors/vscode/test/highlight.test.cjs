// Highlighting in VS Code, on the shared sample (tests/e2e/highlight): the
// server's tokens reach VS Code for every look the extension contributes
// (`semanticTokenScopes`), in the Org file, the week's agenda, the TODO
// list and the date prompt; every look's scopes get a color or a font style
// from the color theme, and from every theme VS Code comes with, as VS Code
// probes them; what Org hides is
// concealed; and `org-lsp.headlineLevelColors` colors headlines by level.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const directory = process.env.ORG_LSP_TEST_HIGHLIGHT;
const sampleUri = vscode.Uri.file(path.join(directory, 'sample.org'));

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

/** Semantic tokens of `uri` as `{ line, text, type, modifiers }`. */
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
    out.push({ line, text: document.lineAt(line).text.slice(column, column + length), type: legend.tokenTypes[type], modifiers });
  }
  return out.length > 0 ? out : undefined;
}

function matches(selector, token) {
  const [type, modifier] = selector.split('.');
  return (type === '*' || type === token.type) && (modifier === undefined || token.modifiers.includes(modifier));
}

const extension = () => vscode.extensions.getExtension('kristijanhusak.org-lsp');

/** The looks the extension contributes, by language: `{ language: { selector: scopes } }`. */
function contributed() {
  const looks = {};
  for (const entry of extension().packageJSON.contributes.semanticTokenScopes) {
    looks[entry.language] = entry.scopes;
  }
  return looks;
}

/** The token colors of a color theme, with those of the themes it includes first. */
function themeRules(file) {
  const theme = JSON.parse(fs.readFileSync(file, 'utf8'));
  const included = theme.include ? themeRules(path.join(path.dirname(file), theme.include)) : [];
  let colors = theme.tokenColors ?? [];
  if (typeof colors === 'string') {
    colors = JSON.parse(fs.readFileSync(path.join(path.dirname(file), colors), 'utf8')).settings ?? [];
  }
  return included.concat(colors);
}

/** The color themes of the extensions installed, or of those VS Code comes with, as `{ name, file }`. */
function themes(builtin = false) {
  const found = [];
  for (const candidate of vscode.extensions.all) {
    if (builtin && !candidate.extensionPath.startsWith(path.join(vscode.env.appRoot, 'extensions'))) {
      continue;
    }
    const nls = path.join(candidate.extensionPath, 'package.nls.json');
    const labels = fs.existsSync(nls) ? JSON.parse(fs.readFileSync(nls, 'utf8')) : {};
    for (const theme of candidate.packageJSON.contributes?.themes ?? []) {
      const label = theme.label?.replace(/^%(.*)%$/, (_, key) => labels[key] ?? key);
      found.push({ name: theme.id ?? label, label, file: path.join(candidate.extensionPath, theme.path) });
    }
  }
  return found;
}

/** The file of the color theme VS Code shows. */
function shownTheme() {
  const name = vscode.workspace.getConfiguration('workbench').get('colorTheme');
  const found = themes().find((theme) => theme.name === name || theme.label === name);
  assert.ok(found, `no color theme ${name}`);
  return found;
}

/**
 * What the theme gives the first of `scopes` that it colors, as VS Code
 * probes a semantic token's scopes (`resolveScopes`): a rule's selector
 * matches a scope that is it or starts with it and a dot.
 */
function probe(rules, scopes) {
  for (const scope of scopes) {
    let style;
    for (const rule of rules) {
      const selectors = Array.isArray(rule.scope) ? rule.scope : String(rule.scope ?? '').split(',');
      for (const selector of selectors.map((part) => part.trim())) {
        if (selector && !selector.includes(' ') && (scope === selector || scope.startsWith(`${selector}.`))) {
          if (rule.settings?.foreground || rule.settings?.fontStyle) {
            style = { scope, selector, ...rule.settings };
          }
        }
      }
    }
    if (style) {
      return style;
    }
  }
  return undefined;
}

describe('Highlighting', () => {
  // The tokens VS Code got, by the language of their document.
  const seen = {};

  before(async () => {
    await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(sampleUri));
    seen.org = await until('the tokens of the sample', () => tokens(sampleUri));

    await vscode.commands.executeCommand('org.agenda.list');
    const agenda = await until('the agenda', () => {
      const editor = vscode.window.activeTextEditor;
      return editor?.document.languageId === 'org-agenda' && editor.document.lineAt(0).text.startsWith('Week-agenda') && editor;
    });
    seen['org-agenda'] = await until('the tokens of the agenda', () => tokens(agenda.document.uri));
    await vscode.commands.executeCommand('org.agenda.todos');
    await until('the TODO list', () => vscode.window.activeTextEditor?.document.lineAt(0).text.startsWith('Global list'));
    const todos = await until('the tokens of the TODO list', async () => {
      const found = await tokens(agenda.document.uri);
      return found?.some((token) => token.type === 'agendaClocking') && found;
    });
    seen['org-agenda'].push(...todos);

    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(sampleUri));
    const row = editor.document.getText().split('\n').indexOf('* Headline level one');
    editor.selection = new vscode.Selection(row, 0, row, 0);
    await vscode.commands.executeCommand('org.schedule');
    const calendar = await until('the calendar', () => vscode.window.activeTextEditor?.document.languageId === 'org-date' && vscode.window.activeTextEditor);
    seen['org-date'] = await until('the tokens of the calendar', async () => {
      const found = await tokens(calendar.document.uri);
      return found?.some((token) => token.type === 'calendarJournal') && found;
    });
    await vscode.commands.executeCommand('org.calendar.cancel');

    // Headlines in the colors of their level, then without them again.
    const settings = () => vscode.workspace.getConfiguration('org-lsp');
    await settings().update('headlineLevelColors', true, vscode.ConfigurationTarget.Global);
    seen.levels = await until('headlines in the colors of their level', async () => {
      const found = await tokens(sampleUri);
      return found?.some((token) => token.type === 'outline') && found;
    });
    seen.org.push(...seen.levels);
    await settings().update('headlineLevelColors', undefined, vscode.ConfigurationTarget.Global);
    seen.plain = await until('headlines in the color of headings', async () => {
      const found = await tokens(sampleUri);
      return found?.every((token) => token.type !== 'outline') && found;
    });
  });

  it('gets a token for every look it contributes', () => {
    const missing = [];
    const everything = Object.values(seen).flat();
    for (const selector of Object.keys(contributed().org)) {
      if (!everything.some((token) => matches(selector, token))) {
        missing.push(selector);
      }
    }
    assert.deepEqual(missing, [], 'looks no token of the sample shows');
  });

  it('contributes the same looks to the agenda and the date prompt', () => {
    const looks = contributed();
    assert.deepEqual(Object.keys(looks).sort(), ['org', 'org-agenda', 'org-date']);
    assert.deepEqual(looks['org-agenda'], looks.org);
    assert.deepEqual(looks['org-date'], looks.org);
  });

  it('gives every look a color or a font style from the color theme', () => {
    const shown = shownTheme();
    const bundled = themes(true);
    // Every theme VS Code comes with: among them Dark+ and Light+, the
    // defaults before 1.78, and Dark Modern and Light Modern after, which
    // users keep.
    for (const name of ['Dark+', 'Light+', 'Dark Modern', 'Light Modern']) {
      assert.ok(bundled.some((theme) => theme.name === name), `VS Code comes with ${name}`);
    }
    const plain = {};
    for (const { name, file } of [shown, ...bundled.filter((theme) => theme.file !== shown.file)]) {
      const rules = themeRules(file);
      assert.ok(rules.length > 0, `the token colors of ${name}`);
      for (const [selector, scopes] of Object.entries(contributed().org)) {
        if (!probe(rules, scopes)) {
          (plain[name] ??= []).push(`${selector} (${scopes.join(', ')})`);
        }
      }
    }
    assert.deepEqual(plain, {}, 'looks color themes give no color');
  });

  it('colors headlines by level with org-lsp.headlineLevelColors', () => {
    const at = (list, text) => list.filter((token) => token.text.trim() === text);
    const look = (token) => [token.type, ...token.modifiers].join('.');
    // On: the stars and title of each level are `outline` with the level.
    assert.deepEqual(at(seen.levels, 'Headline level one').map(look), ['outline.level1']);
    assert.deepEqual(at(seen.levels, 'Headline level two').map(look), ['outline.level2']);
    assert.deepEqual(at(seen.levels, '**').map(look), ['outline.level2']);
    // A done headline past its keyword keeps the look of done ones.
    assert.deepEqual(at(seen.levels, 'A finished task').map(look), ['headline.done']);
    // The levels take different scopes, none of them the heading's.
    const scopes = contributed().org;
    assert.notDeepEqual(scopes['outline.level1'], scopes['outline.level2']);
    for (let level = 1; level <= 8; level++) {
      assert.ok(!scopes[`outline.level${level}`].some((scope) => scope.startsWith('markup.heading')), `level ${level}`);
    }
    // Off: headlines and stars, as before.
    assert.deepEqual(at(seen.plain, 'Headline level one').map(look), ['headline.level1']);
    assert.deepEqual(at(seen.plain, '**').map(look), ['stars.level2']);
  });

  it("draws a habit's days in the backgrounds of org-habit's faces", async () => {
    await vscode.commands.executeCommand('org.agenda.list');
    const editor = await until('the agenda', () => {
      const shown = vscode.window.activeTextEditor;
      return shown?.document.languageId === 'org-agenda' && shown.document.lineAt(0).text.startsWith('Week-agenda') && shown;
    });
    const api = extension().exports;
    const lines = editor.document.getText().split('\n');
    // The face of the day marked `mark` on the habit's line.
    const day = (habit, mark) => {
      const line = lines.findIndex((text) => text.includes(habit));
      const character = lines[line].lastIndexOf(mark);
      return api.habitDays(editor).find(([l, c]) => l === line && c === character)?.[2];
    };
    await until("the habits' days", () => day('Shave', '!'));
    // Done on the 11th, due by the 12th, overdue since; due today; done on
    // the 15th, in its time to be done today; days to come paler.
    assert.equal(day('Water the plants', '*'), 'Clear');
    assert.equal(day('Water the plants', '!'), 'Overdue');
    assert.equal(day('Stretch', '!'), 'Alert');
    assert.equal(day('Shave', '!'), 'Ready');
    const faces = new Set(api.habitDays(editor).map(([, , face]) => face));
    for (const face of ['Clear', 'ClearFuture', 'Ready', 'ReadyFuture', 'Alert', 'AlertFuture', 'Overdue', 'OverdueFuture']) {
      assert.ok(faces.has(face), `a day in ${face}`);
    }
    // Each face's background is a color the extension contributes, Org's.
    const colors = extension().packageJSON.contributes.colors;
    const overdue = colors.find((color) => color.id === 'org.agendaHabitOverdueBackground');
    assert.deepEqual([overdue?.defaults.light, overdue?.defaults.dark], ['#f9372d', '#b22222']);
    for (const face of faces) {
      assert.ok(colors.some((color) => color.id === `org.agendaHabit${face}Background`), face);
    }
  });

  it('shows emphasis within emphasis with all its looks', async () => {
    // A theme's `markup.bold` turns italic off, its `markup.italic` bold:
    // VS Code takes one, and the extension paints the both of them.
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(sampleUri));
    const row = editor.document.getText().split('\n').findIndex((line) => line.startsWith('Nested: *bold with /italic/'));
    const shown = await until('/italic/ in *bold*', () => {
      const found = extension().exports.emphasis(editor).filter(([line]) => line === row);
      return found.length > 0 && found;
    });
    assert.ok(shown.some(([, text, looks]) => text.includes('italic') && looks === 'bold+italic'), JSON.stringify(shown));
    assert.ok(shown.every(([, text]) => !text.startsWith('bold with')), 'bold alone is left to the theme');
  });

  it('hides link brackets and emphasis markers', async () => {
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(sampleUri));
    editor.selection = new vscode.Selection(0, 0, 0, 0);
    const lines = editor.document.getText().split('\n');
    const row = lines.findIndex((line) => line.startsWith('Text in *bold*'));
    editor.revealRange(new vscode.Range(row, 0, row + 3, 0), vscode.TextEditorRevealType.InCenter);
    const api = extension().exports;
    const wanted = ['[[https://orgmode.org][', ']]', '[[', ']]', '*', '*', '/', '/', '_', '_', '+', '+', '=', '=', '~', '~'];
    await until(`${wanted.join(' ')} hidden`, () => {
      const hidden = api.concealed(editor);
      const counts = (list) => list.reduce((map, text) => map.set(text, (map.get(text) ?? 0) + 1), new Map());
      const have = counts(hidden);
      return [...counts(wanted)].every(([text, count]) => (have.get(text) ?? 0) >= count);
    });
  });
});
