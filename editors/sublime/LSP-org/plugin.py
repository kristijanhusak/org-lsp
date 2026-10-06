"""Sublime Text glue for the org-lsp language server, on top of sublimelsp/LSP.

The server does the Org work and describes itself (its commands and their
keys, how its tokens look); Default.sublime-keymap and the settings are
generated from that (generate.mjs). This file runs the server's commands
with the cursor position, asks what they ask for, and cycles folds.
"""
from __future__ import annotations

import io
import os
import shutil
import tarfile
import threading
import urllib.request
import zipfile

import sublime
import sublime_plugin
from LSP.plugin import AbstractPlugin
from LSP.plugin import apply_text_edits
from LSP.plugin import position_to_offset
from LSP.plugin import LspTextCommand
from LSP.plugin import LspWindowCommand
from LSP.plugin import Request
from LSP.plugin import parse_uri
from LSP.plugin import register_plugin
from LSP.plugin import text_document_identifier
from LSP.plugin import text_document_position_params
from LSP.plugin import unregister_plugin

# The folder the package is in: `LSP-org`, as it is installed.
PACKAGE = __package__ or 'LSP-org'
SESSION = 'LSP-org'
SETTINGS = 'LSP-org.sublime-settings'
CLOCK_KEY = 'org-lsp-clock'
# How long to wait for the server to start, in steps of 200 ms.
STARTING_TRIES = 150
REPOSITORY = 'kristijanhusak/org-lsp'
RELEASE = 'nightly'
# The release's name for each platform Sublime Text runs on.
PLATFORMS = {
    ('linux', 'x64'): 'linux-x86_64',
    ('linux', 'arm64'): 'linux-arm64',
    ('osx', 'arm64'): 'macos-arm64',
    ('osx', 'x64'): 'macos-x86_64',
    ('windows', 'x64'): 'windows-x86_64',
}


def settings() -> sublime.Settings:
    return sublime.load_settings(SETTINGS)


# ----- the server binary

def executable() -> str:
    return 'org-lsp.exe' if sublime.platform() == 'windows' else 'org-lsp'


def search_path() -> str:
    """PATH, and where cargo, pipx-style installs and Homebrew put programs:
    started from a desktop launcher, Sublime Text often has none of them."""
    home = os.path.expanduser('~')
    extra = [os.path.join(home, '.cargo', 'bin'), os.path.join(home, '.local', 'bin'), '/usr/local/bin',
             '/opt/homebrew/bin']
    return os.pathsep.join([os.environ.get('PATH', '')] + extra)


def checkout_build():
    """The server built in the checkout this package is linked from
    (`editors/sublime/LSP-org`), if there is one."""
    package = os.path.realpath(os.path.join(sublime.packages_path(), PACKAGE))
    root = os.path.dirname(os.path.dirname(os.path.dirname(package)))
    for profile in ('release', 'debug'):
        path = os.path.join(root, 'target', profile, executable())
        if os.path.isfile(path):
            return path
    return None


def downloaded_path(storage: str) -> str:
    return os.path.join(storage, PACKAGE, executable())


def server_path(storage: str):
    """The server to start: the configured `command`, `org-lsp` on the search
    path, a build in the checkout, or the one downloaded before."""
    command = settings().get('command') or ['org-lsp']
    if command[0] != 'org-lsp':
        return os.path.expanduser(command[0])
    found = shutil.which('org-lsp', path=search_path()) or checkout_build()
    if found:
        return found
    downloaded = downloaded_path(storage)
    return downloaded if os.path.isfile(downloaded) else None


def download(storage: str) -> str:
    """Fetch the nightly build of the server for this platform into the
    package's storage, and return its path."""
    platform = PLATFORMS.get((sublime.platform(), sublime.arch()))
    if not platform:
        raise RuntimeError('org-lsp has no build for this platform; put org-lsp on PATH or set "command"')
    windows = sublime.platform() == 'windows'
    archive = 'org-lsp-{}.{}'.format(platform, 'zip' if windows else 'tar.gz')
    url = 'https://github.com/{}/releases/download/{}/{}'.format(REPOSITORY, RELEASE, archive)
    with urllib.request.urlopen(url, timeout=120) as response:
        data = response.read()
    if windows:
        with zipfile.ZipFile(io.BytesIO(data)) as files:
            binary = files.read(executable())
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as files:
            member = files.extractfile(executable())
            if member is None:
                raise RuntimeError('no {} in {}'.format(executable(), url))
            binary = member.read()
    target = downloaded_path(storage)
    os.makedirs(os.path.dirname(target), exist_ok=True)
    partial = target + '.part'
    with open(partial, 'wb') as out:
        out.write(binary)
    os.chmod(partial, 0o755)
    os.replace(partial, target)
    return target


