// VS Code glue for the org-lsp language server.
//
// The server does the Org work. What VS Code has to be told ahead of time
// (commands, keys, token colors, the agenda's document type) is in
// package.json, generated from the server's own description of itself; see
// scripts/generate.mjs. This file starts the server and tells it where the
// cursor is when a command runs.

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions } from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

/**
 * The server command: the configured path, the binary bundled with the
 * extension, or `org-lsp` from PATH.
 */
function serverCommand(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace.getConfiguration('org-lsp').get<string>('server.path', '').trim();
  if (configured !== '') {
    return configured;
  }
  const exe = process.platform === 'win32' ? 'org-lsp.exe' : 'org-lsp';
  const bundled = path.join(context.extensionPath, 'server', exe);
  return fs.existsSync(bundled) ? bundled : exe;
}

/**
 * Where the cursor is, in the form the server's commands take; with the
 * selection, if any, as Emacs's region.
 */
function cursor(running: LanguageClient): unknown[] {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return [];
  }
  const context: Record<string, unknown> = {
    textDocument: running.code2ProtocolConverter.asTextDocumentIdentifier(editor.document),
    position: running.code2ProtocolConverter.asPosition(editor.selection.active),
  };
  if (!editor.selection.isEmpty) {
    context.range = running.code2ProtocolConverter.asRange(editor.selection);
  }
  return [context];
}

/**
 * What a key passes to its command (see scripts/generate.mjs): the commands
 * to run instead, in turn, when the server says the command does not apply
 * at the cursor, so that the key keeps its own meaning there.
 */
interface KeyArgs {
  fallback: (string | { command: string; args?: unknown; platform?: string })[];
}

function keyArgs(args: unknown[]): KeyArgs | undefined {
  const first = args[0] as Partial<KeyArgs> | undefined;
  return args.length === 1 && Array.isArray(first?.fallback) ? (first as KeyArgs) : undefined;
}

/** Run the first of `commands` that VS Code has. */
async function fallBack(commands: KeyArgs['fallback']): Promise<void> {
  for (const entry of commands) {
    const { command, args, platform } =
      typeof entry === 'string' ? { command: entry, args: undefined, platform: undefined } : entry;
    // One for another platform (a key that means something else there).
    if (platform && platform !== process.platform) {
      continue;
    }
    try {
      await vscode.commands.executeCommand(command, ...(args === undefined ? [] : [args]));
      return;
    } catch {
      // Not there (VSCodeVim's, without VSCodeVim): the next one.
    }
  }
}

/** How a key of a key input shows: Emacs's names for the blank ones, and
 * `C-b` for a control character. */
function keyName(key: string): string {
  const named = ({ ' ': 'SPC', '\r': 'RET', '\t': 'TAB' } as Record<string, string>)[key];
  if (named) return named;
  const code = key.length === 1 ? key.charCodeAt(0) : -1;
  return code > 0 && code < 32 ? `C-${String.fromCharCode(code + 96)}` : key;
}

function notApplicable(error: unknown): boolean {
  return (error as { data?: { notApplicable?: unknown } } | null)?.data?.notApplicable === true;
}

/**
 * A command whose result carries its edit and where the cursor goes
 * (`org.return`): applied to the editor at hand, at once.
 */
async function applyCarried(running: LanguageClient, result: unknown): Promise<void> {
  const carried = result as { edits?: unknown[]; cursor?: { line: number; character: number } } | null;
  const editor = vscode.window.activeTextEditor;
  if (!editor || !Array.isArray(carried?.edits)) {
    return;
  }
  const edits: vscode.TextEdit[] =
    (await running.protocol2CodeConverter.asTextEdits(carried.edits as Parameters<typeof running.protocol2CodeConverter.asTextEdits>[0])) ?? [];
  await editor.edit((builder) => {
    for (const edit of edits) {
      builder.replace(edit.range, edit.newText);
    }
  }, { undoStopBefore: false, undoStopAfter: false });
  if (carried.cursor) {
    const position = running.protocol2CodeConverter.asPosition(carried.cursor);
    editor.selection = new vscode.Selection(position, position);
    editor.revealRange(new vscode.Range(position, position));
  }
}

/** The server's commands run so far, in turn: what the tests see of the
 * keys they press. */
const commandsRun: string[] = [];
/** How many commands, and documents the server asked to show, are not done yet. */
let busy = 0;

async function whileBusy<T>(run: () => Promise<T>): Promise<T> {
  busy += 1;
  try {
    return await run();
  } finally {
    busy -= 1;
  }
}

function counted<T>(name: string, run: () => Promise<T>): Promise<T> {
  commandsRun.push(name);
  return whileBusy(run);
}

/** What the server said column view shows, last (`org-lsp/columns`). */
let columnsReported: unknown;

/** Done when the date prompt the server asked to show is shown. */
let datePromptShown: Promise<void> = Promise.resolve();

function sameUri(a: string, b: string): boolean {
  return vscode.Uri.parse(a).toString() === vscode.Uri.parse(b).toString();
}

async function showDatePrompt(params: { uri: string; takeFocus?: boolean; selection?: { start: { line: number; character: number } } }): Promise<void> {
  const uri = vscode.Uri.parse(params.uri);
  const document = await vscode.workspace.openTextDocument(uri);
  const shown = vscode.window.visibleTextEditors.find((editor) => editor.document.uri.toString() === uri.toString());
  const start = params.selection?.start;
  const position = start ? new vscode.Position(start.line, start.character) : undefined;
  await vscode.window.showTextDocument(document, {
    viewColumn: shown?.viewColumn ?? vscode.ViewColumn.Beside,
    preserveFocus: params.takeFocus === false,
    preview: false,
    selection: position ? new vscode.Range(position, position) : undefined,
  });
}

/**
 * Show the agenda or a capture buffer where `org-lsp.winSplitMode` says: in
 * a group below (`split`), beside (`vsplit`), in a window of its own
 * (`float`), or in the current group. An editor that already shows it is
 * used again.
 */
async function showApart(params: { uri: string; takeFocus?: boolean; selection?: { start: { line: number; character: number } } }): Promise<void> {
  const uri = vscode.Uri.parse(params.uri);
  const document = await vscode.workspace.openTextDocument(uri);
  const start = params.selection?.start;
  const position = start ? new vscode.Position(start.line, start.character) : undefined;
  const options = (viewColumn: vscode.ViewColumn): vscode.TextDocumentShowOptions => ({
    viewColumn,
    preserveFocus: params.takeFocus === false,
    preview: false,
    selection: position ? new vscode.Range(position, position) : undefined,
  });
  // A group with a tab of it already, even one not in front.
  const group = vscode.window.tabGroups.all.find((candidate) =>
    candidate.tabs.some((tab) => tab.input instanceof vscode.TabInputText && sameUri(tab.input.uri.toString(), params.uri)),
  );
  if (group) {
    await vscode.window.showTextDocument(document, options(group.viewColumn));
    return;
  }
  const mode = vscode.workspace.getConfiguration('org-lsp').get<string>('winSplitMode', 'split');
  if (mode === 'vsplit') {
    await vscode.window.showTextDocument(document, options(vscode.ViewColumn.Beside));
    return;
  }
  // An empty group below the current one, which the document then fills.
  if (mode === 'split' && vscode.window.activeTextEditor) {
    await vscode.commands.executeCommand('workbench.action.newGroupBelow');
  }
  await vscode.window.showTextDocument(document, options(vscode.ViewColumn.Active));
  if (mode === 'float') {
    await vscode.commands.executeCommand('workbench.action.moveEditorToNewWindow');
  }
}

/**
 * Show the org-roam buffer in the group beside the current one, or in the
 * group that has it, the cursor staying where it is when the server asks.
 */
async function showBeside(params: { uri: string; takeFocus?: boolean }): Promise<void> {
  const document = await vscode.workspace.openTextDocument(vscode.Uri.parse(params.uri));
  const group = vscode.window.tabGroups.all.find((candidate) =>
    candidate.tabs.some((tab) => tab.input instanceof vscode.TabInputText && sameUri(tab.input.uri.toString(), params.uri)),
  );
  await vscode.window.showTextDocument(document, {
    viewColumn: group?.viewColumn ?? vscode.ViewColumn.Beside,
    preserveFocus: params.takeFocus === false,
    preview: false,
  });
}

/**
 * Show an entry's file from the agenda with TAB (`org-agenda-goto`): in a
 * group that has it, else in an empty group, else in a new group beside
 * the agenda. The agenda keeps its group.
 */
async function showInOtherGroup(params: { uri: string; takeFocus?: boolean; selection?: { start: { line: number; character: number } } }): Promise<void> {
  const uri = vscode.Uri.parse(params.uri);
  const document = await vscode.workspace.openTextDocument(uri);
  const start = params.selection?.start;
  const position = start ? new vscode.Position(start.line, start.character) : undefined;
  const active = vscode.window.tabGroups.activeTabGroup;
  const from = vscode.window.activeTextEditor;
  const others = vscode.window.tabGroups.all.filter((group) => group !== active);
  const group =
    others.find((candidate) =>
      candidate.tabs.some((tab) => tab.input instanceof vscode.TabInputText && sameUri(tab.input.uri.toString(), params.uri)),
    ) ?? others.find((candidate) => candidate.tabs.length === 0);
  if (!group) {
    // A new, empty group to the right, which the file then fills.
    await vscode.commands.executeCommand('workbench.action.newGroupRight');
  }
  await vscode.window.showTextDocument(document, {
    viewColumn: group?.viewColumn ?? vscode.ViewColumn.Active,
    preserveFocus: params.takeFocus === false,
    preview: false,
    selection: position ? new vscode.Range(position, position) : undefined,
  });
  // Follow mode shows the entry and leaves the cursor in the agenda, which
  // a new group took it from.
  if (params.takeFocus === false && from && vscode.window.activeTextEditor !== from) {
    await vscode.window.showTextDocument(from.document, { viewColumn: from.viewColumn, preserveFocus: false });
  }
}

/** Save the files the server names that have changes. */
async function saveAsked(result: unknown): Promise<void> {
  const save = (result as { save?: unknown } | null)?.save;
  if (!Array.isArray(save)) {
    return;
  }
  for (const document of vscode.workspace.textDocuments) {
    if (document.isDirty && save.some((uri) => typeof uri === 'string' && sameUri(uri, document.uri.toString()))) {
      await document.save();
    }
  }
}

/** Show what a command has to say, as Emacs shows it in the echo area. */
function sayAsked(result: unknown): void {
  const message = (result as { message?: unknown } | null)?.message;
  if (typeof message === 'string') {
    say(message);
  }
}

/**
 * Tell the user `message`: as a notification, or in the status bar
 * (`org-lsp.messages`).
 */
function say(message: string): void {
  if (vscode.workspace.getConfiguration('org-lsp').get<string>('messages') === 'statusBar') {
    vscode.window.setStatusBarMessage(message, 5000);
  } else {
    void vscode.window.showInformationMessage(message);
  }
}

/** Close the editors of a document the server is done with. */
function closeAsked(result: unknown): void {
  const close = (result as { close?: string } | null)?.close;
  if (typeof close !== 'string') {
    return;
  }
  const tabs = vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .filter((tab) => tab.input instanceof vscode.TabInputText && sameUri(tab.input.uri.toString(), close));
  // A file of the server's own, such as a capture buffer, is saved first,
  // so that closing it asks nothing: the server has its text already.
  const document = vscode.workspace.textDocuments.find((open) => sameUri(open.uri.toString(), close));
  const saved = document?.isDirty && document.uri.scheme === 'file' ? document.save() : Promise.resolve(true);
  void saved.then(() => vscode.window.tabGroups.close(tabs));
}

/** What a command asks for before it runs, when its result says so. */
interface Input {
  kind: string;
  prompt: string;
  value: string;
  /** What to choose from: a key (kind `key`) or a name, and how it shows. */
  choices?: Choice[];
  /** Text that may be typed, offered as it is typed (kind `text`). */
  completions?: string[];
  /** What separates several answers (the `:` between tags): what follows the last is completed. */
  separator?: string;
  /** Whether a completion picked with nothing typed is the answer, not the prompt's default. */
  select?: boolean;
  /** What to send back with the answer, besides it. */
  context?: Record<string, unknown>;
}

