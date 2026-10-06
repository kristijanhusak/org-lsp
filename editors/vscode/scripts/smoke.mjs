// Talk to the server with the same protocol library VS Code's client uses.
// This does not run VS Code; it checks that the server and the JavaScript
// LSP implementation understand each other.
//
//   npm run smoke [-- path/to/org-lsp]
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const rpc = require('vscode-jsonrpc/node');

const exe = process.platform === 'win32' ? 'org-lsp.exe' : 'org-lsp';
const binary = resolve(process.argv[2] ?? join('..', '..', 'target', 'release', exe));
const server = spawn(binary, ['--stdio'], { stdio: ['pipe', 'pipe', 'inherit'] });
const connection = rpc.createMessageConnection(
  new rpc.StreamMessageReader(server.stdout),
  new rpc.StreamMessageWriter(server.stdin),
);
connection.listen();

const uri = 'file:///tmp/smoke.org';
const text = '#+TITLE: Smoke\n* TODO Ünï 😀 *bold*\nbody\n';

const init = await connection.sendRequest('initialize', {
  processId: process.pid,
  rootUri: null,
  // VS Code offers UTF-16 only and supports versioned document edits.
  capabilities: {
    general: { positionEncodings: ['utf-16'] },
    workspace: { workspaceEdit: { documentChanges: true } },
    textDocument: { documentSymbol: { hierarchicalDocumentSymbolSupport: true } },
  },
  initializationOptions: { todoKeywords: ['TODO', '|', 'DONE'] },
});
assert.equal(init.capabilities.positionEncoding, 'utf-16');
const legend = init.capabilities.semanticTokensProvider.legend;
await connection.sendNotification('initialized', {});
await connection.sendNotification('textDocument/didOpen', {
  textDocument: { uri, languageId: 'org', version: 1, text },
});

const tokens = await connection.sendRequest('textDocument/semanticTokens/full', { textDocument: { uri } });
const decoded = [];
let line = 0;
let column = 0;
for (let i = 0; i < tokens.data.length; i += 5) {
  const [deltaLine, deltaStart, length, type] = tokens.data.slice(i, i + 5);
  line += deltaLine;
  column = deltaLine === 0 ? column + deltaStart : deltaStart;
  // JavaScript strings are UTF-16, the same units VS Code counts in.
  decoded.push([text.split('\n')[line].slice(column, column + length), legend.tokenTypes[type]]);
}
assert.deepEqual(decoded, [
  ['#+TITLE:', 'keyword'],
  ['*', 'stars'],
  ['TODO', 'todoKeyword'],
  ['Ünï 😀 ', 'headline'],
  ['*bold*', 'headline'],
]);

const actions = await connection.sendRequest('textDocument/codeAction', {
  textDocument: { uri },
  range: { start: { line: 2, character: 1 }, end: { line: 2, character: 1 } },
  context: { diagnostics: [], only: ['org.todo.next'] },
});
assert.equal(actions.length, 1);
assert.equal(actions[0].kind, 'org.todo.next');
const change = actions[0].edit.documentChanges[0];
assert.equal(change.textDocument.version, 1);
assert.deepEqual(change.edits, [
  { range: { start: { line: 1, character: 2 }, end: { line: 1, character: 6 } }, newText: 'DONE' },
]);

const folds = await connection.sendRequest('textDocument/foldingRange', { textDocument: { uri } });
assert.deepEqual(folds, [{ startLine: 1, endLine: 2 }]);

const symbols = await connection.sendRequest('textDocument/documentSymbol', { textDocument: { uri } });
assert.equal(symbols[0].name, 'Ünï 😀 *bold*');
// The selection range is in UTF-16 units: the emoji counts as two.
assert.deepEqual(symbols[0].selectionRange, {
  start: { line: 1, character: 7 },
  end: { line: 1, character: 20 },
});

await connection.sendRequest('shutdown');
await connection.sendNotification('exit');
const code = await new Promise((done) => server.on('exit', done));
assert.equal(code, 0);
console.log('ok: server and vscode-jsonrpc agree');
