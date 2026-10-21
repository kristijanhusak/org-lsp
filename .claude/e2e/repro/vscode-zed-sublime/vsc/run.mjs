// Exploratory runner: runs the cases of a module (CASES) in VS Code with
// the org-lsp extension of editors/vscode, records what each step shows
// and writes it to OUT (JSON).
//   node run.mjs CASES.cjs OUT.json [settings.json]
// Settings: { "folder": {...workspace settings}, "user": {...user settings} }
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync, copyFileSync, cpSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const here = dirname(fileURLToPath(import.meta.url));
const ext = resolve(here, '../../../../../editors/vscode');
const require = createRequire(join(ext, 'package.json'));
const { runTests } = require('@vscode/test-electron');
const [cases, out, settingsFile] = process.argv.slice(2);
const scratch = process.env.SCRATCH ?? '/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vscode-zed-sublime';
mkdirSync(scratch, { recursive: true });
const ws = mkdtempSync(join(scratch, 'ws-'));
const user = mkdtempSync(join('/tmp', 'ovx-'));
const server = resolve(ext, '../../target/debug/org-lsp');
const s = settingsFile ? JSON.parse(readFileSync(settingsFile, 'utf8').replaceAll('$WS', ws)) : {};
mkdirSync(join(ws, '.vscode'));
writeFileSync(join(ws, '.vscode', 'settings.json'), JSON.stringify({ 'org-lsp.server.path': server, ...(s.folder ?? {}) }, null, 2));
mkdirSync(join(user, 'User'));
writeFileSync(join(user, 'User', 'settings.json'), JSON.stringify({ 'workbench.startupEditor': 'none', 'window.restoreWindows': 'none', ...(s.user ?? {}) }, null, 2));
for (const [name, text] of Object.entries(s.files ?? {})) {
  mkdirSync(dirname(join(ws, name)), { recursive: true });
  writeFileSync(join(ws, name), text);
}
// The extension's test module must be inside the extension, or it gets an
// API object of its own (its stubs would not reach the extension): a copy
// of the extension in the scratch directory, with the explorer in it.
const copy = join(scratch, 'ext');
cpSync(join(ext, 'dist'), join(copy, 'dist'), { recursive: true });
cpSync(join(ext, 'package.json'), join(copy, 'package.json'));
mkdirSync(join(copy, 'explore'), { recursive: true });
copyFileSync(join(here, 'explore.cjs'), join(copy, 'explore', 'explore.cjs'));
console.log('workspace', ws, 'user', user);
try {
  await runTests({
    vscodeExecutablePath: process.env.VSCODE_EXECUTABLE || '/root/vscodium/codium',
    extensionDevelopmentPath: copy,
    extensionTestsPath: join(copy, 'explore', 'explore.cjs'),
    extensionTestsEnv: { CASES: resolve(cases), OUT: resolve(out), WS: ws, ORG_LSP_CONFIG: '/nonexistent/org-lsp/config.toml', ORG_LISP_DIR: process.env.ORG_LISP_DIR ?? '/root/org-mode/lisp', ONLY: process.env.ONLY ?? '' },
    launchArgs: [ws, '--disable-extensions', '--disable-gpu', '--no-sandbox', '--disable-workspace-trust', '--user-data-dir', user],
  });
} catch (e) { console.error('runTests failed', e); process.exitCode = 1; }