/**
 * A choice of an input: a TODO keyword comes with the selector of its
 * token (`todoKeyword`, `todoKeyword.done`) and its face, if it has one.
 */
interface Choice {
  key: string;
  label: string;
  highlight?: string;
  face?: { foreground?: string };
}

/** Where the icons of choices are written. */
let iconDirectory: vscode.Uri | undefined;

/**
 * The icon of a choice offered in a look of its own, as Org's
 * `org-fast-todo-selection` shows each keyword in its face: a dot in the
 * color of its face, or else in red or green for a TODO or a done keyword.
 * A quick pick cannot color its text.
 */
function choiceIcon(choice: Choice): vscode.Uri | vscode.ThemeIcon | undefined {
  const color = choice.face?.foreground;
  if (color && /^#?\w+$/.test(color) && iconDirectory) {
    const icon = vscode.Uri.joinPath(iconDirectory, `dot-${color.replace('#', '')}.svg`);
    if (!fs.existsSync(icon.fsPath)) {
      fs.mkdirSync(iconDirectory.fsPath, { recursive: true });
      fs.writeFileSync(
        icon.fsPath,
        `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="5" fill="${color}"/></svg>`,
      );
    }
    return icon;
  }
  if (!choice.highlight) return undefined;
  const done = choice.highlight.endsWith('.done');
  return new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor(done ? 'charts.green' : 'charts.red'));
}

/**
 * Ask what `input` asks for. A key is picked from its choices, the way
 * Org's fast selection offers them; anything else is typed.
 */
async function ask(input: Input, typed: (text: string) => Promise<vscode.InputBoxValidationMessage | undefined>) {
  if (input.choices) {
    // A key shows with its choice, as Org's fast selection lists them.
    const items = input.choices.map((choice) => ({
      label: input.kind === 'key' ? `[${keyName(choice.key)}] ${choice.label}` : choice.label,
      key: choice.key,
      iconPath: choiceIcon(choice),
    }));
    const picked = await vscode.window.showQuickPick(items, { placeHolder: input.prompt, ignoreFocusOut: true });
    return picked?.key;
  }
  if (input.completions?.length) return typeOrPick(input, input.completions);
  return vscode.window.showInputBox({
    prompt: input.prompt,
    value: input.value,
    ignoreFocusOut: true,
    validateInput: input.kind === 'date' ? typed : undefined,
  });
}

/**
 * Text typed with `completions` offered, as Emacs's `completing-read`
 * offers them: what is typed is the first item, so that it can be taken
 * as it is; an empty answer stands for the prompt's default.
 */
function typeOrPick(input: Input, completions: string[]): Promise<string | undefined> {
  const pick = vscode.window.createQuickPick();
  pick.title = input.prompt;
  pick.placeholder = input.prompt;
  pick.ignoreFocusOut = true;
  const items = (typed: string) => {
    // With a separator, what comes before the last one is kept.
    const at = input.separator ? typed.lastIndexOf(input.separator) : -1;
    const before = at >= 0 ? typed.slice(0, at + (input.separator ?? '').length) : '';
    const others = completions.map((label) => ({ label: before + label }));
    if (!typed || others.some((other) => other.label === typed)) return others;
    return [{ label: typed, description: 'as typed' }, ...others];
  };
  pick.items = items(input.value);
  pick.value = input.value;
  pick.onDidChangeValue((typed) => (pick.items = items(typed)));
  return new Promise((resolve) => {
    let answered = false;
    pick.onDidAccept(() => {
      answered = true;
      const empty = pick.value === '' && !input.select;
      resolve(empty ? '' : (pick.selectedItems[0]?.label ?? pick.value));
      pick.hide();
    });
    pick.onDidHide(() => {
      if (!answered) resolve(undefined);
      pick.dispose();
    });
    pick.show();
  });
}

function asked(result: unknown): Input | undefined {
  const input = (result as { input?: Input } | null)?.input;
  return input && typeof input.prompt === 'string' ? input : undefined;
}

/**
 * What a date typed so far means, shown under the input box as the server
 * reads it; nothing while it reads as nothing.
 */
async function preview(
  running: LanguageClient,
  command: string,
  context: Record<string, unknown>,
  text: string,
): Promise<vscode.InputBoxValidationMessage | undefined> {
  try {
    const params = { command: 'org.date.preview', arguments: [{ ...context, command, input: text }] };
    const result = (await running.sendRequest('workspace/executeCommand', params)) as { timestamp?: string } | null;
    return result?.timestamp
      ? { message: `=> ${result.timestamp}`, severity: vscode.InputBoxValidationSeverity.Info }
      : undefined;
  } catch {
    return undefined;
  }
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const command = serverCommand(context);
  const serverOptions: ServerOptions = { command, args: [] };
  const { languages, agendaScheme, capturePrefix, roamBufferPrefix, datePromptUri, clipboardCommands } = context.extension
    .packageJSON.orgLsp as {
    languages: string[];
    agendaScheme: string;
    capturePrefix: string;
    roamBufferPrefix: string;
    datePromptUri: string;
    clipboardCommands: string[];
  };
  // A file of the org-roam buffer; the persistent one, which follows the
  // cursor, has the prefix for its name.
  const isRoamBuffer = (uri: vscode.Uri): boolean =>
    uri.scheme === 'file' && path.basename(uri.fsPath).startsWith(roamBufferPrefix);
  const roamBufferShown = (): boolean =>
    vscode.window.visibleTextEditors.some(
      (editor) => editor.document.uri.scheme === 'file' && path.basename(editor.document.uri.fsPath) === `${roamBufferPrefix}.org`,
    );
  // The agenda and capture buffers open apart from the file at hand.
  const apart = (uri: string): boolean => {
    const parsed = vscode.Uri.parse(uri);
    if (parsed.scheme === agendaScheme) {
      return !sameUri(uri, datePromptUri);
    }
    return parsed.scheme === 'file' && path.basename(parsed.fsPath).startsWith(capturePrefix);
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [...languages.map((language) => ({ language })), { scheme: agendaScheme }],
    initializationOptions: vscode.workspace.getConfiguration('org-lsp').get('settings'),
    middleware: {
      // The client registers every command of the server with VS Code. Run
      // from a key or the palette, a command has no arguments: add the
      // cursor. The server's reason for refusing a command is shown as is.
      executeCommand: (name, args) => counted(name, async () => {
        const running = client;
        const key = keyArgs(args);
        if (!running) {
          // A key keeps its own meaning without a server (Enter types).
          return key ? fallBack(key.fallback) : undefined;
        }
        // Emacs's prefix argument, from a key's arguments (`{ "count": 16 }`): with the cursor.
        const count = !key && args.length === 1 ? (args[0] as { count?: unknown; textDocument?: unknown } | null) : null;
        const counted = typeof count?.count === 'number' && count.textDocument === undefined;
        const params = { command: name, arguments: args.length > 0 && !key && !counted ? args : cursor(running) };
        if (counted && params.arguments[0]) {
          params.arguments[0] = { ...(params.arguments[0] as object), count: count?.count };
        }
        // In column view, the column picked.
        if (name.startsWith('org.columns.') && params.arguments[0]) {
          const editor = vscode.window.activeTextEditor;
          params.arguments[0] = { ...(params.arguments[0] as object), column: editor ? columnPicked(editor) : 0 };
        }
        // What the command reads as Emacs's latest kill: the clipboard.
        if (clipboardCommands.includes(name) && params.arguments[0]) {
          params.arguments[0] = { ...(params.arguments[0] as object), clipboard: await vscode.env.clipboard.readText() };
        }
        // Whether the org-roam buffer shows, which the command hides then.
        if (name === 'org.roam.buffer.toggle' && params.arguments[0]) {
          params.arguments[0] = { ...(params.arguments[0] as object), visible: roamBufferShown() };
        }
        // LaTeX is drawn in the colors of the text.
        if (name === 'org.latex.preview' && params.arguments[0]) {
          params.arguments[0] = { ...latexColors(), ...(params.arguments[0] as object) };
        }
        // What a result carries: an edit, text for the clipboard, documents
        // to close or save, a message.
        const handle = async (result: unknown): Promise<void> => {
          await applyCarried(running, result);
          // What was cut or copied goes to the clipboard.
          const copied = (result as { copy?: unknown } | null)?.copy;
          if (typeof copied === 'string') {
            await vscode.env.clipboard.writeText(copied);
          }
          await showSparse(running, result);
          closeAsked(result);
          await saveAsked(result);
          sayAsked(result);
          // Images of links, shown or taken away.
          const preview = (result as { preview?: Preview } | null)?.preview;
          if (preview) {
            await applyPreview(preview);
          }
        };
        try {
          let result = await running.sendRequest('workspace/executeCommand', params);
          await handle(result);
          let context = (params.arguments[0] ?? {}) as Record<string, unknown>;
          // The command asks for something first: ask, and run it again
          // with the answer, until it asks for nothing more.
          for (let input = asked(result); input; input = asked(result)) {
            // The server may have shown its date prompt for this; the
            // input box comes after it, so that it keeps the focus.
            await datePromptShown;
            const answer = await ask(input, (text) => preview(running, name, context, text));
            if (answer === undefined) {
              return undefined;
            }
            context = { ...context, ...(input.context ?? {}), input: answer };
            result = await running.sendRequest('workspace/executeCommand', { command: name, arguments: [context] });
            await handle(result);
          }
          return result;
        } catch (error) {
          // A key keeps its own meaning where the command does not apply,
          // and when it fails: Enter is never lost.
          if (key) {
            await fallBack(key.fallback);
            if (notApplicable(error)) {
              return undefined;
            }
          }
          void vscode.window.showWarningMessage(error instanceof Error ? error.message : String(error));
          return undefined;
        }
      }),
      window: {
        // The date prompt opens beside the file, as Emacs pops its calendar
        // up in another window, and stays in that editor as it changes.
        showDocument: (params, token, next) => whileBusy(async () => {
          if (params.external) {
            return next(params, token);
          }
          if (apart(params.uri)) {
            await showApart(params);
            return { success: true };
          }
          // The org-roam buffer beside the file, as Emacs shows it in a
          // window of its own.
          if (isRoamBuffer(vscode.Uri.parse(params.uri))) {
            await showBeside(params);
            return { success: true };
          }
          if ((params as { orgOtherWindow?: boolean }).orgOtherWindow) {
            await showInOtherGroup(params);
            return { success: true };
          }
          if (!sameUri(params.uri, datePromptUri)) {
            return next(params, token);
          }
          datePromptShown = showDatePrompt(params);
          await datePromptShown;
          return { success: true };
        }),
      },
      // Headlines in the colors of their level, with
      // `org-lsp.headlineLevelColors`; a habit's days in the agenda, in
      // their backgrounds. The server sends no deltas.
      provideDocumentSemanticTokens: async (document, token, next) => {
        const tokens = await next(document, token);
        habitsOf(document, tokens);
        return levelColored(tokens);
      },
      provideDocumentRangeSemanticTokens: async (document, range, token, next) =>
        levelColored(await next(document, range, token)),
    },
  };
  client = new LanguageClient('org-lsp', 'Org Language Server', serverOptions, clientOptions);
  // This client shows the server's date prompt, and the prefixes of
  // `org-indent-mode`.
  client.registerFeature({
    fillClientCapabilities(capabilities) {
      capabilities.experimental = {
        ...(capabilities.experimental ?? {}),
        // org-lsp's own client: its token types, a command's result.
        orgClient: true,
        orgDatePrompt: true,
        orgVirtualIndent: true,
        orgDisplay: true,
        orgLineHighlights: true,
        orgImages: imagesEnabled(),
        // Commands the server asks for (`org-lsp/runCommand`).
        orgRunCommand: true,
      };
    },
    initialize() {},
    getState() {
      return { kind: 'static' };
    },
    clear() {},
  });
  // The line of an Org file the cursor is on, while the org-roam buffer
  // shows: the buffer follows the node it is in.
  const orgLines = new Map<string, number>();
  context.subscriptions.push(
    vscode.window.onDidChangeTextEditorSelection((event) => {
      const document = event.textEditor.document;
      if (document.languageId !== 'org' || isRoamBuffer(document.uri) || !client || !roamBufferShown()) {
        return;
      }
      const line = event.selections[0]?.active.line ?? 0;
      if (orgLines.get(document.uri.toString()) === line) {
        return;
      }
      orgLines.set(document.uri.toString(), line);
      void client.sendNotification('org-lsp/cursor', {
        textDocument: { uri: document.uri.toString() },
        position: { line, character: 0 },
      });
    }),
  );
  // The line of the agenda the cursor is on, each time it changes, for
  // follow mode (`F`), which the server keeps.
  const agendaLines = new Map<string, number>();
  context.subscriptions.push(
    vscode.window.onDidChangeTextEditorSelection((event) => {
      const uri = event.textEditor.document.uri;
      if (uri.scheme !== agendaScheme || sameUri(uri.toString(), datePromptUri) || !client) {
        return;
      }
      const line = event.selections[0]?.active.line ?? 0;
      if (agendaLines.get(uri.toString()) === line) {
        return;
      }
      agendaLines.set(uri.toString(), line);
      // `org-agenda-show-outline-path`: the entry's outline path, in the
      // status bar for a while, as Emacs shows it in the echo area.
      void client
        .sendRequest('org-lsp/agendaCursor', { textDocument: { uri: uri.toString() }, position: { line, character: 0 } })
        .then((result) => {
          const echo = (result as { echo?: unknown } | null)?.echo;
          if (typeof echo === 'string') {
            echoed = echo;
            vscode.window.setStatusBarMessage(echo, 5000);
          }
        });
    }),
  );
  try {
    await client.start();
    // Files the server changed from elsewhere (the agenda, a capture, a
    // refile), to save now that the change is made: `auto_save`.
    client.onNotification('org-lsp/save', (params: { uris?: string[] }) => saveAsked({ save: params.uris ?? [] }));
    // The clock that runs, for the status bar.
    client.onNotification('org-lsp/clock', (params: ClockReport | null) => showClock(params));
    // What column view shows, or that it is off.
    client.onNotification('org-lsp/columns', (params: ColumnsShown) => showColumns(params));
    // The relative timer or the countdown, for the status bar.
    client.onNotification('org-lsp/timer', (params: TimerReport | null) => showTimer(params));
    // How far the lines of a file with `org-indent-mode` show indented.
    client.onNotification('org-lsp/virtualIndent', (params: VirtualIndent) => showVirtualIndent(params));
    // What stands in for hidden text: entities, sub- and superscripts.
    client.onNotification('org-lsp/display', (params: Display) => showDisplay(params));
    // Lines of the agenda highlighted from edge to edge: the clocked entry.
    client.onNotification('org-lsp/lineHighlights', (params: LineHighlights) => showLineHighlights(params));
    // Faces of particular keywords and priorities.
    setFaces(client.initializeResult?.capabilities.experimental?.orgFaces);
    // Images in place of LaTeX fragments, made by Emacs, and of links to
    // images downloaded first; quiet for those a file opens with.
    client.onNotification('org-lsp/images', (params: Preview) => applyPreview(params, params.startup === true));
    // org-journal's files, Org files whose name need not say so.
    journalDirectory = client.initializeResult?.capabilities.experimental?.orgJournal?.directory;
    vscode.workspace.textDocuments.forEach((document) => void asJournalFile(document));
    // The headlines of a document org-journal showed, folded as it folds
    // them, or the subtrees a yank or paste folded.
    client.onNotification('org-lsp/folds', (params: JournalFolds) => void journalFolds(params));
    // A command the server asks to run as from its key, where the cursor
    // is, with fields of its own: a capture an org-protocol URL started.
    const running = client;
    client.onNotification('org-lsp/runCommand', (params: { command?: unknown; argument?: unknown } | null) => {
      if (typeof params?.command !== 'string') {
        return;
      }
      const here = (cursor(running)[0] ?? {}) as Record<string, unknown>;
      const given = typeof params.argument === 'object' && params.argument ? params.argument : {};
      void vscode.commands.executeCommand(params.command, { ...here, ...given });
    });
    // The previews of the files open.
    const started = client;
    vscode.workspace.textDocuments.forEach((document) => void startupPreviews(started, document));
  } catch (error) {
    client = undefined;
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(
      `org-lsp: could not start the server (${command}): ${message}. ` +
        'Install `org-lsp` on your PATH or set "org-lsp.server.path".',
    );
  }
}