class OrgLsp(AbstractPlugin):
    @classmethod
    def name(cls) -> str:
        return SESSION

    @classmethod
    def configuration(cls):
        # Named here, whatever LSP makes of the name: newer versions put
        # `LSP-` before it (`LSP-LSP-org`), older ones do not.
        return settings(), 'Packages/{}/{}'.format(PACKAGE, SETTINGS)

    @classmethod
    def needs_update_or_installation(cls) -> bool:
        return server_path(cls.storage_path()) is None

    @classmethod
    def install_or_update(cls) -> None:
        download(cls.storage_path())

    @classmethod
    def can_start(cls, window, initiating_view, workspace_folders, configuration):
        if server_path(cls.storage_path()) is None:
            return 'org-lsp: no server found. Put org-lsp on PATH, set "command" in the LSP-org settings, ' \
                   'or run "Org: Download the latest server".'
        return None

    @classmethod
    def on_pre_start(cls, window, initiating_view, workspace_folders, configuration):
        found = server_path(cls.storage_path())
        if found and configuration.command and configuration.command[0] == 'org-lsp':
            configuration.command[0] = found
        # This package shows the documents the server asks it to.
        options = configuration.init_options
        options.set('showDocument', True)
        # `org-indent-mode`'s prefixes, as phantoms (`org-lsp/virtualIndent`).
        options.set('virtualIndent', True)
        return None

    # The server changed files from elsewhere (the agenda, a capture, a
    # refile), and `autoSave` is on: save them.
    def m_org_lsp_save(self, params) -> None:
        uris = (params or {}).get('uris', [])
        sublime.set_timeout(lambda: save(uris))

    # The clock that runs, for the status bar.
    def m_org_lsp_clock(self, params) -> None:
        sublime.set_timeout(lambda: show_clock(params))

    # How the lines of a file with `org-indent-mode` show.
    def m_org_lsp_virtualIndent(self, params) -> None:
        sublime.set_timeout(lambda: show_virtual_indent(params))


class OrgLspUpdateServerCommand(sublime_plugin.WindowCommand):
    """Download the nightly build of the server again, then restart it."""

    def run(self) -> None:
        sublime.status_message('org-lsp: downloading the server…')

        def work() -> None:
            try:
                path = download(OrgLsp.storage_path())
            except Exception as error:
                sublime.set_timeout(lambda: sublime.error_message('org-lsp: could not download the server: {}'.format(error)))
                return
            sublime.set_timeout(lambda: self.done(path))
        threading.Thread(target=work, daemon=True).start()

    def done(self, path: str) -> None:
        sublime.status_message('org-lsp: the server is at {}'.format(path))
        if server_path(OrgLsp.storage_path()) != path:
            sublime.message_dialog('org-lsp: downloaded to {}, but another server is used: '
                                   '{}'.format(path, server_path(OrgLsp.storage_path())))
            return
        view = self.window.active_view()
        if view:
            view.run_command('lsp_restart_server', {'config_name': SESSION})


def plugin_loaded() -> None:
    register_plugin(OrgLsp)


def plugin_unloaded() -> None:
    unregister_plugin(OrgLsp)


def view_for(uri: str):
    _, path = parse_uri(uri)
    for window in sublime.windows():
        view = window.find_open_file(path)
        if view:
            return view
    return None


def save(uris) -> None:
    for uri in uris:
        view = view_for(uri)
        if view and view.is_dirty():
            view.run_command('save')


def close(uri: str) -> None:
    view = view_for(uri)
    if not view:
        return
    if view.is_dirty() and view.file_name():
        view.run_command('save')
    view.set_scratch(True)
    view.close()


# The phantoms of `org-indent-mode` by view; a set of phantoms shows only
# while it is kept.
indent_phantoms = {}


def show_virtual_indent(params) -> None:
    """Show the prefixes the server sent for a file (`org-lsp/virtualIndent`):
    `lines` of `[line, prefix, wrap, hidden stars]`. Each prefix is a phantom
    of blanks before its line. Sublime Text cannot repeat it on the lines a
    long line wraps into, nor hide the leading stars."""
    view = view_for((params or {}).get('uri', ''))
    if not view:
        return
    rows = view.rowcol(view.size())[0] + 1
    phantoms = []
    for line, prefix, _wrap, _stars in (params or {}).get('lines', []):
        # For older text than the view has: newer prefixes are on their way.
        if prefix > 0 and line < rows:
            point = view.text_point(line, 0)
            html = '<body style="margin: 0; padding: 0">{}</body>'.format('&nbsp;' * prefix)
            phantoms.append(sublime.Phantom(sublime.Region(point), html, sublime.LAYOUT_INLINE))
    phantom_set = indent_phantoms.get(view.id())
    if phantom_set is None:
        phantom_set = indent_phantoms[view.id()] = sublime.PhantomSet(view, 'org-lsp-indent')
    phantom_set.update(phantoms)


