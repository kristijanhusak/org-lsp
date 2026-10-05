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

/** What a command asks for before it runs, when its result says so. */
interface Input {
  kind: string;
  prompt: string;
  value: string;
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
  const { languages, agendaScheme } = context.extension.packageJSON.orgLsp as {
    languages: string[];
    agendaScheme: string;
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
          const result = await running.sendRequest('workspace/executeCommand', params);
          const input = asked(result);
          if (!input) {
            return result;
          }
          // The command asks for something first: ask, and run it again
          // with the answer.
          const context = (params.arguments[0] ?? {}) as Record<string, unknown>;
          const answer = await vscode.window.showInputBox({
            prompt: input.prompt,
            value: input.value,
            validateInput: input.kind === 'date' ? (text) => preview(running, name, context, text) : undefined,
          });
          if (answer === undefined) {
            return undefined;
          }
          const again = { command: name, arguments: [{ ...context, input: answer }] };
          return await running.sendRequest('workspace/executeCommand', again);
        } catch (error) {
          void vscode.window.showWarningMessage(error instanceof Error ? error.message : String(error));
          return undefined;
        }
      },
    },
  };
  client = new LanguageClient('org-lsp', 'Org Language Server', serverOptions, clientOptions);
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