/** The last restart: a restart waits for the one before, so that two never
 * start two servers. */
let restarting: Promise<void> = Promise.resolve();
// Once the extension is deactivated, a restart under way starts no server.
let deactivated = false;

function restartClient(context: vscode.ExtensionContext): Promise<void> {
  restarting = restarting
    .catch(() => undefined)
    .then(async () => {
      // A server that is already gone cannot be stopped: start a new one all the same.
      await stopClient().catch(() => undefined);
      if (!deactivated) {
        await startClient(context);
      }
    });
  return restarting;
}

async function stopClient(): Promise<void> {
  const running = client;
  client = undefined;
  await running?.stop();
}

// ----- keyword faces

/** A face of `org_todo_keyword_faces` or `org_priority_faces`, as the server announces it. */
interface Face {
  foreground?: string;
  background?: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strikethrough?: boolean;
}

/** The faces of particular TODO keywords and priorities. */
interface Faces {
  todo?: Record<string, Face>;
  priority?: Record<string, Face>;
}

const faceDecorations = new Map<string, vscode.TextEditorDecorationType>();
/** For tests: the keywords and priorities painted in each editor, `[line, text]`. */
const facesShown = new WeakMap<vscode.TextEditor, [number, string][]>();
const faceTimers = new Map<string, ReturnType<typeof setTimeout>>();

function setFaces(faces: Faces | undefined): void {
  for (const decoration of faceDecorations.values()) {
    decoration.dispose();
  }
  faceDecorations.clear();
  for (const [kind, list] of Object.entries(faces ?? {}) as [string, Record<string, Face>][]) {
    for (const [name, face] of Object.entries(list ?? {})) {
      const lines = [face.underline && 'underline', face.strikethrough && 'line-through'].filter(Boolean).join(' ');
      faceDecorations.set(
        `${kind}:${name}`,
        vscode.window.createTextEditorDecorationType({
          color: face.foreground,
          backgroundColor: face.background,
          fontWeight: face.bold ? 'bold' : undefined,
          fontStyle: face.italic ? 'italic' : undefined,
          textDecoration: lines || undefined,
        }),
      );
    }
  }
  vscode.window.visibleTextEditors.forEach((editor) => void paintFaces(editor));
}

