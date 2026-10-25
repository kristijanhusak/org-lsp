// Exploratory runner for TESTING.org by hand, typed: runs the cases of a
// module in VS Code with the org-lsp extension of editors/vscode, every
// key typed with xdotool (typed.cjs), and writes what each step showed to
// OUT (JSON), with screenshots next to it.
//   DISPLAY=:N node run.mjs CASES.cjs OUT.json [settings.json]
// Settings: { "folder": {...workspace settings}, "user": {...user settings},
//             "keys": [...user keybindings], "files": {name: text} }
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync, copyFileSync, cpSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ext = resolve(here, '../../../../editors/vscode');
const require = createRequire(join(ext, 'package.json'));
const { runTests } = require('@vscode/test-electron');
const [cases, out, settingsFile] = process.argv.slice(2);
const scratch = process.env.SCRATCH ?? join(process.env.TMPDIR ?? '/tmp', 'typed-vscode');
mkdirSync(scratch, { recursive: true });
const ws = mkdtempSync(join(scratch, 'ws-'));
const user = mkdtempSync(join('/tmp', 'otx-'));
const server = process.env.ORG_LSP_BIN ?? resolve(ext, '../../target/debug/org-lsp');
const s = settingsFile ? JSON.parse(readFileSync(settingsFile, 'utf8').replaceAll('$WS', ws)) : {};
mkdirSync(join(ws, '.vscode'));
writeFileSync(join(ws, '.vscode', 'settings.json'), JSON.stringify({ 'org-lsp.server.path': server, ...(s.folder ?? {}) }, null, 2));
mkdirSync(join(user, 'User'));
writeFileSync(
  join(user, 'User', 'settings.json'),
  JSON.stringify({ 'workbench.startupEditor': 'none', 'window.restoreWindows': 'none', 'org-lsp.server.path': server, ...(s.user ?? {}) }, null, 2),
);
writeFileSync(join(user, 'User', 'keybindings.json'), JSON.stringify(s.keys ?? [], null, 2));
for (const [name, text] of Object.entries(s.files ?? {})) {
  mkdirSync(dirname(join(ws, name)), { recursive: true });
  writeFileSync(join(ws, name), text);
}
// The test module must be inside the extension, or it gets a `vscode`
// object of its own: a copy of the extension, with the explorer in it.
const copy = join(scratch, 'ext');
cpSync(join(ext, 'dist'), join(copy, 'dist'), { recursive: true });
cpSync(join(ext, 'package.json'), join(copy, 'package.json'));
cpSync(join(ext, 'syntaxes'), join(copy, 'syntaxes'), { recursive: true });
cpSync(join(ext, 'language-configuration.json'), join(copy, 'language-configuration.json'));
mkdirSync(join(copy, 'explore'), { recursive: true });
copyFileSync(join(here, 'typed.cjs'), join(copy, 'explore', 'typed.cjs'));
console.log('workspace', ws, 'user', user);
try {
  await runTests({
    vscodeExecutablePath: process.env.VSCODE_EXECUTABLE || '/root/vscodium/codium',
    extensionDevelopmentPath: copy,
    extensionTestsPath: join(copy, 'explore', 'typed.cjs'),
    extensionTestsEnv: {
      CASES: resolve(cases),
      OUT: resolve(out),
      WS: ws,
      USER_DATA: user,
      ORG_LSP_CONFIG: process.env.ORG_LSP_CONFIG ?? '/dev/null',
      ORG_LISP_DIR: process.env.ORG_LISP_DIR ?? '/root/org-mode/lisp',
      ONLY: process.env.ONLY ?? '',
      ICONS: process.env.ICONS ?? '',
    },
    launchArgs: [ws, '--disable-extensions', '--disable-gpu', '--no-sandbox', '--disable-workspace-trust', '--user-data-dir', user],
  });
} catch (e) {
  console.error('runTests failed', e);
  process.exitCode = 1;
}
