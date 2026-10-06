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

/** Where the cursor is, in the form the server's commands take. */
function cursor(running: LanguageClient): unknown[] {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return [];
  }
  return [
    {
      textDocument: running.code2ProtocolConverter.asTextDocumentIdentifier(editor.document),
      position: running.code2ProtocolConverter.asPosition(editor.selection.active),
    },
  ];
}

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
    vscode.window.setStatusBarMessage(message, 5000);
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
  choices?: { key: string; label: string }[];
  /** What to send back with the answer, besides it. */
  context?: Record<string, unknown>;
}

/**
 * Ask what `input` asks for. A key is picked from its choices, the way
 * Org's fast selection offers them; anything else is typed.
 */
async function ask(input: Input, typed: (text: string) => Promise<vscode.InputBoxValidationMessage | undefined>) {
  if (input.choices) {
    // A key shows with its choice, as Org's fast selection lists them.
    const items = input.choices.map((choice) => ({
      label: input.kind === 'key' ? `[${choice.key === ' ' ? 'SPC' : choice.key}] ${choice.label}` : choice.label,
      key: choice.key,
    }));
    const picked = await vscode.window.showQuickPick(items, { placeHolder: input.prompt, ignoreFocusOut: true });
    return picked?.key;
  }
  return vscode.window.showInputBox({
    prompt: input.prompt,
    value: input.value,
    ignoreFocusOut: true,
    validateInput: input.kind === 'date' ? typed : undefined,
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
  const { languages, agendaScheme, capturePrefix, datePromptUri } = context.extension.packageJSON.orgLsp as {
    languages: string[];
    agendaScheme: string;
    capturePrefix: string;
    datePromptUri: string;
  };
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
      executeCommand: async (name, args) => {
        const running = client;
        if (!running) {
          return undefined;
        }
        const params = { command: name, arguments: args.length > 0 ? args : cursor(running) };
        try {
          let result = await running.sendRequest('workspace/executeCommand', params);
          closeAsked(result);
          await saveAsked(result);
          sayAsked(result);
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
            closeAsked(result);
            sayAsked(result);
          }
          return result;
        } catch (error) {
          void vscode.window.showWarningMessage(error instanceof Error ? error.message : String(error));
          return undefined;
        }
      },
      window: {
        // The date prompt opens beside the file, as Emacs pops its calendar
        // up in another window, and stays in that editor as it changes.
        showDocument: async (params, token, next) => {
          if (params.external) {
            return next(params, token);
          }
          if (apart(params.uri)) {
            await showApart(params);
            return { success: true };
          }
          if (!sameUri(params.uri, datePromptUri)) {
            return next(params, token);
          }
          datePromptShown = showDatePrompt(params);
          await datePromptShown;
          return { success: true };
        },
      },
    },
  };
  client = new LanguageClient('org-lsp', 'Org Language Server', serverOptions, clientOptions);
  // This client shows the server's date prompt, and the prefixes of
  // `org-indent-mode`.
  client.registerFeature({
    fillClientCapabilities(capabilities) {
      capabilities.experimental = {
        ...(capabilities.experimental ?? {}),
        orgDatePrompt: true,
        orgVirtualIndent: true,
      };
    },
    initialize() {},
    getState() {
      return { kind: 'static' };
    },
    clear() {},
  });
  try {
    await client.start();
    // Files the server changed from elsewhere (the agenda, a capture, a
    // refile), to save now that the change is made: `autoSave`.
    client.onNotification('org-lsp/save', (params: { uris?: string[] }) => saveAsked({ save: params.uris ?? [] }));
    // The clock that runs, for the status bar.
    client.onNotification('org-lsp/clock', (params: ClockReport | null) => showClock(params));
    // How far the lines of a file with `org-indent-mode` show indented.
    client.onNotification('org-lsp/virtualIndent', (params: VirtualIndent) => showVirtualIndent(params));
  } catch (error) {
    client = undefined;
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(
      `org-lsp: could not start the server (${command}): ${message}. ` +
        'Install `org-lsp` on your PATH or set "org-lsp.server.path".',
    );
  }
}

async function stopClient(): Promise<void> {
  const running = client;
  client = undefined;
  await running?.stop();
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

function showClock(report: ClockReport | null): void {
  clockState = report && typeof report.heading === 'string' ? { ...report, received: Date.now() } : undefined;
  clearInterval(clockTimer);
  clockTimer = clockState ? setInterval(updateClockItem, 30000) : undefined;
  updateClockItem();
}

// ----- concealing

/**
 * The text the server marks `hidden` in its tokens (link brackets and
 * targets, emphasis markers with `hideEmphasisMarkers`) is hidden, as Org
 * hides it. VS Code cannot conceal text; a decoration whose CSS takes the
 * text out of the line is the usual way. On a line with a cursor or a
 * selection it shows, so that it can be edited.
 */
const hiddenDecoration = vscode.window.createTextEditorDecorationType({ textDecoration: 'none; display: none;' });

/** The hidden ranges of each document, for the version they were read for. */
const hiddenRanges = new Map<string, { version: number; ranges: vscode.Range[] }>();

/** What each editor hides now, as text; for the tests. */
const concealedText = new Map<vscode.TextEditor, string[]>();

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
 * `org-indent-mode` (`startupIndented`, `#+STARTUP: indent`): the server
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

/** What the extension offers other code: for its tests. */
export interface Api {
  /** The text hidden in `editor` now. */
  concealed(editor: vscode.TextEditor): string[];
  /** What the status bar shows for the clock: empty when none runs. */
  clock(): string;
  /** The lines of `editor` shown indented: `[line, prefix, hidden stars]`. */
  virtualIndent(editor: vscode.TextEditor): number[][];
}

export async function activate(context: vscode.ExtensionContext): Promise<Api> {
  context.subscriptions.push(
    vscode.commands.registerCommand('org-lsp.restartServer', async () => {
      await stopClient();
      await startClient(context);
    }),
    // The server reads its settings once, at start.
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration('org-lsp.settings') || event.affectsConfiguration('org-lsp.server')) {
        await stopClient();
        await startClient(context);
      }
    }),
  );
  clockItem = vscode.window.createStatusBarItem('org-lsp.clock', vscode.StatusBarAlignment.Left, 50);
  clockItem.name = 'Org clock';
  clockItem.tooltip = 'Org: the clock runs on this entry. Click to go to it.';
  clockItem.command = 'org.clock.goto';
  context.subscriptions.push(clockItem, { dispose: () => clearInterval(clockTimer) });
  await startClient(context);
  watchConcealing(context);
  concealVisible();
  context.subscriptions.push(
    prefixDecoration,
    hiddenStarDecoration,
    vscode.window.onDidChangeVisibleTextEditors((editors) => editors.forEach(indentVirtually)),
  );
  return {
    concealed: (editor) => concealedText.get(editor) ?? [],
    clock: clockText,
    virtualIndent: (editor) => virtualIndentShown.get(editor) ?? [],
  };
}

export function deactivate(): Promise<void> {
  return stopClient();
}