/** Give the keywords and priorities of `editor` that have a face of their own that face. */
async function paintFaces(editor: vscode.TextEditor): Promise<void> {
  const running = client;
  const document = editor.document;
  if (!running || faceDecorations.size === 0 || !['org', 'org-agenda'].includes(document.languageId)) {
    return;
  }
  const legend = running.initializeResult?.capabilities.semanticTokensProvider?.legend;
  const version = document.version;
  const tokens = (await running
    .sendRequest('textDocument/semanticTokens/full', { textDocument: { uri: document.uri.toString() } })
    .catch(() => undefined)) as { data?: number[] } | undefined;
  if (!legend || !tokens?.data || document.version !== version) {
    return;
  }
  const ranges = new Map<string, vscode.Range[]>();
  const shown: [number, string][] = [];
  const data = tokens.data;
  let line = 0;
  let character = 0;
  for (let i = 0; i + 4 < data.length; i += 5) {
    line += data[i];
    character = data[i] === 0 ? character + data[i + 1] : data[i + 1];
    const type = legend.tokenTypes[data[i + 3]];
    // `org-tag-faces`: each tag of a headline in an Org file with a face of
    // its own, its name without the colons; the agenda shows tags in
    // `org-tag` alone.
    if (type === 'tag' && document.languageId === 'org' && line < document.lineCount) {
      const text = document.lineAt(line).text.slice(character, character + data[i + 2]);
      const names = /:([^:\s]+)(?=:)/g;
      for (let found = names.exec(text); found; found = names.exec(text)) {
        const key = `tag:${found[1]}`;
        if (faceDecorations.has(key)) {
          const start = character + found.index + 1;
          const range = new vscode.Range(line, start, line, start + found[1].length);
          ranges.set(key, [...(ranges.get(key) ?? []), range]);
          shown.push([line, found[1]]);
        }
      }
      continue;
    }
    const kind = type === 'todoKeyword' ? 'todo' : type === 'priority' ? 'priority' : undefined;
    if (!kind || line >= document.lineCount) {
      continue;
    }
    const range = new vscode.Range(line, character, line, character + data[i + 2]);
    let name = document.getText(range);
    if (kind === 'priority') {
      name = /^\[#(.*)\]$/.exec(name)?.[1] ?? name;
    }
    const key = `${kind}:${name}`;
    if (faceDecorations.has(key)) {
      ranges.set(key, [...(ranges.get(key) ?? []), range]);
      shown.push([line, document.getText(range)]);
    }
  }
  for (const [key, decoration] of faceDecorations) {
    editor.setDecorations(decoration, ranges.get(key) ?? []);
  }
  facesShown.set(editor, shown);
}

/** Paint again a while after the document changed. */
function facesChanged(event: vscode.TextDocumentChangeEvent): void {
  const key = event.document.uri.toString();
  clearTimeout(faceTimers.get(key));
  faceTimers.set(
    key,
    setTimeout(() => {
      faceTimers.delete(key);
      vscode.window.visibleTextEditors.filter((editor) => editor.document === event.document).forEach((editor) => void paintFaces(editor));
    }, 300),
  );
}

// ----- column view

/** What the server says column view shows on a document (`org-lsp/columns`). */
interface ColumnsShown {
  uri: string;
  header?: string;
  columns?: { key: string; title: string; width: number }[];
  /** Each headline's line, the text it shows, its values; null when off. */
  lines: [number, string, string[]][] | null;
}

const columnsShown = new Map<string, ColumnsShown>();
/** The outline path the status bar showed last for the agenda's cursor. */
let echoed = '';
const columnsPicked = new Map<string, number>();
/** A headline's own text, under its columns. */
const columnsHidden = vscode.window.createTextEditorDecorationType({ textDecoration: 'none; display: none;' });
/** The columns over a headline, in the colours of Emacs's `org-column`. */
const columnsDecoration = vscode.window.createTextEditorDecorationType({});
let columnsItem: vscode.StatusBarItem | undefined;

function columnPicked(editor: vscode.TextEditor): number {
  return columnsPicked.get(editor.document.uri.toString()) ?? 0;
}

/**
 * Column view in `editor`: each headline covered by its columns, and the
 * header in the status bar, the column picked in brackets (Emacs's cursor
 * moves between them, Left and Right here).
 */
function drawColumns(editor: vscode.TextEditor): void {
  const shown = columnsShown.get(editor.document.uri.toString());
  const lines = (shown?.lines ?? []).filter(([line]) => line < editor.document.lineCount);
  editor.setDecorations(
    columnsHidden,
    lines.map(([line]) => editor.document.lineAt(line).range),
  );
  editor.setDecorations(
    columnsDecoration,
    lines.map(([line, text]) => ({
      range: new vscode.Range(line, 0, line, 0),
      renderOptions: {
        before: {
          contentText: text.replaceAll(' ', '\u00a0'),
          backgroundColor: new vscode.ThemeColor('editor.lineHighlightBackground'),
        },
      },
    })),
  );
  if (editor === vscode.window.activeTextEditor) {
    updateColumnsItem(editor);
  }
}

function updateColumnsItem(editor: vscode.TextEditor | undefined): void {
  const shown = editor && columnsShown.get(editor.document.uri.toString());
  void vscode.commands.executeCommand('setContext', 'org-lsp.columns', shown !== undefined);
  if (!columnsItem) {
    return;
  }
  if (!shown || !editor) {
    columnsItem.hide();
    return;
  }
  const picked = columnPicked(editor);
  const titles = (shown.columns ?? []).map((column, index) => (index === picked ? `[${column.title}]` : column.title));
  columnsItem.text = `$(table) ${titles.join(' | ')}`;
  columnsItem.show();
}

function showColumns(params: ColumnsShown): void {
  columnsReported = params;
  if (params.lines) {
    columnsShown.set(params.uri, params);
    const count = params.columns?.length ?? 1;
    columnsPicked.set(params.uri, Math.min(columnsPicked.get(params.uri) ?? 0, count - 1));
  } else {
    columnsShown.delete(params.uri);
  }
  for (const editor of vscode.window.visibleTextEditors) {
    if (sameUri(editor.document.uri.toString(), params.uri)) {
      drawColumns(editor);
    }
  }
  updateColumnsItem(vscode.window.activeTextEditor);
}

/** Left and Right in column view: the column picked moves. */
function moveColumn(delta: number): void {
  const editor = vscode.window.activeTextEditor;
  const shown = editor && columnsShown.get(editor.document.uri.toString());
  if (!editor || !shown) {
    return;
  }
  const last = (shown.columns?.length ?? 1) - 1;
  columnsPicked.set(editor.document.uri.toString(), Math.max(0, Math.min(last, columnPicked(editor) + delta)));
  updateColumnsItem(editor);
}

// ----- the clock

/** What the server says about the clock that runs (`org-lsp/clock`). */
interface ClockReport {
  heading: string;
  /** Minutes clocked on the entry when the report was sent. */
  minutes: number;
  /** The entry's effort, in minutes. */
  effort: number | null;
}

let clockItem: vscode.StatusBarItem | undefined;
let clockTimer: NodeJS.Timeout | undefined;
let clockState: (ClockReport & { received: number }) | undefined;

function duration(minutes: number): string {
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, '0')}`;
}

/**
 * What Emacs's mode line shows while a clock runs, such as
 * `[0:25/1:00] (Write the report)`; empty when none does.
 */
function clockText(): string {
  if (!clockState) {
    return '';
  }
  const minutes = clockState.minutes + Math.floor((Date.now() - clockState.received) / 60000);
  const time = clockState.effort == null ? duration(minutes) : `${duration(minutes)}/${duration(clockState.effort)}`;
  return `[${time}] (${clockState.heading})`;
}

function updateClockItem(): void {
  if (!clockItem) {
    return;
  }
  if (!clockState) {
    clockItem.hide();
    return;
  }
  clockItem.text = `$(clock) ${clockText()}`;
  clockItem.show();
}

/** What the server says about the relative timer or the countdown (`org-lsp/timer`). */
interface TimerReport {
  /** Its value when the report was sent. */
  seconds: number;
  countdown: boolean;
  paused: boolean;
}

let timerItem: vscode.StatusBarItem | undefined;
let timerTicks: NodeJS.Timeout | undefined;
let timerState: (TimerReport & { received: number }) | undefined;

/**
 * What Emacs's mode line shows while the relative timer or a countdown is
 * on (`org-timer-update-mode-line`), such as `<0:05:23>`; empty when none is.
 */
function timerText(): string {
  if (!timerState) {
    return '';
  }
  const passed = timerState.paused ? 0 : Math.floor((Date.now() - timerState.received) / 1000);
  const seconds = timerState.countdown ? Math.max(0, timerState.seconds - passed) : timerState.seconds + passed;
  const two = (n: number) => String(n).padStart(2, '0');
  return `<${Math.floor(seconds / 3600)}:${two(Math.floor((seconds % 3600) / 60))}:${two(seconds % 60)}>`;
}

function updateTimerItem(): void {
  if (!timerItem) {
    return;
  }
  if (!timerState) {
    timerItem.hide();
    return;
  }
  timerItem.text = `$(watch) ${timerText()}`;
  timerItem.show();
}

function showTimer(report: TimerReport | null): void {
  timerState = report && typeof report.seconds === 'number' ? { ...report, received: Date.now() } : undefined;
  clearInterval(timerTicks);
  timerTicks = timerState && !timerState.paused ? setInterval(updateTimerItem, 1000) : undefined;
  updateTimerItem();
}

function showClock(report: ClockReport | null): void {
  clockState = report && typeof report.heading === 'string' ? { ...report, received: Date.now() } : undefined;
  clearInterval(clockTimer);
  clockTimer = clockState ? setInterval(updateClockItem, 30000) : undefined;
  updateClockItem();
}

// ----- concealing

/**
 * The text the server marks `hidden` in its tokens (link brackets and
 * targets, emphasis markers with `org_hide_emphasis_markers`) is hidden, as Org
 * hides it. VS Code cannot conceal text; a decoration whose CSS takes the
 * text out of the line is the usual way. On a line with a cursor or a
 * selection it shows, so that it can be edited.
 */
const hiddenDecoration = vscode.window.createTextEditorDecorationType({ textDecoration: 'none; display: none;' });

/** The hidden ranges of each document, for the version they were read for. */
const hiddenRanges = new Map<string, { version: number; ranges: vscode.Range[] }>();

/** What each editor hides now, as text; for the tests. */
const concealedText = new Map<vscode.TextEditor, string[]>();

/**
 * `org-lsp/display`: what stands in for hidden text (an entity's
 * character, a sub- or superscript in Unicode, a shrunk table field), as
 * `[line, character, end line, end character, shown]`, for a version of
 * the document. Hidden with the rest, it shows before the hidden text, and
 * on a line being edited neither does: the text shows as it is.
 */
interface Display {
  uri: string;
  version: number;
  items: [number, number, number, number, string][];
}

const displays = new Map<string, Display>();
const standInDecoration = vscode.window.createTextEditorDecorationType({});
/** The text each editor hides for what stands in for it. */
const hiddenStandIns = new Map<vscode.TextEditor, vscode.Range[]>();

/** What stands in for hidden text in each editor now; for the tests. */
const standInsShown = new Map<vscode.TextEditor, string[]>();

function showDisplay(params: Display): void {
  const key = vscode.Uri.parse(params.uri).toString();
  displays.set(key, params);
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document.uri.toString() === key) {
      void conceal(editor);
    }
  }
}

async function hiddenIn(document: vscode.TextDocument): Promise<vscode.Range[]> {
  const key = document.uri.toString();
  const known = hiddenRanges.get(key);
  if (known && known.version === document.version) {
    return known.ranges;
  }
  const version = document.version;
  const legend = await vscode.commands.executeCommand<vscode.SemanticTokensLegend | undefined>(
    'vscode.provideDocumentSemanticTokensLegend',
    document.uri,
  );
  const tokens = await vscode.commands.executeCommand<vscode.SemanticTokens | undefined>(
    'vscode.provideDocumentSemanticTokens',
    document.uri,
  );
  const bit = legend?.tokenModifiers.indexOf('hidden') ?? -1;
  const ranges: vscode.Range[] = [];
  if (tokens && bit >= 0) {
    let line = 0;
    let character = 0;
    for (let i = 0; i < tokens.data.length; i += 5) {
      const [deltaLine, deltaStart, length, , modifiers] = tokens.data.slice(i, i + 5);
      line += deltaLine;
      character = deltaLine === 0 ? character + deltaStart : deltaStart;
      if (modifiers & (1 << bit)) {
        ranges.push(new vscode.Range(line, character, line, character + length));
      }
    }
  }
  // Tokens of an older version would hide the wrong text.
  if (document.version === version) {
    hiddenRanges.set(key, { version, ranges });
  }
  return ranges;
}

async function conceal(editor: vscode.TextEditor): Promise<void> {
  const enabled = vscode.workspace.getConfiguration('org-lsp').get<boolean>('conceal', true);
  if (editor.document.languageId !== 'org') {
    return;
  }
  const ranges = enabled ? await hiddenIn(editor.document) : [];
  const editing = (range: vscode.Range) =>
    editor.selections.some((selection) => selection.start.line <= range.end.line && range.start.line <= selection.end.line);
  const shown = ranges.filter((range) => !editing(range));
  // What stands in for hidden text, for this version of the text; for
  // another, it waits for the server's.
  const display = displays.get(editor.document.uri.toString());
  if (!display || display.version === editor.document.version) {
    const standIns: vscode.DecorationOptions[] = [];
    for (const [line, character, endLine, endCharacter, contentText] of enabled ? (display?.items ?? []) : []) {
      const range = new vscode.Range(line, character, endLine, endCharacter);
      if (!editing(range)) {
        shown.push(range);
        const at = new vscode.Range(range.start, range.start);
        standIns.push({ range: at, renderOptions: { before: { contentText } } });
      }
    }
    editor.setDecorations(standInDecoration, standIns);
    standInsShown.set(editor, standIns.map((standIn) => standIn.renderOptions?.before?.contentText ?? ''));
  } else {
    shown.push(...(hiddenStandIns.get(editor) ?? []).filter((range) => !editing(range)));
  }
  hiddenStandIns.set(editor, shown.filter((range) => !ranges.includes(range)));
  editor.setDecorations(hiddenDecoration, shown);
  concealedText.set(editor, shown.map((range) => editor.document.getText(range)));
}

function concealVisible(): void {
  for (const editor of vscode.window.visibleTextEditors) {
    void conceal(editor);
  }
}

/** Conceal again as documents, cursors and settings change. */
function watchConcealing(context: vscode.ExtensionContext): void {
  let pending: NodeJS.Timeout | undefined;
  context.subscriptions.push(
    hiddenDecoration,
    standInDecoration,
    vscode.window.onDidChangeVisibleTextEditors(concealVisible),
    vscode.window.onDidChangeTextEditorSelection((event) => void conceal(event.textEditor)),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.languageId !== 'org') {
        return;
      }
      // The server's tokens for the new text come a moment later.
      clearTimeout(pending);
      pending = setTimeout(concealVisible, 200);
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('org-lsp.conceal')) {
        concealVisible();
      }
    }),
  );
}

// ----- virtual indentation

/**
 * `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`): the server
 * says how far each line shows indented, `[line, prefix, wrap, hidden
 * stars]`, for a version of the document. The prefix is a decoration
 * before the line; VS Code has no way to indent the lines a long line
 * wraps into. Hidden leading stars keep their place, invisible.
 */
interface VirtualIndent {
  uri: string;
  version: number;
  lines: number[][];
}

const virtualIndents = new Map<string, VirtualIndent>();
const prefixDecoration = vscode.window.createTextEditorDecorationType({});
const hiddenStarDecoration = vscode.window.createTextEditorDecorationType({ opacity: '0' });

/** The prefix of each line and its hidden stars, by editor; for the tests. */
const virtualIndentShown = new Map<vscode.TextEditor, number[][]>();

function showVirtualIndent(params: VirtualIndent): void {
  const key = vscode.Uri.parse(params.uri).toString();
  virtualIndents.set(key, params);
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document.uri.toString() === key) {
      indentVirtually(editor);
    }
  }
}

function indentVirtually(editor: vscode.TextEditor): void {
  const known = virtualIndents.get(editor.document.uri.toString());
  // For another version of the text, the decorations stay as they are
  // until the prefixes of this one come.
  if (known && known.version !== editor.document.version) {
    return;
  }
  const prefixes: vscode.DecorationOptions[] = [];
  const stars: vscode.Range[] = [];
  const shown: number[][] = [];
  for (const [line, prefix, , hidden] of known?.lines ?? []) {
    if (line >= editor.document.lineCount) {
      continue;
    }
    if (prefix > 0) {
      const contentText = '\u00a0'.repeat(prefix);
      prefixes.push({ range: new vscode.Range(line, 0, line, 0), renderOptions: { before: { contentText } } });
    }
    if (hidden > 0) {
      stars.push(new vscode.Range(line, 0, line, hidden));
    }
    if (prefix > 0 || hidden > 0) {
      shown.push([line, prefix, hidden]);
    }
  }
  editor.setDecorations(prefixDecoration, prefixes);
  editor.setDecorations(hiddenStarDecoration, stars);
  virtualIndentShown.set(editor, shown);
}

// ----- line highlights

/**
 * `org-lsp/lineHighlights`: lines highlighted from edge to edge: the
 * agenda's clocked entry, and headlines with
 * `org_fontify_whole_heading_line`.
 */
interface LineHighlights {
  uri: string;
  lines: { line: number; highlight: string }[];
}

const lineHighlights = new Map<string, LineHighlights>();
/**
 * A theme's token colours have no background: a decoration's has. A
 * headline's line takes the background of its level's colour
 * (`org.headlineLevel1Background` …), none unless the user gives one, as
 * Org's `org-level-N` faces have none.
 */
const lineDecorations: Record<string, vscode.TextEditorDecorationType> = {
  agendaClocking: vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    backgroundColor: new vscode.ThemeColor('org.agendaClockingBackground'),
  }),
  ...Object.fromEntries(
    [1, 2, 3, 4, 5, 6, 7, 8].map((level) => [
      `headline.level${level}`,
      vscode.window.createTextEditorDecorationType({
        isWholeLine: true,
        backgroundColor: new vscode.ThemeColor(`org.headlineLevel${level}Background`),
      }),
    ]),
  ),
};

/** The lines of each editor highlighted, by highlight; for the tests. */
const lineHighlightsShown = new Map<vscode.TextEditor, [number, string][]>();

function showLineHighlights(params: LineHighlights): void {
  const key = vscode.Uri.parse(params.uri).toString();
  lineHighlights.set(key, params);
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document.uri.toString() === key) {
      highlightLines(editor);
    }
  }
}

function highlightLines(editor: vscode.TextEditor): void {
  const known = lineHighlights.get(editor.document.uri.toString());
  const shown: [number, string][] = [];
  for (const [highlight, decoration] of Object.entries(lineDecorations)) {
    const lines = (known?.lines ?? []).filter(
      (line) => line.highlight === highlight && line.line < editor.document.lineCount,
    );
    editor.setDecorations(
      decoration,
      lines.map((line) => new vscode.Range(line.line, 0, line.line, 0)),
    );
    shown.push(...lines.map((line): [number, string] => [line.line, highlight]));
  }
  lineHighlightsShown.set(editor, shown);
}

// ----- habits

/**
 * The days of a habit's consistency graph in the agenda, in the
 * background colors of org-habit's faces (`org-habit-clear-face` …
 * `org-habit-overdue-future-face`), which a theme's token colors cannot
 * give: decorations over the server's `agendaHabit` tokens, in the text's
 * own color as Org shows them.
 */
const HABIT_FACES = ['Clear', 'Ready', 'Alert', 'Overdue'].flatMap((kind) => [kind, `${kind}Future`]);
const habitDecorations: Record<string, vscode.TextEditorDecorationType> = Object.fromEntries(
  HABIT_FACES.map((face) => [
    face,
    vscode.window.createTextEditorDecorationType({
      backgroundColor: new vscode.ThemeColor(`org.agendaHabit${face}Background`),
      color: new vscode.ThemeColor('editor.foreground'),
    }),
  ]),
);
/** The days of each agenda document, `[line, start, end, face]`, from its tokens. */
const habitDays = new Map<string, [number, number, number, string][]>();
/** For the tests: the days each editor shows, `[line, character, face]`. */
const habitDaysShown = new WeakMap<vscode.TextEditor, [number, number, string][]>();

/** Take the habits' days from the tokens of `document` as they pass by. */
function habitsOf(document: vscode.TextDocument, tokens: vscode.SemanticTokens | null | undefined): void {
  const legend = client?.initializeResult?.capabilities.semanticTokensProvider?.legend;
  if (!tokens || !legend || document.languageId !== 'org-agenda') {
    return;
  }
  const habit = legend.tokenTypes.indexOf('agendaHabit');
  const bit = (name: string) => 1 << legend.tokenModifiers.indexOf(name);
  const days: [number, number, number, string][] = [];
  const data = tokens.data;
  let line = 0;
  let character = 0;
  for (let i = 0; i + 4 < data.length; i += 5) {
    line += data[i];
    character = data[i] === 0 ? character + data[i + 1] : data[i + 1];
    if (data[i + 3] !== habit) {
      continue;
    }
    const modifiers = data[i + 4];
    const kind = ['ready', 'alert', 'overdue'].find((name) => modifiers & bit(name)) ?? 'clear';
    const face = kind[0].toUpperCase() + kind.slice(1) + (modifiers & bit('future') ? 'Future' : '');
    days.push([line, character, character + data[i + 2], face]);
  }
  habitDays.set(document.uri.toString(), days);
  vscode.window.visibleTextEditors.filter((editor) => editor.document === document).forEach(showHabits);
}

function showHabits(editor: vscode.TextEditor): void {
  const days = habitDays.get(editor.document.uri.toString()) ?? [];
  for (const face of HABIT_FACES) {
    editor.setDecorations(
      habitDecorations[face],
      days.filter((day) => day[3] === face).map(([line, start, end]) => new vscode.Range(line, start, line, end)),
    );
  }
  habitDaysShown.set(
    editor,
    days.map(([line, start, , face]) => [line, start, face]),
  );
}

// ----- images

/**
 * What a preview command (`org.link.preview`, `org.latex.preview`) or
 * `org-lsp/images` says: `remove` true takes away the previews of `kind`
 * in `range`; false shows `items` there, anew; null (a link or a fragment
 * at the cursor) toggles them.
 */
interface Preview {
  kind: 'link' | 'latex';
  uri: string;
  scope: string;
  range: { start: { line: number; character: number }; end: { line: number; character: number } };
  remove: boolean | null;
  includeLinked?: boolean;
  items?: { range: Preview['range']; path: string; width?: ImageSize; maxWidth?: ImageSize }[];
  startup?: boolean;
}

/**
 * A width the server gives an image (`org-image-actual-width`,
 * `org-image-max-width`): pixels, a part of the window's text, or columns.
 */
interface ImageSize {
  pixels?: number;
  fraction?: number;
  columns?: number;
}

/** An image file: as a `data:` URI, its type and its size in pixels. */
interface ImageFile {
  data: string;
  width: number;
  height: number;
}

/**
 * A preview shown, as Emacs keeps its overlays: where its text is (as
 * offsets, which follow the edits around it), that text, and its image.
 */
interface ImageShown {
  kind: Preview['kind'];
  path: string;
  start: number;
  end: number;
  text: string;
  file: ImageFile;
  /** How wide the server says it shows, or at most. */
  sizing: { width?: ImageSize; maxWidth?: ImageSize };
  /** The image as the editor draws it: an SVG of the size it takes in the line. */
  icon: vscode.Uri;
  width: number;
  height: number;
}

/** The previews of each document, by URI. */
const imagesShown = new Map<string, ImageShown[]>();
/**
 * An image in place of its link or fragment. VS Code cannot leave lines
 * free for an image, as Emacs and Neovim do: it takes the height of a
 * line, and shows whole on hover. On a line being edited the text shows,
 * the image after it.
 */
const imageDecoration = vscode.window.createTextEditorDecorationType({});
const imageHiddenText = vscode.window.createTextEditorDecorationType({ textDecoration: 'none; display: none;' });

/** The width and height of an image in pixels, from its header, and its type. */
function imageSize(bytes: Buffer, file: string): { type: string; width: number; height: number } | undefined {
  const ascii = (at: number, length: number) => bytes.toString('latin1', at, at + length);
  if (bytes.length >= 24 && ascii(1, 3) === 'PNG') {
    return { type: 'image/png', width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
  }
  if (bytes.length >= 10 && ascii(0, 4) === 'GIF8') {
    return { type: 'image/gif', width: bytes.readUInt16LE(6), height: bytes.readUInt16LE(8) };
  }
  if (bytes.length >= 26 && ascii(0, 2) === 'BM') {
    return { type: 'image/bmp', width: bytes.readInt32LE(18), height: Math.abs(bytes.readInt32LE(22)) };
  }
  if (bytes.length >= 30 && ascii(0, 4) === 'RIFF' && ascii(8, 4) === 'WEBP') {
    const chunk = ascii(12, 4);
    if (chunk === 'VP8 ') {
      return { type: 'image/webp', width: bytes.readUInt16LE(26) & 0x3fff, height: bytes.readUInt16LE(28) & 0x3fff };
    }
    if (chunk === 'VP8L') {
      const bits = bytes.readUInt32LE(21);
      return { type: 'image/webp', width: (bits & 0x3fff) + 1, height: ((bits >> 14) & 0x3fff) + 1 };
    }
    if (chunk === 'VP8X') {
      return { type: 'image/webp', width: bytes.readUIntLE(24, 3) + 1, height: bytes.readUIntLE(27, 3) + 1 };
    }
  }
  if (bytes.length >= 4 && bytes[0] === 0xff && bytes[1] === 0xd8) {
    // The frame's header, among the segments.
    let at = 2;
    while (at + 9 < bytes.length && bytes[at] === 0xff) {
      const marker = bytes[at + 1];
      if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) {
        return { type: 'image/jpeg', width: bytes.readUInt16BE(at + 7), height: bytes.readUInt16BE(at + 5) };
      }
      at += 2 + bytes.readUInt16BE(at + 2);
    }
    return undefined;
  }
  if (/\.svgz?$/i.test(file) || /^\s*(<\?xml[^>]*>\s*)?(<!--[\s\S]*?-->\s*)*<svg/i.test(ascii(0, 512))) {
    const svg = bytes.toString('utf8');
    const tag = /<svg\b[^>]*>/i.exec(svg)?.[0] ?? '';
    const attribute = (name: string) => new RegExp(`\\s${name}\\s*=\\s*["']\\s*([\\d.]+)(px)?\\s*["']`, 'i').exec(tag)?.[1];
    const box = /\sviewBox\s*=\s*["']\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(tag);
    const width = Number(attribute('width') ?? box?.[1] ?? 0);
    const height = Number(attribute('height') ?? box?.[2] ?? 0);
    return width > 0 && height > 0 ? { type: 'image/svg+xml', width, height } : undefined;
  }
  return undefined;
}

