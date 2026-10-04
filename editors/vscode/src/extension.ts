// VS Code glue for the org-lsp language server.
//
// The server does the Org work. This extension only starts it, declares the
// `org` language and its semantic tokens (in package.json), and exposes the
// server's code actions as commands that can be bound to keys.

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

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const command = serverCommand(context);
  const serverOptions: ServerOptions = { command, args: [] };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: 'org' }],
    initializationOptions: {
      todoKeywords: vscode.workspace.getConfiguration('org-lsp').get<string[]>('todoKeywords'),
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

/**
 * Run the server's code action of exactly this kind at the cursor. Every Org
 * command goes through this: the server computes the edit, VS Code applies it.
 */
function codeAction(kind: string): () => Thenable<unknown> {
  return () => vscode.commands.executeCommand('editor.action.codeAction', { kind, apply: 'first' });
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand('org-lsp.todoNext', codeAction('org.todo.next')),
    vscode.commands.registerCommand('org-lsp.todoPrevious', codeAction('org.todo.previous')),
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