def show_clock(params) -> None:
    text = ''
    if params and params.get('heading'):
        minutes = params.get('minutes') or 0
        text = '[{}:{:02d}] ({})'.format(minutes // 60, minutes % 60, params['heading'])
    for window in sublime.windows():
        for view in window.views():
            if text:
                view.set_status(CLOCK_KEY, text)
            else:
                view.erase_status(CLOCK_KEY)


def starter_path() -> str:
    """The file that starts the server when no Org file is open: an agenda
    file, which LSP starts the server for."""
    return os.path.join(sublime.cache_path(), PACKAGE, 'Starting.org-agenda')


def is_org(view) -> bool:
    return bool(view) and view.match_selector(0, 'text.org | text.org-agenda')


class OrgLspRunCommand(LspWindowCommand):
    """Run a server command where the cursor is. `fallback` is the command
    Sublime runs instead where the server says the command does not apply
    (a key that keeps its own meaning)."""

    session_name = SESSION

    def is_enabled(self, **kwargs) -> bool:
        return True

    # The view that started the server for a command run with no Org
    # file open; it closes once the command is done.
    starter = None

    def run(self, command: str, fallback=None) -> None:
        session = self.session_by_name(SESSION)
        view = self.window.active_view()
        if not session:
            # A key keeps its own meaning without a server (Enter types).
            if fallback and view:
                view.run_command(fallback['command'], fallback.get('args'))
            elif is_org(view):
                sublime.status_message('org-lsp: the server is starting')
            else:
                # The agenda, a capture: no Org file needs to be open.
                self.start(lambda session: self.execute(session, view, command, {}, None))
            return
        if is_org(view):
            context = text_document_position_params(view, view.sel()[0].b)
        else:
            context = {}
        self.execute(session, view, command, context, fallback)

    def start(self, then) -> None:
        """Start the server, as LSP starts it: for a view it applies to.
        The view is a file of the package's that says the server is
        starting, and closes when the command is done; the agenda or the
        capture buffer the command opens keeps the server running."""
        path = starter_path()
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'w', encoding='utf-8') as out:
            out.write('org-lsp is starting…\n')
        self.close_starter()
        OrgLspRunCommand.starter = self.window.open_file(path)
        sublime.status_message('org-lsp: starting the server…')

        def wait(tries: int) -> None:
            session = self.session_by_name(SESSION)
            # READY: the server has answered `initialize`.
            if session and getattr(session, 'state', 1) == 1:
                then(session)
            elif tries > 0:
                sublime.set_timeout(lambda: wait(tries - 1), 200)
            else:
                self.close_starter()
                sublime.status_message('org-lsp: the server did not start; see the LSP log panel')
        wait(STARTING_TRIES)

    def close_starter(self) -> None:
        starter = OrgLspRunCommand.starter
        OrgLspRunCommand.starter = None
        if starter and starter.is_valid():
            starter.set_scratch(True)
            starter.close()

    def execute(self, session, view, command, context, fallback) -> None:
        params = {'command': command, 'arguments': [context] if context else []}
        session.send_request(
            Request('workspace/executeCommand', params),
            lambda result: sublime.set_timeout(lambda: self.done(session, view, command, context, fallback, result)),
            lambda error: sublime.set_timeout(lambda: self.failed(view, fallback, error)),
        )

    def failed(self, view, fallback, error) -> None:
        data = error.get('data') if isinstance(error, dict) else None
        # A key keeps its own meaning where the command does not apply,
        # and when it fails: Enter is never lost.
        self.finish_start()
        if fallback:
            view.run_command(fallback['command'], fallback.get('args'))
        if not (isinstance(data, dict) and data.get('notApplicable')):
            sublime.status_message('org-lsp: {}'.format(error.get('message', error) if isinstance(error, dict) else error))

    def done(self, session, view, command, context, fallback, result) -> None:
        result = result if isinstance(result, dict) else {}
        # The edit and the cursor, carried in the result (`org.return`).
        if isinstance(result.get('edits'), list):
            cursor = result.get('cursor')

            def place(_) -> None:
                if cursor:
                    point = position_to_offset(cursor, view)
                    view.sel().clear()
                    view.sel().add(sublime.Region(point))
            apply_text_edits(view, result['edits']).then(lambda done: sublime.set_timeout(lambda: place(done)))
        if isinstance(result.get('save'), list):
            save(result['save'])
        if isinstance(result.get('message'), str):
            sublime.status_message(result['message'])
        if isinstance(result.get('close'), str):
            close(result['close'])
        request = result.get('input')
        if isinstance(request, dict):
            ask(self.window, request,
                lambda answer: self.answered(session, view, command, context, fallback, request, answer),
                self.finish_start)
        else:
            self.finish_start()

    def finish_start(self) -> None:
        """Close the view that started the server, once what the command
        opened (the agenda) has had the time to open and keep the server."""
        if OrgLspRunCommand.starter:
            sublime.set_timeout(self.close_starter, 1000)

    def answered(self, session, view, command, context, fallback, request, answer) -> None:
        context = dict(context)
        context.update(request.get('context') or {})
        context['input'] = answer
        self.execute(session, view, command, context, fallback)