/** The image file `file`, for as long as it stays as it is; none where VS Code cannot show it. */
const imageFiles = new Map<string, { modified: number; file: ImageFile | undefined }>();

async function imageFile(file: string): Promise<ImageFile | undefined> {
  try {
    const modified = (await fs.promises.stat(file)).mtimeMs;
    const known = imageFiles.get(file);
    if (known && known.modified === modified) {
      return known.file;
    }
    const bytes = await fs.promises.readFile(file);
    const size = imageSize(bytes, file);
    const found = size && size.width > 0 && size.height > 0
      ? { data: `data:${size.type};base64,${bytes.toString('base64')}`, width: size.width, height: size.height }
      : undefined;
    imageFiles.set(file, { modified, file: found });
    return found;
  } catch {
    return undefined;
  }
}

/** The height of a line of the editor, in pixels (`editor.lineHeight`). */
function editorLineHeight(): number {
  const editor = vscode.workspace.getConfiguration('editor');
  const fontSize = editor.get<number>('fontSize', 14);
  const lineHeight = editor.get<number>('lineHeight', 0);
  if (lineHeight <= 0) {
    return Math.round(fontSize * (process.platform === 'darwin' ? 1.5 : 1.35));
  }
  return Math.round(lineHeight < 8 ? fontSize * lineHeight : lineHeight);
}

/**
 * The size image `file` takes in a line: the line's height, or less for a
 * smaller one, its width as its proportions say; and the image at that
 * size, an SVG around it.
 */
function lineImage(file: ImageFile): { icon: vscode.Uri; width: number; height: number } {
  const height = Math.max(1, Math.min(file.height, editorLineHeight() - 2));
  const width = Math.max(1, Math.round((file.width * height) / file.height));
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${width}" height="${height}">` +
    `<image width="${width}" height="${height}" preserveAspectRatio="xMidYMid meet" href="${file.data}" xlink:href="${file.data}"/></svg>`;
  return { icon: vscode.Uri.parse(`data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`), width, height };
}

function previewDocument(uri: string): vscode.TextDocument | undefined {
  return vscode.workspace.textDocuments.find((document) => sameUri(document.uri.toString(), uri));
}

