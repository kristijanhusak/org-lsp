// Run the extension's tests inside VS Code.
//
//   npm test
//
// Downloads VS Code unless VSCODE_EXECUTABLE names one. Needs a display;
// on a headless machine run it under `xvfb-run -a`.
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { runTests } from '@vscode/test-electron';

const exe = process.platform === 'win32' ? 'org-lsp.exe' : 'org-lsp';
const server = resolve(process.env.ORG_LSP_BIN ?? join('..', '..', 'target', 'debug', exe));

// A workspace with two agenda files and a fixed clock: Monday 18 May 2026, 10:30.
const workspace = mkdtempSync(join(tmpdir(), 'org-lsp-vscode-'));
writeFileSync(
  join(workspace, 'work.org'),
  [
    '#+TITLE: Work',
    '* TODO [#A] Write report :office:',
    'SCHEDULED: <2026-05-18 Mon 09:00>',
    'Some *bold* text.',
    '** Outline of the report',
    '* TODO Call the client',
    'DEADLINE: <2026-05-20 Wed>',
    '* Meeting <2026-05-19 Tue 14:00-15:00>',
    '',
  ].join('\n'),
);
writeFileSync(
  join(workspace, 'home.org'),
  ['* TODO Water plants', 'SCHEDULED: <2026-05-16 Sat +1w>', '* DONE Buy milk', ''].join('\n'),
);
// Captures go to a file outside the workspace, so that the agenda does not see them.
const captured = join(mkdtempSync(join(tmpdir(), 'org-lsp-vscode-capture-')), 'inbox.org');
mkdirSync(join(workspace, '.vscode'));
writeFileSync(
  join(workspace, '.vscode', 'settings.json'),
  JSON.stringify({
    'org-lsp.server.path': server,
    'org-lsp.settings': {
      clock: { today: '2026-05-18', now: '10:30' },
      org_todo_keyword_faces: { TODO: ':foreground #ff8800 :weight bold' },
      org_priority_faces: { A: 'red' },
      org_capture_templates: [{ key: 't', description: 'Task', target: ['file', captured], template: '* TODO %?' }],
    },
  }),
);

try {
  await runTests({
    vscodeExecutablePath: process.env.VSCODE_EXECUTABLE || undefined,
    extensionDevelopmentPath: resolve('.'),
    extensionTestsPath: resolve('test', 'suite.cjs'),
    // The tests' own options only, whatever configuration file the machine has.
    extensionTestsEnv: {
      ORG_LSP_TEST_WORKSPACE: workspace,
      ORG_LSP_TEST_CAPTURED: captured,
      ORG_LSP_CONFIG: '/nonexistent/org-lsp/config.toml',
    },
    launchArgs: [workspace, '--disable-extensions', '--disable-gpu', '--no-sandbox', '--disable-workspace-trust'],
  });
} catch (error) {
  console.error(error);
  process.exit(1);
}
