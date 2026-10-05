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

/** Close the editors of a document the server is done with. */
function closeAsked(result: unknown): void {
  const close = (result as { close?: string } | null)?.close;
  if (typeof close !== 'string') {
    return;
  }
  const tabs = vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .filter((tab) => tab.input instanceof vscode.TabInputText && sameUri(tab.input.uri.toString(), close));
  void vscode.window.tabGroups.close(tabs);
}

/** What a command asks for before it runs, when its result says so. */
interface Input {
  kind: string;
  prompt: string;
  value: string;
  /** For `key`: the keys to choose from and what each stands for. */
  choices?: { key: string; label: string }[];
  /** What to send back with the answer, besides it. */
  context?: Record<string, unknown>;
}

/**
 * Ask what `input` asks for. A key is picked from its choices, the way
 * Org's fast selection offers them; anything else is typed.
 */
async function ask(input: Input, typed: (text: string) => Promise<vscode.InputBoxValidationMessage | undefined>) {
  if (input.kind === 'key' && input.choices) {
    const items = input.choices.map((choice) => ({
      label: `[${choice.key === ' ' ? 'SPC' : choice.key}] ${choice.label}`,
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
  const { languages, agendaScheme, datePromptUri } = context.extension.packageJSON.orgLsp as {
    languages: string[];
    agendaScheme: string;
    datePromptUri: string;
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
          if (!sameUri(params.uri, datePromptUri) || params.external) {
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
  // This client shows the server's date prompt.
  client.registerFeature({
    fillClientCapabilities(capabilities) {
      capabilities.experimental = { ...(capabilities.experimental ?? {}), orgDatePrompt: true };
    },
    initialize() {},
    getState() {
      return { kind: 'static' };
    },
    clear() {},
  });
  try {
    await client.start();
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

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand('org-lsp.restartServer', async () => {
      await stopClient();
      await startClient(context);
    }),
    // The server reads its settings once, at start.
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration('org-lsp')) {
        await stopClient();
        await startClient(context);
      }
    }),
  );
  await startClient(context);
}

export function deactivate(): Promise<void> {
  return stopClient();
}
