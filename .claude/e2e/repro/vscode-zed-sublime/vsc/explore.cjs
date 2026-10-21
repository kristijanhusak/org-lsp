// Runs in VS Code's extension host (see run.mjs): each case of CASES opens
// a file, runs steps (commands with answers to what they ask, typing,
// edits) and records what VS Code shows: the text, the active editor,
// messages, what was asked. Writes the record to OUT.
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const ws = process.env.WS;
let log = [];
let answers = [];

function nextAnswer(kind, prompt, items) {
  const entry = { kind, prompt, items };
  log.push(entry);
  if (answers.length === 0) {
    entry.unanswered = true;
    return undefined;
  }
  const a = answers.shift();
  entry.answer = a;
  return a;
}

function stub() {
  for (const kind of ['showInformationMessage', 'showWarningMessage', 'showErrorMessage']) {
    vscode.window[kind] = (message, ...rest) => {
      log.push({ kind, message: String(message) });
      return Promise.resolve(undefined);
    };
  }
  const status = vscode.window.setStatusBarMessage;
  vscode.window.setStatusBarMessage = (text, t) => {
    log.push({ kind: 'status', message: String(text) });
    return status(text, t);
  };
  vscode.window.showQuickPick = async (items, options) => {
    const shown = await items;
    const labels = shown.map((i) => (typeof i === 'string' ? i : i.label));
    const a = nextAnswer('pick', options?.placeHolder, labels.slice(0, 60));
    if (a === undefined) return undefined;
    if (typeof a === 'string') {
      const found = shown.find((i) => i.key === a) ?? shown.find((i) => i.label === a);
      if (!found) log.push({ kind: 'nochoice', answer: a });
      return found;
    }
    const lab = (i) => i.label.replace(/^\[[^\]]*\] /, '');
    return shown.find((i) => lab(i) === a.choice) ?? shown.find((i) => lab(i).includes(a.choice));
  };
  vscode.window.showInputBox = async (options) => {
    const a = nextAnswer('input', options?.prompt, undefined);
    const entry = log[log.length - 1];
    entry.value = options?.value;
    if (a !== undefined && options?.validateInput) {
      const v = await options.validateInput(a);
      entry.preview = v && (v.message ?? v);
    }
    return a;
  };
  vscode.window.createQuickPick = () => {
    const listeners = { value: [], accept: [], hide: [] };
    const qp = {
      items: [],
      selectedItems: [],
      activeItems: [],
      value: '',
      onDidChangeValue: (l) => listeners.value.push(l),
      onDidAccept: (l) => listeners.accept.push(l),
      onDidHide: (l) => listeners.hide.push(l),
      onDidChangeActive: () => ({ dispose() {} }),
      onDidChangeSelection: () => ({ dispose() {} }),
      onDidTriggerButton: () => ({ dispose() {} }),
      onDidTriggerItemButton: () => ({ dispose() {} }),
      show() {
        const a = nextAnswer('typed', qp.title ?? qp.placeholder, qp.items.map((i) => i.label).slice(0, 60));
        setTimeout(() => {
          if (typeof a !== 'string') {
            qp.hide();
            return;
          }
          qp.value = a;
          listeners.value.forEach((l) => l(a));
          const typed = a.toLowerCase();
          qp.selectedItems = qp.items.filter((i) => i.label.toLowerCase().includes(typed)).slice(0, 1);
          listeners.accept.forEach((l) => l());
        });
      },
      hide: () => listeners.hide.forEach((l) => l()),
      dispose() {},
    };
    return qp;
  };
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

async function withTimeout(promise, ms, what) {
  let timer;
  const t = new Promise((_, reject) => (timer = setTimeout(() => reject(new Error(`timeout: ${what}`)), ms)));
  try {
    return await Promise.race([promise, t]);
  } finally {
    clearTimeout(timer);
  }
}

exports.run = async function run() {
  stub();
  const extension = vscode.extensions.getExtension('kristijanhusak.org-lsp');
  const api = await extension.activate();
  const cases = require(process.env.CASES);
  const only = process.env.ONLY;
  const results = [];
  const ctx = { vscode, api, ws, sleep, path, fs, withTimeout };
  for (const c of cases) {
    if (only && !c.name.includes(only)) continue;
    log = [];
    answers = [];
    const result = { name: c.name, steps: [] };
    results.push(result);
    try {
      if (c.files) {
        for (const [name, text] of Object.entries(c.files)) {
          fs.mkdirSync(path.dirname(path.join(ws, name)), { recursive: true });
          fs.writeFileSync(path.join(ws, name), text);
        }
      }
      if (c.file) {
        const uri = vscode.Uri.file(path.join(ws, c.file));
        if (c.content !== undefined) fs.writeFileSync(uri.fsPath, c.content);
        const doc = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(doc);
        // The server has the file once it gives its symbols.
        for (let i = 0; i < 100; i++) {
          const s = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', uri);
          if (s !== undefined) break;
          await sleep(100);
        }
        await sleep(200);
      }
      for (const step of c.steps ?? []) {
        log = [];
        if (step.at) {
          const [l, ch, l2, ch2] = step.at;
          const e = vscode.window.activeTextEditor;
          e.selection = new vscode.Selection(l, ch, l2 ?? l, ch2 ?? ch);
          await sleep(100);
        }
        answers = typeof step.answers === 'function' ? step.answers(ctx) : [...(step.answers ?? [])];
        if (step.command) {
          try {
            await withTimeout(Promise.resolve(vscode.commands.executeCommand(step.command, ...(step.args ?? []))), step.timeout ?? 30000, step.command);
          } catch (error) {
            log.push({ kind: 'thrown', message: String(error && error.message) });
          }
        }
        if (step.type) {
          await vscode.commands.executeCommand('type', { text: step.type });
        }
        if (step.run) {
          const r = await step.run(ctx);
          if (r !== undefined) log.push({ kind: 'run', value: r });
        }
        await sleep(step.wait ?? 600);
        const snap = snapshot(step.label ?? step.command ?? step.type ?? 'step');
        if (answers.length) snap.leftover = answers;
        if (step.after) {
          try {
            snap.after = await step.after(ctx);
          } catch (error) {
            snap.after = `after failed: ${error.message}`;
          }
        }
        result.steps.push(snap);
      }
      if (c.check) result.check = await c.check(ctx);
    } catch (error) {
      result.error = String(error && error.stack);
    }
    try {
      await vscode.commands.executeCommand('workbench.action.files.saveAll');
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      await sleep(200);
    } catch {}
    fs.writeFileSync(process.env.OUT, JSON.stringify(results, null, 1));
  }
  fs.writeFileSync(process.env.OUT, JSON.stringify(results, null, 1));
};