def ask(window, request, then, cancelled=None) -> None:
    """Ask what a command asks for: a key or one of a list in the quick
    panel, or text in the input panel. `cancelled` runs when it is given
    up."""
    choices = request.get('choices')
    if choices:
        if request.get('kind') == 'key':
            items = ['[{}] {}'.format('SPC' if c['key'] == ' ' else c['key'], c['label']) for c in choices]
        else:
            items = [c['label'] for c in choices]

        def picked(index: int) -> None:
            if index >= 0:
                then(choices[index]['key'])
            elif cancelled:
                cancelled()
        window.show_quick_panel(items, picked, placeholder=request.get('prompt', ''))
        return
    window.show_input_panel(request.get('prompt', '') + ':', request.get('value', ''), then, None, cancelled)


# ----- help

def key_label(keys) -> str:
    return ' '.join('+'.join(part.capitalize() if len(part) > 1 else part.upper() if '+' in key else part
                             for part in key.split('+')) for key in keys)


def help_items(view):
    """The keys the package binds where `view` is, with the preset in
    `org_keys`: `[(keys, title, command, args)]`."""
    preset = settings().get('org_keys', 'emacs')
    keymap = sublime.decode_value(sublime.load_resource('Packages/{}/Default.sublime-keymap'.format(PACKAGE)))
    palette = sublime.decode_value(sublime.load_resource('Packages/{}/{}.sublime-commands'.format(PACKAGE, SESSION)))
    titles = {}
    for entry in palette:
        name = (entry.get('args') or {}).get('command') or entry['command']
        titles[name] = entry['caption'][len('Org: '):]
    capture = OrgLspListener().on_query_context(view, 'org_lsp.capture', sublime.OP_EQUAL, True, False)
    items = []
    for binding in keymap:
        context = {entry['key']: entry['operand'] for entry in binding.get('context', [])}
        if context.get('org_lsp.keys') != preset:
            continue
        if not view.match_selector(0, context.get('selector', '')):
            continue
        if context.get('org_lsp.capture') and not capture:
            continue
        name = (binding.get('args') or {}).get('command') or binding['command']
        items.append((key_label(binding['keys']), titles.get(name, name), binding['command'], binding.get('args')))
    return items


class OrgLspHelpCommand(sublime_plugin.WindowCommand):
    """The keys of the view at hand, in the quick panel; picking one runs it."""

    def run(self) -> None:
        view = self.window.active_view()
        items = help_items(view) if view else []
        if not items:
            sublime.status_message('org-lsp: no keys are bound here')
            return

        def picked(index: int) -> None:
            if index >= 0:
                _, _, command, args = items[index]
                target = self.window if command == 'org_lsp_run' else view
                target.run_command(command, args)
        self.window.show_quick_panel([[title, keys] for keys, title, _, _ in items], picked, placeholder='Org keys')


# ----- visibility cycling

# The last cycle: (view id, change count, line, state); the step after
# CHILDREN depends on the last command having been the same cycle.
last_cycle = None


def fold_region(view, folding_range) -> sublime.Region:
    start = view.line(view.text_point(folding_range['startLine'], 0)).b
    end = view.line(view.text_point(folding_range['endLine'], 0)).b
    return sublime.Region(start, end)


def remember(view, line: int, state: str) -> None:
    global last_cycle
    last_cycle = (view.id(), view.change_count(), line, state)
    sublime.status_message(state.upper())


