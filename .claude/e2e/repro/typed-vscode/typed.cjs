// Runs in VS Code's extension host (see run.mjs): each case opens a file,
// then types its steps into VS Code's window with xdotool, as the user
// types them: keys, text, and the answers to what VS Code asks (typed in
// its quick picks and input boxes, then Enter). Nothing is answered for
// the user: prompts and messages are only watched. Records what each step
// shows (text, cursor, messages, prompts, status bar, what `after` finds)
// and screenshots (`shot`), into OUT.
const assert = require('node:assert/strict');
const { execFile, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const ws = process.env.WS;
const shots = path.join(path.dirname(process.env.OUT), path.basename(process.env.OUT, '.json'));
let log = [];
let api;

async function until(what, probe, timeout = 10000) {
  const start = Date.now();
  for (;;) {
    const value = await probe();
    if (value) return value;
    if (Date.now() - start > timeout) throw new Error(`timed out waiting for ${what}`);
    await sleep(20);
  }
}

// ----- watching what VS Code shows

let prompts = [];
const openPrompt = () => prompts.find((p) => p.shown && !p.done);

function watch() {
  for (const kind of ['showInformationMessage', 'showWarningMessage', 'showErrorMessage']) {
    const show = vscode.window[kind];
    vscode.window[kind] = (message, ...rest) => {
      const items = rest.filter((r) => typeof r === 'string' || (r && typeof r.title === 'string'));
      log.push({ kind, message: String(message), ...(items.length ? { items: items.map((i) => i.title ?? i) } : {}) });
      const answered = show.call(vscode.window, message, ...rest);
      if (items.length) {
        const prompt = { kind: 'message', title: String(message), items, shown: true, done: false };
        prompts.push(prompt);
        Promise.resolve(answered).then(
          () => (prompt.done = true),
          () => (prompt.done = true),
        );
      }
      return answered;
    };
  }
  const status = vscode.window.setStatusBarMessage;
  vscode.window.setStatusBarMessage = (text, t) => {
    log.push({ kind: 'status', message: String(text) });
    return status.call(vscode.window, text, t);
  };
  const showQuickPick = vscode.window.showQuickPick;
  vscode.window.showQuickPick = (items, options = {}, token) => {
    const prompt = { kind: 'pick', title: options.placeHolder, items: [], shown: false, done: false };
    prompts.push(prompt);
    Promise.resolve(items).then((list) => {
      prompt.items = list;
      log.push({ kind: 'pick', prompt: options.placeHolder, items: list.map((i) => i.label ?? i).slice(0, 60) });
    });
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
    log.push({ kind: 'input', prompt: options.prompt, value: options.value });
    const validate = options.validateInput;
    const validateInput = async (value) => {
      prompt.value = value;
      prompt.shown = true;
      const result = await validate?.(value);
      if (result) log.push({ kind: 'preview', value, message: result.message ?? result });
      return result;
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
    Object.defineProperties(prompt, {
      kind: { get: () => (quickPick.items.some((item) => 'key' in item) ? 'pick' : 'text') },
      title: { get: () => quickPick.title ?? quickPick.placeholder },
      items: { get: () => quickPick.items },
    });
    prompts.push(prompt);
    let logged = false;
    quickPick.onDidChangeActive((items) => {
      prompt.active = items[0];
      prompt.shown = true;
      if (!logged) {
        logged = true;
        log.push({ kind: prompt.kind, prompt: prompt.title, items: quickPick.items.map((i) => i.label).slice(0, 60) });
        if (process.env.ICONS) log.push({ kind: 'icons', icons: quickPick.items.map((i) => JSON.stringify(i.iconPath)) });
      }
    });
    quickPick.onDidChangeValue((value) => (prompt.value = value));
    quickPick.onDidHide(() => (prompt.done = true));
    return quickPick;
  };
  // What the extension and its client write to their output channels.
  const channel = vscode.window.createOutputChannel;
  vscode.window.createOutputChannel = (...args) => {
    const made = channel.call(vscode.window, ...args);
    for (const m of ['append', 'appendLine', 'error', 'warn', 'info']) {
      if (typeof made[m] === 'function') {
        const f = made[m];
        made[m] = (text, ...rest) => {
          output.push(`[${args[0]}] ${String(text)}`);
          return f.call(made, text, ...rest);
        };
      }
    }
    return made;
  };
}
const output = [];

// ----- typing

function xdotool(...args) {
  return new Promise((resolve, reject) => execFile('xdotool', args, (error) => (error ? reject(error) : resolve())));
}
const typeText = (text) => xdotool('type', '--delay', '40', '--', text);

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

function keyName(key) {
  const named = { ' ': 'SPC', '\r': 'RET', '\t': 'TAB' }[key];
  if (named) return named;
  const code = key.length === 1 ? key.charCodeAt(0) : -1;
  return code > 0 && code < 32 ? `C-${String.fromCharCode(code + 96)}` : key;
}

function pick(items, given) {
  if (typeof given === 'string') return items.find((item) => item.key === given) ?? items.find((item) => item.label === given);
  const label = (item) => item.label.replace(/^\[[^\]]*\] /, '');
  return items.find((item) => label(item) === given.choice) ?? items.find((item) => label(item).includes(given.choice));
}

/** Answer `prompt` as the user does; `{ keys }` presses keys there instead. */
async function answer(prompt, given) {
  log.push({ kind: 'answer', prompt: prompt.title, given });
  if (given && typeof given === 'object' && given.keys) {
    await xdotool('key', '--delay', '60', ...xdotoolKeys(given.keys));
  } else if (prompt.kind === 'message') {
    // A notification's buttons: focused with the user's key, then Tab to the button.
    const title = (item) => String(item.title ?? item);
    const index = prompt.items.findIndex((item) => title(item) === given || title(item).toLowerCase().startsWith(String(given).toLowerCase()));
    assert.ok(index >= 0, `no button ${given} in ${prompt.items.map((i) => i.title ?? i)}`);
    await xdotool('key', 'ctrl+alt+shift+F12');
    await sleep(300);
    for (let i = 0; i <= index; i++) await xdotool('key', 'Tab');
    await xdotool('key', 'Return');
  } else if (prompt.kind === 'pick') {
    const wanted = pick(prompt.items, given);
    assert.ok(wanted, `no choice ${JSON.stringify(given)} in ${JSON.stringify(prompt.items.map((i) => i.label))}`);
    const typed = typeof given === 'string' ? (wanted.key !== undefined ? keyName(given) : given) : given.choice;
    await typeText(typed);
    await until(`${typed} taken`, () => prompt.active === wanted, 3000).catch(() =>
      assert.fail(`typed ${JSON.stringify(typed)}, Enter would pick ${JSON.stringify(prompt.active?.label)}, not ${JSON.stringify(wanted.label)}`),
    );
    await xdotool('key', 'Return');
  } else {
    if (given !== '') {
      await typeText(given);
      // After what the box starts with, if anything.
      await until(`${given} typed`, () => prompt.value?.endsWith(given), 3000).catch(() =>
        assert.fail(`typed ${JSON.stringify(given)}, VS Code has ${JSON.stringify(prompt.value)}`),
      );
      given = prompt.value;
      await until(`${given} typed`, () => true, 10).catch(() =>
        assert.fail(`typed ${JSON.stringify(given)}, VS Code has ${JSON.stringify(prompt.value)}`),
      );
    }
    if (prompt.kind === 'text' && given !== '') {
      await until(`${given} offered`, () => prompt.active?.label === given, 3000).catch(() =>
        log.push({ kind: 'note', message: `typed ${JSON.stringify(given)}, Enter takes ${JSON.stringify(prompt.active?.label)}` }),
      );
    }
    await xdotool('key', 'Return');
  }
  await until('the answer taken', () => prompt.done, 5000).catch(() => log.push({ kind: 'note', message: 'prompt still open' }));
}

function state() {
  const editor = vscode.window.activeTextEditor;
  return JSON.stringify([
    editor?.document.uri.toString(),
    editor?.selection.active,
    log.length,
    vscode.window.visibleTextEditors.map((shown) => shown.document.uri.toString()),
    vscode.workspace.textDocuments.map((document) => [document.uri.toString(), document.version, document.isDirty]),
    api.commands().running,
  ]);
}

async function settle() {
  let last;
  let same = 0;
  for (let round = 0; round < 200 && same < 3; round++) {
    await api.sync();
    await sleep(40);
    const now = state();
    same = now === last && api.commands().running === 0 ? same + 1 : 0;
    last = now;
  }
}

/** Answer what is asked with `answers` until nothing is under way. */
async function answered(answers, timeout = 60000) {
  for (;;) {
    const prompt = await until('the command to finish or ask', () => openPrompt() ?? (api.commands().running === 0 && 'done'), timeout).catch(
      () => 'timeout',
    );
    if (prompt === 'timeout') {
      log.push({ kind: 'note', message: 'still running' });
      return;
    }
    if (prompt === 'done') {
      await settle();
      if (!openPrompt()) return;
      continue;
    }
    if (answers.length === 0) {
      log.push({ kind: 'unanswered', prompt: prompt.title, items: (prompt.items ?? []).map((i) => i.label ?? i.title ?? i).slice(0, 40) });
      await xdotool('key', 'Escape');
      await sleep(300);
      if (prompt.kind === 'message') prompt.done = true;
      continue;
    }
    await answer(prompt, answers.shift());
  }
}

function snapshot(label) {
  const editor = vscode.window.activeTextEditor;
  const text = editor?.document.getText();
  return {
    label,
    active: editor ? (editor.document.uri.scheme === 'file' ? path.relative(ws, editor.document.uri.fsPath) : editor.document.uri.toString()) : null,
    cursor: editor ? [editor.selection.active.line, editor.selection.active.character] : null,
    text: text && text.length > 4000 ? text.slice(0, 4000) + '…' : text,
    log,
  };
}

exports.run = async function run() {
  watch();
  api = await vscode.extensions.getExtension('kristijanhusak.org-lsp').activate();
  const cases = require(process.env.CASES);
  const only = process.env.ONLY;
  const results = [];
  const base = vscode.workspace.getConfiguration('org-lsp').get('settings') ?? {};
  fs.mkdirSync(shots, { recursive: true });
  const ctx = {
    vscode, api, ws, sleep, path, fs, until, settle, xdotool, typeText,
    read: (name) => (fs.existsSync(path.join(ws, name)) ? fs.readFileSync(path.join(ws, name), 'utf8') : '<none>'),
    shot: (name) => execFileSync('import', ['-display', process.env.DISPLAY, '-window', 'root', path.join(shots, `${name}.png`)]),
    editor: () => vscode.window.activeTextEditor,
  };
  for (const c of cases) {
    if (only && !c.name.includes(only)) continue;
    log = [];
    prompts = [];
    const result = { name: c.name, steps: [] };
    results.push(result);
    try {
      for (const [name, text] of Object.entries(c.files ?? {})) {
        fs.mkdirSync(path.dirname(path.join(ws, name)), { recursive: true });
        fs.writeFileSync(path.join(ws, name), text);
      }
      // The case's settings over the folder's.
      const wanted = { ...base, ...(c.settings ?? {}) };
      if (JSON.stringify(wanted) !== JSON.stringify(vscode.workspace.getConfiguration('org-lsp').get('settings'))) {
        await vscode.workspace.getConfiguration('org-lsp').update('settings', wanted, vscode.ConfigurationTarget.Workspace);
        await vscode.commands.executeCommand('org-lsp.restartServer');
        await sleep(800);
      }
      if (c.file) {
        const uri = vscode.Uri.file(path.join(ws, c.file));
        if (c.content !== undefined) fs.writeFileSync(uri.fsPath, c.content);
        const doc = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(doc, { preview: false });
        for (let i = 0; i < 100; i++) {
          const s = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', uri);
          if (s !== undefined) break;
          await sleep(100);
        }
        await sleep(300);
      }
      for (const step of c.steps ?? []) {
        log = [];
        if (step.at !== undefined) {
          const e = vscode.window.activeTextEditor;
          let [l, ch] = Array.isArray(step.at) ? step.at : [];
          if (typeof step.at === 'string') {
            const lines = e.document.getText().split('\n');
            l = lines.findIndex((t) => t.includes(step.at));
            ch = lines[l].indexOf(step.at) + (step.column ?? 0);
          }
          e.selection = new vscode.Selection(l, ch, l, ch);
          await settle();
        }
        if (step.select) {
          const [l, ch, l2, ch2] = step.select;
          vscode.window.activeTextEditor.selection = new vscode.Selection(l, ch, l2, ch2);
          await settle();
        }
        const answers = [...(step.answers ?? [])].map((a) => (typeof a === 'string' ? a.replaceAll('$WS', ws) : a));
        if ((step.keys || step.type || step.palette) && !step.nofocus) {
          await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
        }
        if (step.type) {
          await typeText(step.type);
          await sleep(200);
        }
        if (step.keys) {
          for (const key of [].concat(step.keys)) {
            await xdotool('key', '--delay', '50', ...xdotoolKeys(key));
            await sleep(step.between ?? 150);
          }
        }
        // A command from the Command Palette, typed by its title.
        if (step.palette) {
          await xdotool('key', 'F1');
          await sleep(500);
          await typeText(step.palette);
          await sleep(700);
          await xdotool('key', 'Return');
          await sleep(300);
        }
        if (step.run) {
          try {
            const r = await step.run(ctx);
            if (r !== undefined) log.push({ kind: 'run', value: r });
          } catch (error) {
            log.push({ kind: 'run failed', message: String(error && error.message) });
          }
        }
        try {
          await answered(answers, step.timeout ?? 30000);
        } catch (error) {
          log.push({ kind: 'answer failed', message: String(error && error.message) });
          await xdotool('key', 'Escape');
        }
        await sleep(step.wait ?? 300);
        // A question that came late (a toast).
        if (answers.length > 0 && openPrompt()) {
          await answered(answers, step.timeout ?? 30000).catch((error) => log.push({ kind: 'answer failed', message: String(error.message) }));
          await sleep(step.wait ?? 300);
        }
        await settle();
        const snap = snapshot(step.label ?? [].concat(step.keys ?? step.type ?? 'step').join(','));
        if (answers.length) snap.leftover = answers;
        if (step.shot) {
          await sleep(300);
          ctx.shot(step.shot);
          snap.shot = step.shot;
        }
        if (step.after) {
          try {
            snap.after = await step.after(ctx);
          } catch (error) {
            snap.after = `after failed: ${error.message}`;
          }
        }
        result.steps.push(snap);
      }
    } catch (error) {
      result.error = String(error && error.stack);
    }
    try {
      await vscode.commands.executeCommand('workbench.action.files.saveAll');
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      await vscode.commands.executeCommand('workbench.action.closePanel');
      await sleep(300);
    } catch {}
    fs.writeFileSync(process.env.OUT, JSON.stringify(results, null, 1));
  }
  results.push({ name: '(output channels)', steps: [], output });
  fs.writeFileSync(process.env.OUT, JSON.stringify(results, null, 1));
};
