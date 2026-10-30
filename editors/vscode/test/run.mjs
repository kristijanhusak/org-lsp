// Run the extension's tests inside VS Code, the highlighting of the shared
// sample, then the shared end-to-end scenarios (tests/e2e/scenarios) with
// the keys of each preset.
//
//   npm test                 all three
//   npm test -- extension    the extension's tests
//   npm test -- highlight    the highlighting of the shared sample (tests/e2e/highlight)
//   npm test -- screenshots  pictures of it, into ORG_LSP_TEST_SHOTS (only when named)
//   npm test -- scenarios    the scenarios (ORG_LSP_E2E=NAME: those with NAME in their name)
//   npm test -- scenarios vim    with the keys of one preset only
//
// Downloads VS Code unless VSCODE_EXECUTABLE names one. Needs a display;
// on a headless machine run it under `xvfb-run -a`. The scenarios press
// keys with `xdotool`, in the display VS Code is in.
import { cpSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { runTests } from '@vscode/test-electron';

const parts = process.argv.slice(2);
const wanted = (part) => parts.length === 0 || parts.includes(part);

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
// org-roam's notes, two linked to one another.
const roam = join(workspace, 'roam');
mkdirSync(roam);
writeFileSync(join(roam, 'a.org'), ':PROPERTIES:\n:ID:       a-id\n:END:\n#+title: A note\n\nText.\n');
writeFileSync(join(roam, 'b.org'), ':PROPERTIES:\n:ID:       b-id\n:END:\n#+title: B note\n\nSee [[id:a-id][A]].\n');
// org-journal's files, outside the workspace too: yesterday's, with a TODO.
const journal = mkdtempSync(join(tmpdir(), 'org-lsp-vscode-journal-'));
writeFileSync(join(journal, '20260517'), '* Sunday, 05/17/26\n** 09:00 standup\n** TODO carried\na note\n');
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
      org_tag_faces: { office: '#0088ff' },
      org_capture_templates: [
        { key: 't', description: 'Task', target: ['file', captured], template: '* TODO %?' },
        { key: 'w', description: 'Web', target: ['file', captured], template: ['* %:description', '%:link', '%i%?'] },
      ],
      org_roam_enabled: true,
      org_roam_directory: roam,
      org_journal_enabled: true,
      org_journal_dir: journal,
      org_fontify_whole_heading_line: true,
      // Shift and the arrows are Org's wherever they apply, as in Emacs.
      org_support_shift_select: false,
    },
  }),
);

/**
 * VS Code's settings and state, in a short path: its IPC socket is in it,
 * and a socket's path cannot be longer than 107 characters. With the user
 * settings and keybindings given.
 */
function userData(settings = {}, keybindings = []) {
  const directory = mkdtempSync(join(tmpdir(), 'org-lsp-vscode-user-'));
  mkdirSync(join(directory, 'User'));
  writeFileSync(join(directory, 'User', 'settings.json'), JSON.stringify(settings, null, 2));
  writeFileSync(join(directory, 'User', 'keybindings.json'), JSON.stringify(keybindings, null, 2));
  return directory;
}

async function run(suite, folder, env, user) {
  await runTests({
    vscodeExecutablePath: process.env.VSCODE_EXECUTABLE || undefined,
    extensionDevelopmentPath: resolve('.'),
    extensionTestsPath: resolve('test', 'suite.cjs'),
    // The tests' own options only, whatever configuration file the machine has.
    extensionTestsEnv: { ORG_LSP_TEST_SUITE: suite, ORG_LSP_CONFIG: '/dev/null', ...env },
    launchArgs: [folder, '--disable-extensions', '--disable-gpu', '--no-sandbox', '--disable-workspace-trust', '--user-data-dir', user],
  });
}

const scenarios = join('..', '..', 'tests', 'e2e', 'scenarios');

function scenarioNames() {
  return readdirSync(scenarios)
    .filter((file) => file.endsWith('.json'))
    .map((file) => file.slice(0, -'.json'.length))
    .sort();
}

/**
 * The user's own keys, as docs/keys/vscode.org says to bind them. VS Code
 * gives a key no count: a user binds a key with `"args": { "count": N }`.
 * The scenarios' steps with a count press such keys, one for each command
 * and count, where the preset binds the command; a command the preset
 * leaves without a key (tests/e2e/exceptions.json) gets a key of the
 * user's too, so that every step is typed.
 */