def last_state(view, line: int):
    if last_cycle and last_cycle[:3] == (view.id(), view.change_count(), line):
        return last_cycle[3]
    return None


def children_of(outer, ranges):
    inside = [r for r in ranges if r['startLine'] > outer['startLine'] and r['endLine'] <= outer['endLine']]
    return [r for r in inside if not any(
        o is not r and o['startLine'] < r['startLine'] and r['endLine'] <= o['endLine'] for o in inside)]


class OrgLspCycleCommand(LspTextCommand):
    """Org's TAB (`org-cycle`) on the fold that starts at the cursor's line:
    FOLDED, CHILDREN, SUBTREE. On a line that starts no fold, `fallback`
    runs (Tab's own), or else the innermost fold around the cursor cycles."""

    session_name = SESSION

    def run(self, edit, fallback=None) -> None:
        session = self.session_by_name(SESSION)
        if not session:
            return
        params = {'textDocument': text_document_identifier(self.view)}
        session.send_request(
            Request('textDocument/foldingRange', params),
            lambda ranges: sublime.set_timeout(lambda: self.cycle(ranges or [], fallback)))

    def cycle(self, ranges, fallback) -> None:
        view = self.view
        ranges = [r for r in ranges if r['endLine'] > r['startLine']]
        line = view.rowcol(view.sel()[0].b)[0]
        current = next((r for r in ranges if r['startLine'] == line), None)
        if current is None:
            if fallback:
                view.run_command(fallback['command'], fallback.get('args'))
                return
            around = [r for r in ranges if r['startLine'] < line <= r['endLine']]
            if not around:
                return
            current = max(around, key=lambda r: r['startLine'])
        start = current['startLine']
        region = fold_region(view, current)
        if view.is_folded(region):
            view.unfold(region)
            children = children_of(current, ranges)
            for child in children:
                view.fold(fold_region(view, child))
            remember(view, start, 'children' if children else 'subtree')
        elif last_state(view, start) == 'children':
            view.unfold(region)
            remember(view, start, 'subtree')
        else:
            view.fold(region)
            remember(view, start, 'folded')


class OrgLspCycleGlobalCommand(LspTextCommand):
    """Org's S-TAB (`org-global-cycle`): OVERVIEW, CONTENTS, SHOW ALL."""

    session_name = SESSION

    def run(self, edit) -> None:
        session = self.session_by_name(SESSION)
        if not session:
            return
        params = {'textDocument': text_document_identifier(self.view)}
        session.send_request(
            Request('textDocument/foldingRange', params),
            lambda ranges: sublime.set_timeout(lambda: self.cycle(ranges or [])))

    def cycle(self, ranges) -> None:
        view = self.view
        ranges = [r for r in ranges if r['endLine'] > r['startLine']]
        state = last_state(view, -1)
        view.unfold(sublime.Region(0, view.size()))
        if state == 'overview':
            def headline(r):
                return view.substr(view.line(view.text_point(r['startLine'], 0))).startswith('*')
            parents = [r for r in ranges if headline(r) and any(
                headline(i) and i['startLine'] > r['startLine'] and i['endLine'] <= r['endLine'] for i in ranges)]
            for r in ranges:
                if r not in parents:
                    view.fold(fold_region(view, r))
            remember(view, -1, 'contents')
        elif state == 'contents':
            remember(view, -1, 'show all')
        else:
            top = [r for r in ranges if not any(
                o is not r and o['startLine'] < r['startLine'] and r['endLine'] <= o['endLine'] for o in ranges)]
            for r in top:
                view.fold(fold_region(view, r))
            remember(view, -1, 'overview')


class OrgLspListener(sublime_plugin.EventListener):
    def on_query_context(self, view, key, operator, operand, match_all):
        """`org_lsp.keys`: the preset of keys the settings pick (emacs, vim
        or custom); `org_lsp.capture`: whether the view is a capture
        buffer."""
        if key == 'org_lsp.keys':
            value = settings().get('org_keys', 'emacs')
        elif key == 'org_lsp.capture':
            value = (view.file_name() or '').split('/')[-1].split('\\')[-1].startswith('CAPTURE-')
        else:
            return None
        if operator == sublime.OP_EQUAL:
            return value == operand
        if operator == sublime.OP_NOT_EQUAL:
            return value != operand
        return None

    def on_load(self, view) -> None:
        # The agenda is a file the server writes, to read, not to edit.
        if (view.file_name() or '').endswith('.org-agenda'):
            view.set_read_only(True)
            view.set_scratch(True)