/** Apply what the server says of previews (see `Preview`), and say what Emacs says, unless `quiet`. */
async function applyPreview(preview: Preview, quiet = false): Promise<void> {
  const document = previewDocument(preview.uri);
  if (!document) {
    return;
  }
  const key = document.uri.toString();
  const version = document.version;
  const offset = (position: { line: number; character: number }) =>
    document.offsetAt(new vscode.Position(position.line, position.character));
  const tell = (message: string) => !quiet && say(message);
  const loaded = await Promise.all((preview.items ?? []).map((item) => imageFile(item.path)));
  // The text changed while the images were read: the previews are for
  // text that is no more.
  if (document.version !== version) {
    return;
  }
  const from = offset(preview.range.start);
  const to = offset(preview.range.end);
  const list = imagesShown.get(key) ?? [];
  const overlapping = (entry: ImageShown) =>
    entry.kind === preview.kind && !(entry.end < from || (entry.end === from && entry.start < from)) && !(entry.start >= to);
  const old = list.filter(overlapping);
  const kept = list.filter((entry) => !overlapping(entry));
  const link = preview.kind === 'link';
  if (preview.remove === true || (preview.remove === null && old.length > 0)) {
    imagesShown.set(key, kept);
    if (link) {
      tell(`[${preview.scope}] Inline link previews turned off (removed ${old.length} images)`);
    } else if (preview.scope === 'buffer') {
      tell('LaTeX previews removed from buffer');
    } else if (old.length > 0 && preview.remove === null) {
      tell('LaTeX preview removed');
    }
  } else {
    let count = 0;
    (preview.items ?? []).forEach((item, index) => {
      const file = loaded[index];
      if (!file) {
        return;
      }
      const start = offset(item.range.start);
      const end = offset(item.range.end);
      const text = document.getText(new vscode.Range(document.positionAt(start), document.positionAt(end)));
      const sizing = { width: item.width, maxWidth: item.maxWidth };
      kept.push({ kind: preview.kind, path: item.path, start, end, text, file, sizing, ...lineImage(file) });
      count += 1;
    });
    kept.sort((a, b) => a.start - b.start);
    imagesShown.set(key, kept);
    if (link) {
      if (count > 0) {
        tell(`[${preview.scope}] Displaying ${count} images inline ${preview.includeLinked ? '(including images with description)' : ''}`.trimEnd());
      } else if (preview.scope === 'buffer') {
        tell(`[${preview.scope}] No images to display inline`);
      } else {
        tell(`[${preview.scope}] No images to display inline.  Use \`C-u C-u' or 11 argument to preview the whole buffer`);
      }
    }
  }
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document === document) {
      drawImages(editor);
    }
  }
}

function drawImages(editor: vscode.TextEditor): void {
  const document = editor.document;
  const list = imagesShown.get(document.uri.toString()) ?? [];
  const images: vscode.DecorationOptions[] = [];
  const hidden: vscode.Range[] = [];
  for (const entry of list) {
    const range = new vscode.Range(document.positionAt(entry.start), document.positionAt(entry.end));
    const editing = editor.selections.some(
      (selection) => selection.start.line <= range.end.line && range.start.line <= selection.end.line,
    );
    if (!editing) {
      hidden.push(range);
    }
    const at = editing ? range.end : range.start;
    images.push({
      range: new vscode.Range(at, at),
      renderOptions: {
        [editing ? 'after' : 'before']: {
          contentIconPath: entry.icon,
          width: `${entry.width}px`,
          height: `${entry.height}px`,
          margin: editing ? '0 0 0 0.3em' : undefined,
          textDecoration: 'none; vertical-align: middle;',
        },
      },
    });
  }
  editor.setDecorations(imageHiddenText, hidden);
  editor.setDecorations(imageDecoration, images);
}

/**
 * The previews of a document follow its text as it is edited, and one
 * whose own text changes goes, as Emacs takes away an overlay then.
 */
function imagesChanged(event: vscode.TextDocumentChangeEvent): void {
  const key = event.document.uri.toString();
  const list = imagesShown.get(key);
  if (!list || event.contentChanges.length === 0) {
    return;
  }
  // The changes are all of the text before them: the last one first.
  const changes = [...event.contentChanges].sort((a, b) => b.rangeOffset - a.rangeOffset);
  let kept = list;
  for (const change of changes) {
    const end = change.rangeOffset + change.rangeLength;
    const delta = change.text.length - change.rangeLength;
    kept = kept.flatMap((entry) => {
      // Text typed just before or after it is not its own.
      if (entry.end <= change.rangeOffset) {
        return [entry];
      }
      if (entry.start >= end) {
        return [{ ...entry, start: entry.start + delta, end: entry.end + delta }];
      }
      return [];
    });
  }
  // What is left still reads as it did.
  kept = kept.filter(
    (entry) =>
      event.document.getText(new vscode.Range(event.document.positionAt(entry.start), event.document.positionAt(entry.end))) === entry.text,
  );
  imagesShown.set(key, kept);
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document === event.document) {
      drawImages(editor);
    }
  }
}

/**
 * How wide the image of a preview shows on hover, in pixels: as wide as
 * the server says (`org-image-actual-width`), else its own width up to the
 * most it says (`org-image-max-width`), and never more than 600, the
 * hover's width. Columns are those of the editor's font.
 */
function hoverWidth(entry: ImageShown): number {
  const hover = 600;
  const column = vscode.workspace.getConfiguration('editor').get<number>('fontSize', 14) * 0.6;
  const pixels = (size?: ImageSize) =>
    size?.pixels ?? (size?.fraction !== undefined ? size.fraction * hover : size?.columns !== undefined ? size.columns * column : undefined);
  const wanted = pixels(entry.sizing.width);
  const shown = wanted ?? Math.min(entry.file.width, pixels(entry.sizing.maxWidth) ?? entry.file.width);
  return Math.max(1, Math.round(Math.min(shown, hover)));
}

/** The image of a preview, whole, on hover over it. */
const imageHover: vscode.HoverProvider = {
  provideHover(document, position) {
    const at = document.offsetAt(position);
    const entry = (imagesShown.get(document.uri.toString()) ?? []).find((shown) => shown.start <= at && at <= shown.end);
    if (!entry) {
      return undefined;
    }
    const width = hoverWidth(entry);
    // The file itself, not its data: VS Code cuts a hover's text short
    // past 100,000 characters.
    const markdown = new vscode.MarkdownString(`![](${vscode.Uri.file(entry.path).toString()}|width=${width})`);
    return new vscode.Hover(markdown, new vscode.Range(document.positionAt(entry.start), document.positionAt(entry.end)));
  },
};

/** The colors LaTeX is drawn in: the theme's text, on no background. */
function latexColors(): { foreground: string; background: string } {
  const kind = vscode.window.activeColorTheme.kind;
  const dark = kind === vscode.ColorThemeKind.Dark || kind === vscode.ColorThemeKind.HighContrast;
  const configured = vscode.workspace.getConfiguration('org-lsp').get<string>('latexForeground', '').trim();
  return { foreground: configured || (dark ? '#d4d4d4' : '#000000'), background: 'Transparent' };
}

/**
 * The previews a file opens with: `#+STARTUP: inlineimages` (or
 * `org_startup_with_link_previews`), `latexpreview`. Those of LaTeX
 * come in `org-lsp/images` once Emacs has made them.
 */
async function startupPreviews(running: LanguageClient, document: vscode.TextDocument): Promise<void> {
  if (document.languageId !== 'org' || document.uri.scheme !== 'file' || !imagesEnabled()) {
    return;
  }
  const context = {
    textDocument: { uri: document.uri.toString() },
    position: { line: 0, character: 0 },
    startup: true,
  };
  const links = (await running
    .sendRequest('workspace/executeCommand', { command: 'org.link.preview', arguments: [context] })
    .catch(() => undefined)) as { preview?: Preview } | null | undefined;
  if (links?.preview) {
    await applyPreview(links.preview, true);
  }
  await running
    .sendRequest('workspace/executeCommand', { command: 'org.latex.preview', arguments: [{ ...context, ...latexColors() }] })
    .catch(() => undefined);
}

function imagesEnabled(): boolean {
  return vscode.workspace.getConfiguration('org-lsp').get<boolean>('images', true);
}

// ----- headline levels

/**
 * `org-lsp.headlineLevelColors`: the title and stars of a headline as a
 * token of the type `outline`, with its level, which takes the theme's colors
 * of code as Emacs's `outline-N` faces do (function, variable, keyword,
 * comment …), in place of the one heading color themes give every level.
 * Done and TODO headlines past their keyword, archived ones and inline tasks
 * carry no level and keep their own look. The types and modifiers are the
 * server's legend.
 */
function levelColored(tokens: vscode.SemanticTokens | null | undefined): vscode.SemanticTokens | null | undefined {
  const legend = client?.initializeResult?.capabilities.semanticTokensProvider?.legend;
  if (!tokens || !legend || !vscode.workspace.getConfiguration('org-lsp').get<boolean>('headlineLevelColors', false)) {
    return tokens;
  }
  const outline = legend.tokenTypes.indexOf('outline');
  const headlines = ['headline', 'stars'].map((type) => legend.tokenTypes.indexOf(type)).filter((index) => index >= 0);
  const levels = legend.tokenModifiers.reduce((bits, name, bit) => (/^level[1-8]$/.test(name) ? bits | (1 << bit) : bits), 0);
  if (outline < 0 || levels === 0) {
    return tokens;
  }
  const data = new Uint32Array(tokens.data);
  for (let i = 0; i + 4 < data.length; i += 5) {
    if (headlines.includes(data[i + 3]) && (data[i + 4] & levels) !== 0) {
      data[i + 3] = outline;
    }
  }
  return new vscode.SemanticTokens(data, tokens.resultId);
}

/** Has VS Code ask for the tokens of the open documents again. */
function refreshTokens(): void {
  const feature = client?.getFeature('textDocument/semanticTokens');
  const providers = new Set(vscode.workspace.textDocuments.map((document) => feature?.getProvider(document)));
  providers.forEach((provider) => provider?.onDidChangeSemanticTokensEmitter.fire());
}

// ----- sparse trees

const sparseDecoration = vscode.window.createTextEditorDecorationType({
  backgroundColor: new vscode.ThemeColor('editor.findMatchHighlightBackground'),
});

/** The matches of the sparse tree each document shows; for the tests. */
const sparseShown = new Map<string, { ranges: vscode.Range[]; keep: boolean }>();

/**
 * A sparse tree (`org-sparse-tree`) in the editor at hand: everything
 * folded, then the regions around each of its lines unfolded, as
 * `org-occur` shows its matches; the matches highlighted until the text
 * changes, or with `keepHighlights` until they are replaced. With `keep`
 * (`org-reveal`), only the folds that start on the lines of `unfold` open;
 * the others stay as they are, and `highlights`, when given (`C-c C-c`),
 * replace those shown.
 */
async function showSparse(running: LanguageClient, result: unknown): Promise<void> {
  type Sparse = { lines?: number[]; highlights?: unknown[]; keepHighlights?: boolean; keep?: boolean; unfold?: number[] };
  const sparse = (result as { sparse?: Sparse } | null)?.sparse;
  const editor = vscode.window.activeTextEditor;
  if (!sparse || !editor) {
    return;
  }
  if (!sparse.keep || sparse.highlights) {
    const ranges = (sparse.highlights ?? [])
      .map((range) =>
        running.protocol2CodeConverter.asRange(range as Parameters<typeof running.protocol2CodeConverter.asRange>[0]),
      )
      .filter((range): range is vscode.Range => range !== undefined);
    editor.setDecorations(sparseDecoration, ranges);
    sparseShown.set(editor.document.uri.toString(), { ranges, keep: sparse.keepHighlights === true });
  }
  if (sparse.keep) {
    const unfold = sparse.unfold ?? [];
    if (unfold.length > 0) {
      await vscode.commands.executeCommand('editor.unfold', { levels: 1, selectionLines: unfold });
    }
    return;
  }
  await vscode.commands.executeCommand('editor.foldAll');
  const lines = sparse.lines ?? [];
  if (lines.length > 0) {
    await vscode.commands.executeCommand('editor.unfold', { levels: 100, direction: 'up', selectionLines: lines });
  }
}

/**
 * Org takes the highlighting away at the next change, unless the server
 * says it stays (`org-remove-highlights-with-change`).
 */
function clearSparse(event: vscode.TextDocumentChangeEvent): void {
  const key = event.document.uri.toString();
  const shown = sparseShown.get(key);
  if (event.contentChanges.length === 0 || !shown || shown.keep) {
    return;
  }
  sparseShown.delete(key);
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document.uri.toString() === key) {
      editor.setDecorations(sparseDecoration, []);
    }
  }
}