function userKeys(preset) {
  const bindings = JSON.parse(readFileSync('package.json', 'utf8')).contributes.keybindings;
  const exceptions = JSON.parse(readFileSync(join('..', '..', 'tests', 'e2e', 'exceptions.json'), 'utf8'));
  const excepted = exceptions.editors?.[preset === 'emacs_full' ? 'vscode' : `vscode-${preset}`] ?? {};
  const wanted = new Set();
  for (const name of scenarioNames()) {
    for (const step of JSON.parse(readFileSync(join(scenarios, `${name}.json`), 'utf8')).steps) {
      if (step.command && (step.count || excepted[step.command])) {
        wanted.add(JSON.stringify([step.command, step.count ?? null]));
      }
    }
  }
  const keys = [];
  [...wanted].sort().forEach((entry, index) => {
    const [command, count] = JSON.parse(entry);
    // Ctrl+Alt+Shift+F9 (F10, F11), then a letter: chords no preset binds.
    const key = `ctrl+alt+shift+f${9 + Math.floor(index / 26)} ${String.fromCharCode(97 + (index % 26))}`;
    const whens = new Set(
      bindings
        .filter((binding) => binding.command === command && binding.when?.includes(`config.org-lsp.keys == '${preset}'`))
        .map((binding) => binding.when),
    );
    // Where the preset has no key for it, wherever the user types.
    if (excepted[command]) {
      whens.add('editorTextFocus');
    }
    for (const when of whens) {
      keys.push({ key, command, ...(count ? { args: { count } } : {}), when });
    }
  });
  return keys;
}

try {
  if (wanted('extension')) {
    // A state directory of its own, where the server says it listens for
    // org-protocol URLs.
    const state = mkdtempSync(join(tmpdir(), 'org-lsp-vscode-state-'));
    const env = {
      ORG_LSP_TEST_WORKSPACE: workspace,
      ORG_LSP_TEST_CAPTURED: captured,
      ORG_LSP_TEST_JOURNAL: journal,
      ORG_LSP_TEST_SERVER: server,
      XDG_STATE_HOME: state,
    };
    await run('extension', workspace, env, userData());
  }
  if (wanted('highlight')) {
    await runHighlight('highlight');
  }
  // Only when asked for: pictures, no checks.
  if (parts.includes('screenshots')) {
    await runHighlight('screenshots', {
      'window.newWindowDimensions': 'maximized',
      'workbench.startupEditor': 'none',
      'editor.minimap.enabled': false,
      'workbench.tips.enabled': false,
    });
  }
  if (wanted('scenarios')) {
    await runScenarios();
  }
} catch (error) {
  console.error(error);
  process.exit(1);
}

/**
 * The highlighting of the shared sample (tests/e2e/highlight), copied to a
 * folder of its own, with its options (`$DIR` that folder): its checks
 * (`highlight`), or its pictures (`screenshots`, into
 * `ORG_LSP_TEST_SHOTS`), with VS Code's `settings`.
 */
async function runHighlight(suite, settings = {}) {
  const folder = mkdtempSync(join(tmpdir(), 'org-lsp-vscode-highlight-'));
  cpSync(join('..', '..', 'tests', 'e2e', 'highlight'), folder, { recursive: true });
  const options = readFileSync(join(folder, 'options.json'), 'utf8').replaceAll('$DIR', folder);
  mkdirSync(join(folder, '.vscode'));
  writeFileSync(
    join(folder, '.vscode', 'settings.json'),
    JSON.stringify({ 'org-lsp.server.path': server, 'org-lsp.settings': JSON.parse(options) }),
  );
  const env = { ORG_LSP_TEST_HIGHLIGHT: folder, ORG_LSP_TEST_SHOTS: process.env.ORG_LSP_TEST_SHOTS ?? folder };
  await run(suite, folder, env, userData(settings));
}

/**
 * Each scenario in a VS Code of its own, as Neovim's adapter runs each in
 * a Neovim of its own: VS Code keeps documents a long while after their
 * editors closed, and gives them to each server it starts.
 */
async function runScenarios() {
  // Emacs's keys all: the scenarios are typed as in Emacs.
  const presets = ['emacs_full', 'vim'].filter((preset) => parts.includes(preset));
  const failed = [];
  const started = Date.now();
  let ran = 0;
  for (const preset of presets.length > 0 ? presets : ['emacs_full', 'vim']) {
    const settings = {
      'org-lsp.server.path': server,
      'org-lsp.keys': preset,
      'workbench.startupEditor': 'none',
      'window.restoreWindows': 'none',
    };
    const keys = userKeys(preset);
    for (const name of scenarioNames().filter((name) => !process.env.ORG_LSP_E2E || name.includes(process.env.ORG_LSP_E2E))) {
      const folder = mkdtempSync(join(tmpdir(), 'org-lsp-vscode-e2e-'));
      const user = userData(settings, keys);
      ran += 1;
      try {
        await run('scenarios', folder, { ORG_LSP_TEST_PRESET: preset, ORG_LSP_TEST_SCENARIO: name, ORG_LSP_TEST_USER_DATA: user }, user);
      } catch {
        failed.push(`${name} (${preset})`);
      } finally {
        // A folder and a VS Code state for each run: gone with it.
        for (const directory of [folder, user]) {
          rmSync(directory, { recursive: true, force: true, maxRetries: 5 });
        }
      }
    }
  }
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  console.log(`${ran} scenario runs, ${failed.length} failed, ${seconds} s${failed.length > 0 ? `: ${failed.join(', ')}` : ''}`);
  if (failed.length > 0) {
    process.exit(1);
  }
}