// ----- visibility cycling

/**
 * Org's TAB and S-TAB (`org-cycle`, `org-global-cycle`) over the server's
 * folding ranges. VS Code does not say which folds are closed: a fold is
 * closed when the line after its first is hidden, which `visibleRanges`
 * shows. As in Emacs, the step after CHILDREN depends on the last command
 * having been the same cycle.
 */
type CycleState = 'folded' | 'children' | 'subtree' | 'overview' | 'contents' | 'all';

let lastCycle: { uri: string; version: number; line: number; state: CycleState } | undefined;

const CYCLE_MESSAGES: Record<CycleState, string> = {
  folded: 'FOLDED',
  children: 'CHILDREN',
  subtree: 'SUBTREE',
  overview: 'OVERVIEW',
  contents: 'CONTENTS...done',
  all: 'SHOW ALL',
};

function remember(editor: vscode.TextEditor, line: number, state: CycleState): void {
  lastCycle = { uri: editor.document.uri.toString(), version: editor.document.version, line, state };
  vscode.window.setStatusBarMessage(CYCLE_MESSAGES[state], 2000);
}

/** The state the last cycle left at `line` of `editor`, if it was the last command there. */
function lastState(editor: vscode.TextEditor, line: number): CycleState | undefined {
  const last = lastCycle;
  const same =
    last && last.uri === editor.document.uri.toString() && last.version === editor.document.version && last.line === line;
  return same ? last.state : undefined;
}

async function foldingRanges(document: vscode.TextDocument): Promise<vscode.FoldingRange[]> {
  const ranges = await vscode.commands.executeCommand<vscode.FoldingRange[] | undefined>(
    'vscode.executeFoldingRangeProvider',
    document.uri,
  );
  return (ranges ?? []).filter((range) => range.end > range.start);
}

const visible = (editor: vscode.TextEditor, line: number) =>
  editor.visibleRanges.some((range) => range.start.line <= line && line <= range.end.line);

/** Whether the fold that starts at `line` is closed. */
async function closed(editor: vscode.TextEditor, line: number): Promise<boolean> {
  if (visible(editor, line + 1)) {
    return false;
  }
  // The next line is hidden by a fold, or below the screen: with the line
  // in the middle of the screen, it is in view unless folded.
  const last = editor.visibleRanges[editor.visibleRanges.length - 1];
  if (last && last.end.line <= line) {
    const moved = new Promise<void>((resolve) => {
      const done = vscode.window.onDidChangeTextEditorVisibleRanges((event) => {
        if (event.textEditor === editor) {
          done.dispose();
          resolve();
        }
      });
      setTimeout(() => {
        done.dispose();
        resolve();
      }, 300);
    });
    editor.revealRange(new vscode.Range(line, 0, line, 0), vscode.TextEditorRevealType.InCenter);
    await moved;
    return !visible(editor, line + 1);
  }
  return true;
}

/** org-journal's directory, when it is on (`orgJournal`). */
let journalDirectory: string | undefined;

/**
 * A file of org-journal's directory without an extension is an Org file
 * (`org-journal-is-journal`), whatever VS Code makes of its name.
 */
async function asJournalFile(document: vscode.TextDocument): Promise<void> {
  const directory = journalDirectory;
  if (!directory || document.uri.scheme !== 'file' || document.languageId === 'org') {
    return;
  }
  const path = document.uri.fsPath;
  if (path.startsWith(directory) && !path.slice(directory.length).includes('.')) {
    await vscode.languages.setTextDocumentLanguage(document, 'org');
  }
}

/** The headlines' folds to open and close (`org-lsp/folds`). */
interface JournalFolds {
  uri: string;
  open?: { startLine: number }[];
  close?: { startLine: number }[];
}

/** When each document last changed, for the folds that follow an edit. */
const lastChanged = new Map<string, number>();

/**
 * The headlines of a document org-journal showed, folded as it folds
 * them, or the subtrees a yank or paste folded right after its edit, once
 * the editor shows it as an Org file with its folds. VS Code's folds
 * follow an edit a moment later.
 */
async function journalFolds(params: JournalFolds): Promise<void> {
  let editor: vscode.TextEditor | undefined;
  for (let tries = 0; tries < 60; tries++) {
    editor = vscode.window.activeTextEditor;
    if (editor && sameUri(editor.document.uri.toString(), params.uri) && editor.document.languageId === 'org') {
      break;
    }
    editor = undefined;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (!editor) {
    return;
  }
  const since = Date.now() - (lastChanged.get(editor.document.uri.toString()) ?? 0);
  if (since < 600) {
    await new Promise((resolve) => setTimeout(resolve, 600 - since));
  }
  await foldingRanges(editor.document);
  const open = (params.open ?? []).map((range) => range.startLine);
  if (open.length > 0) {
    await vscode.commands.executeCommand('editor.unfold', { levels: 1, selectionLines: open });
  }
  await fold((params.close ?? []).map((range) => range.startLine));
}

const fold = (lines: number[]) =>
  lines.length > 0
    ? vscode.commands.executeCommand('editor.fold', { levels: 1, direction: 'up', selectionLines: lines })
    : undefined;
/** The files whose startup folds were closed: once each, as Org sets them when it visits a file. */
const startupFolded = new Set<string>();

/**
 * The folds the server says a file opens with closed (`org_startup_folded`,
 * `#+STARTUP:`, `VISIBILITY` properties), closed the first time the file is
 * the active editor.
 */
async function foldAtStartup(editor: vscode.TextEditor | undefined): Promise<void> {
  const document = editor?.document;
  const key = document?.uri.toString();
  if (!document || !key || document.languageId !== 'org' || startupFolded.has(key) || !client?.isRunning()) {
    return;
  }
  startupFolded.add(key);
  const result = (await client.sendRequest('org-lsp/startupFolds', { textDocument: { uri: key } })) as {
    folds?: { startLine: number }[];
  } | null;
  const lines = (result?.folds ?? []).map((range) => range.startLine);
  if (lines.length === 0) {
    return;
  }
  // The editor folds with the server's ranges once it has them.
  await foldingRanges(document);
  if (vscode.window.activeTextEditor?.document === document) {
    await fold(lines);
  }
}

const unfoldAll = (line: number) =>
  vscode.commands.executeCommand('editor.unfold', { levels: 1000, direction: 'down', selectionLines: [line] });

/** The ranges right inside `outer`: in it, and in no other range in it. */
function childrenOf(outer: vscode.FoldingRange, ranges: vscode.FoldingRange[]): vscode.FoldingRange[] {
  const inside = ranges.filter((range) => range.start > outer.start && range.end <= outer.end);
  return inside.filter(
    (range) => !inside.some((other) => other !== range && other.start < range.start && range.end <= other.end),
  );
}

/** Whether `line` is a headline with the ARCHIVE tag of its own. */
function archivedAt(editor: vscode.TextEditor, line: number): boolean {
  return /^\*+\s(?:.*\s)?:(?:[\p{L}\p{N}_@#%]+:)*ARCHIVE:(?:[\p{L}\p{N}_@#%]+:)*\s*$/u.test(
    editor.document.lineAt(line).text,
  );
}

/**
 * `org-cycle-hide-archived-subtrees`: the starts of the archived subtrees
 * among `open`, the outermost ones, to fold again after cycling opened
 * them. (Folding one in a closed fold would close the fold around it.)
 */
function archivedStarts(editor: vscode.TextEditor, open: vscode.FoldingRange[]): number[] {
  const archived = open.filter((range) => archivedAt(editor, range.start));
  const outermost = archived.filter(
    (range) => !archived.some((other) => other.start < range.start && range.end <= other.end),
  );
  return [...new Set(outermost.map((range) => range.start))];
}

const staysClosed = () => say('Subtree is archived and stays closed');

/**
 * `org-cycle` on the fold that starts at the cursor's line: FOLDED, then
 * CHILDREN (its direct folds closed), then SUBTREE (all open). On a line
 * that starts no fold, the `fallback` commands run (Tab's own), or without
 * them the innermost fold around the cursor cycles.
 */
async function cycle(args?: Partial<KeyArgs>): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  // In a table, TAB goes to the next field (`org-table-next-field`).
  if (atTable(editor)) {
    await vscode.commands.executeCommand('org.table.next.field');
    return;
  }
  // What TAB does before it folds (`org.cycle`): a new, empty entry
  // changes level, the start of the file cycles globally.
  let integrateLists = false;
  const running = client;
  if (running) {
    type Before = { edits?: unknown[]; global?: boolean; integrateLists?: boolean } | null;
    let before: Before;
    try {
      before = (await running.sendRequest('workspace/executeCommand', { command: 'org.cycle', arguments: cursor(running) })) as Before;
    } catch (error) {
      void vscode.window.showErrorMessage(error instanceof Error ? error.message : String(error));
      return;
    }
    if (Array.isArray(before?.edits)) {
      await applyCarried(running, before);
      return;
    }
    if (before?.global) {
      await cycleGlobal();
      return;
    }
    integrateLists = before?.integrateLists === true;
  }
  const ranges = await foldingRanges(editor.document);
  const line = editor.selection.active.line;
  let current = ranges.find((range) => range.start === line);
  if (!current) {
    if (args?.fallback) {
      await fallBack(args.fallback);
      return;
    }
    current = ranges
      .filter((range) => range.start < line && line <= range.end)
      .sort((a, b) => b.start - a.start)[0];
    if (!current) {
      return;
    }
  }
  const start = current.start;
  const archived = archivedAt(editor, start);
  if (await closed(editor, start)) {
    await unfoldAll(start);
    // Lists stay open but with `org_cycle_include_plain_lists` `integrate`.
    const folded = (range: vscode.FoldingRange) =>
      integrateLists || !isItem(editor, range.start) || isItem(editor, start);
    const children = [...new Set(childrenOf(current, ranges).filter(folded).map((range) => range.start))];
    await fold(children);
    // An archived subtree stays closed (`org-cycle-hide-archived-subtrees`).
    if (archived) {
      await fold([start]);
      staysClosed();
    }
    remember(editor, start, children.length > 0 ? 'children' : 'subtree');
  } else if (lastState(editor, start) === 'children') {
    await unfoldAll(start);
    const inside = ranges.filter((range) => range.start >= start && range.end <= current.end);
    await fold(archivedStarts(editor, inside));
    if (archived) {
      staysClosed();
    }
    remember(editor, start, 'subtree');
  } else {
    await fold([start]);
    remember(editor, start, 'folded');
  }
}

/** Whether line `line` starts a list item. */
function isItem(editor: vscode.TextEditor, line: number): boolean {
  return /^(\s*([-+]|\d+[.)])|\s+\*)(\s|$)/.test(editor.document.lineAt(line).text);
}

/** Whether the cursor is on a line of a table. */
function atTable(editor: vscode.TextEditor): boolean {
  return /^\s*\|/.test(editor.document.lineAt(editor.selection.active.line).text);
}

/**
 * `org-global-cycle`: OVERVIEW (the top level only), then CONTENTS (every
 * headline, the entries without subheadings closed), then SHOW ALL.
 */
async function cycleGlobal(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  // In a table, S-TAB goes to the previous field.
  if (atTable(editor)) {
    await vscode.commands.executeCommand('org.table.previous.field');
    return;
  }
  const state = lastState(editor, -1);
  if (state === 'overview') {
    const ranges = await foldingRanges(editor.document);
    const headline = (range: vscode.FoldingRange) => /^\*+\s/.test(editor.document.lineAt(range.start).text);
    const parents = new Set(
      ranges
        .filter(headline)
        .filter((range) => ranges.some((inner) => inner.start > range.start && inner.end <= range.end && headline(inner)))
        .map((range) => range.start),
    );
    await vscode.commands.executeCommand('editor.unfoldAll');
    await fold([...new Set(ranges.map((range) => range.start))].filter((start) => !parents.has(start)));
    // Archived subtrees stay closed, of those left open.
    await fold(archivedStarts(editor, ranges.filter((range) => parents.has(range.start))));
    remember(editor, -1, 'contents');
  } else if (state === 'contents') {
    await vscode.commands.executeCommand('editor.unfoldAll');
    await fold(archivedStarts(editor, await foldingRanges(editor.document)));
    remember(editor, -1, 'all');
  } else {
    await vscode.commands.executeCommand('editor.foldAll');
    remember(editor, -1, 'overview');
  }
}

// ----- help

interface Binding {
  command: string;
  key: string;
  when?: string;
  args?: unknown;
}

/** A key as VS Code's menus write it: `ctrl+c ctrl+t` as `Ctrl+C Ctrl+T`. */
function keyLabel(key: string): string {
  const names: Record<string, string> = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', enter: 'Enter', tab: 'Tab', escape: 'Escape' };
  return key
    .split(' ')
    .map((stroke) =>
      stroke
        .split('+')
        .map((part) => names[part] ?? (part.length === 1 && stroke.includes('+') ? part.toUpperCase() : part))
        .join('+'),
    )
    .join(' ');
}

/**
 * The keys the extension binds where `editor` is, with the preset
 * `org-lsp.keys` picks: `[key, title, command, args]`, in the order of the
 * keybindings. Keys of the user's own are not known to extensions.
 */
function helpItems(
  packageJSON: { contributes: { keybindings: Binding[]; commands: { command: string; title: string }[] }; orgLsp: { capturePrefix: string } },
  editor: vscode.TextEditor | undefined,
): { key: string; title: string; command: string; args?: unknown }[] {
  const preset = vscode.workspace.getConfiguration('org-lsp').get<string>('keys', 'emacs');
  const language = editor?.document.languageId;
  const capture = editor ? path.basename(editor.document.uri.fsPath).startsWith(packageJSON.orgLsp.capturePrefix) : false;
  const titles = new Map(packageJSON.contributes.commands.map((command) => [command.command, command.title]));
  return packageJSON.contributes.keybindings
    .filter((binding) => binding.when?.includes(`config.org-lsp.keys == '${preset}'`))
    .filter((binding) => binding.when?.includes(`editorLangId == '${language}'`))
    .filter((binding) => capture || !binding.when?.includes('resourceFilename'))
    .filter((binding) => !binding.when?.includes('vim.mode') || preset === 'vim')
    .map((binding) => ({ key: keyLabel(binding.key), title: titles.get(binding.command) ?? binding.command, command: binding.command, args: binding.args }));
}

/** `org-lsp.help`: the keys of the editor at hand; picking one runs it. */
async function help(packageJSON: Parameters<typeof helpItems>[0]): Promise<void> {
  const items = helpItems(packageJSON, vscode.window.activeTextEditor);
  if (items.length === 0) {
    const preset = vscode.workspace.getConfiguration('org-lsp').get<string>('keys', 'emacs');
    void vscode.window.showInformationMessage(
      preset === 'custom' ? 'Org: no keys are bound ("org-lsp.keys" is "custom").' : 'Org: no keys are bound here.',
    );
    return;
  }
  const picked = await vscode.window.showQuickPick(
    items.map((item) => ({ label: item.title, description: item.key, item })),
    { placeHolder: 'Org keys', matchOnDescription: true },
  );
  if (picked) {
    await vscode.commands.executeCommand(picked.item.command, ...(picked.item.args === undefined ? [] : [picked.item.args]));
  }
}

/** What the extension offers other code: for its tests. */
export interface Api {
  /** The text hidden in `editor` now. */
  concealed(editor: vscode.TextEditor): string[];
  /** What the status bar shows for the clock: empty when none runs. */
  clock(): string;
  /** What the status bar shows for column view. */
  columns(): string;
  /** The headlines column view covers in `editor`: `[line, text shown]`. */
  columnsShown(editor: vscode.TextEditor): [number, string][];
  /** What the status bar shows for the relative timer or the countdown: empty when none is on. */
  timer(): string;
  /** What stands in for hidden text in `editor` (entities' characters). */
  standIns(editor: vscode.TextEditor): string[];
  /** The lines of `editor` shown indented: `[line, prefix, hidden stars]`. */
  virtualIndent(editor: vscode.TextEditor): number[][];
  /** The lines of `editor` highlighted from edge to edge: `[line, highlight]`. */
  lineHighlights(editor: vscode.TextEditor): [number, string][];
  /** The days of habits' graphs in `editor`, `[line, character, face]`: `Clear`, `OverdueFuture`… */
  habitDays(editor: vscode.TextEditor): [number, number, string][];
  /** The keywords and priorities shown in a face of their own, `[line, text]`. */
  faces(editor: vscode.TextEditor): [number, string][];
  /** The text of the matches of the sparse tree `editor` shows. */
  sparse(editor: vscode.TextEditor): string[];
  /** What `org-lsp.help` lists for `editor`: `[key, title]`. */
  help(editor: vscode.TextEditor): string[][];
  /** The previews of `editor`: `[kind, line, file name, width, height]` each. */
  images(editor: vscode.TextEditor): [string, number, string, number, number][];
  /** The outline path of the agenda line shown last (`org-agenda-show-outline-path`). */
  echoed(): string;
  /** The server's commands run so far, in turn, and how many commands and
   * documents to show are not done yet. */
  commands(): { run: string[]; running: number };
  /** What the server said column view shows, last (`org-lsp/columns`). */
  columnsReported(): unknown;
  /** Done when the server has handled what it was sent before. */
  sync(): Promise<void>;
  /** The server's text of one of its documents (the agenda, the date prompt). */
  content(uri: string): Promise<string | undefined>;
}

export async function activate(context: vscode.ExtensionContext): Promise<Api> {
  iconDirectory = vscode.Uri.joinPath(context.globalStorageUri, 'icons');
  context.subscriptions.push(
    vscode.commands.registerCommand('org-lsp.restartServer', () => restartClient(context)),
    // The server reads its settings once, at start.
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (
        event.affectsConfiguration('org-lsp.settings') ||
        event.affectsConfiguration('org-lsp.server') ||
        event.affectsConfiguration('org-lsp.images')
      ) {
        await restartClient(context);
      }
    }),
  );
  clockItem = vscode.window.createStatusBarItem('org-lsp.clock', vscode.StatusBarAlignment.Left, 50);
  clockItem.name = 'Org clock';
  clockItem.tooltip = 'Org: the clock runs on this entry. Click to go to it.';
  clockItem.command = 'org.clock.goto';
  context.subscriptions.push(clockItem, { dispose: () => clearInterval(clockTimer) });
  columnsItem = vscode.window.createStatusBarItem('org-lsp.columns', vscode.StatusBarAlignment.Left, 48);
  columnsItem.name = 'Org column view';
  columnsItem.tooltip = 'Org: column view, the column picked in brackets. q quits.';
  context.subscriptions.push(columnsItem);
  timerItem = vscode.window.createStatusBarItem('org-lsp.timer', vscode.StatusBarAlignment.Left, 49);
  timerItem.name = 'Org timer';
  timerItem.tooltip = 'Org: the relative timer, or the countdown.';
  context.subscriptions.push(timerItem, { dispose: () => clearInterval(timerTicks) });
  // In the same chain as the restarts, so that a setting changed meanwhile starts no second client.
  restarting = restarting.catch(() => undefined).then(() => startClient(context));
  await restarting;
  watchConcealing(context);
  concealVisible();
  context.subscriptions.push(
    vscode.commands.registerCommand('org-lsp.cycle', cycle),
    vscode.commands.registerCommand('org-lsp.cycleGlobal', cycleGlobal),
    vscode.commands.registerCommand('org-lsp.help', () => help(context.extension.packageJSON)),
    vscode.commands.registerCommand('org-lsp.agenda.close', () => vscode.commands.executeCommand('workbench.action.closeActiveEditor')),
    // A cycle goes on from the last one only when nothing came between.
    vscode.window.onDidChangeActiveTextEditor(() => (lastCycle = undefined)),
    vscode.window.onDidChangeActiveTextEditor(foldAtStartup),
    vscode.window.onDidChangeActiveTextEditor(updateColumnsItem),
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach(drawColumns)),
    vscode.commands.registerCommand('org-lsp.columns.left', () => moveColumn(-1)),
    vscode.commands.registerCommand('org-lsp.columns.right', () => moveColumn(1)),
    vscode.workspace.onDidCloseTextDocument((document) => startupFolded.delete(document.uri.toString())),
  );
  void foldAtStartup(vscode.window.activeTextEditor);
  context.subscriptions.push(
    prefixDecoration,
    hiddenStarDecoration,
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach(indentVirtually)),
    ...Object.values(lineDecorations),
    sparseDecoration,
    vscode.workspace.onDidChangeTextDocument(clearSparse),
    vscode.workspace.onDidChangeTextDocument((event) => lastChanged.set(event.document.uri.toString(), Date.now())),
    vscode.workspace.onDidCloseTextDocument((document) => lastChanged.delete(document.uri.toString())),
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach(highlightLines)),
    ...Object.values(habitDecorations),
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach(showHabits)),
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach((editor) => void paintFaces(editor))),
    vscode.workspace.onDidChangeTextDocument(facesChanged),
    { dispose: () => faceDecorations.forEach((decoration) => decoration.dispose()) },
    imageDecoration,
    imageHiddenText,
    vscode.workspace.onDidChangeTextDocument(imagesChanged),
    vscode.window.onDidChangeTextEditorSelection((event) => drawImages(event.textEditor)),
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach(drawImages)),
    vscode.workspace.onDidCloseTextDocument((document) => imagesShown.delete(document.uri.toString())),
    vscode.languages.registerHoverProvider({ language: 'org' }, imageHover),
    vscode.workspace.onDidOpenTextDocument((document) => {
      void asJournalFile(document);
      // After the client has told the server of it.
      const running = client;
      if (running) {
        setTimeout(() => void startupPreviews(running, document), 50);
      }
    }),
    // Headlines in the colors of their level, or all in the heading's.
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('org-lsp.headlineLevelColors')) {
        refreshTokens();
      }
    }),
    // Images as high as a line.
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('editor.lineHeight') || event.affectsConfiguration('editor.fontSize')) {
        for (const list of imagesShown.values()) {
          list.forEach((entry) => Object.assign(entry, lineImage(entry.file)));
        }
        vscode.window.visibleTextEditors.forEach(drawImages);
      }
    }),
  );
  return {
    concealed: (editor) => concealedText.get(editor) ?? [],
    clock: clockText,
    columns: () => columnsItem?.text ?? '',
    echoed: () => echoed,
    commands: () => ({ run: [...commandsRun], running: busy }),
    columnsReported: () => columnsReported,
    // A request the server does not know is answered all the same, in turn.
    sync: async () => {
      await client?.sendRequest('org-lsp/test-sync').catch(() => undefined);
    },
    content: async (uri) => {
      const result = await client?.sendRequest('workspace/textDocumentContent', { uri }).catch(() => undefined);
      return (result as { text?: string } | undefined)?.text;
    },
    columnsShown: (editor) => (columnsShown.get(editor.document.uri.toString())?.lines ?? []).map(([line, text]) => [line, text]),
    timer: timerText,
    standIns: (editor) => standInsShown.get(editor) ?? [],
    virtualIndent: (editor) => virtualIndentShown.get(editor) ?? [],
    lineHighlights: (editor) => lineHighlightsShown.get(editor) ?? [],
    habitDays: (editor) => habitDaysShown.get(editor) ?? [],
    faces: (editor) => facesShown.get(editor) ?? [],
    sparse: (editor) =>
      (sparseShown.get(editor.document.uri.toString())?.ranges ?? []).map((range) => editor.document.getText(range)),
    help: (editor) => helpItems(context.extension.packageJSON, editor).map((item) => [item.key, item.title]),
    images: (editor) =>
      (imagesShown.get(editor.document.uri.toString()) ?? []).map((entry) => [
        entry.kind,
        editor.document.positionAt(entry.start).line,
        path.basename(entry.path),
        entry.width,
        entry.height,
      ]),
  };
}

export function deactivate(): Promise<void> {
  // After any restart under way, so that the server it starts is stopped too.
  deactivated = true;
  restarting = restarting.catch(() => undefined).then(stopClient);
  return restarting;
}
