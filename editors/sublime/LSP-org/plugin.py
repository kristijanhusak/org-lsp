"""Sublime Text glue for the org-lsp language server, on top of sublimelsp/LSP.

The server does the Org work and describes itself (its commands and their
keys, how its tokens look); Default.sublime-keymap and the settings are
generated from that (generate.mjs). This file runs the server's commands
with the cursor position, asks what they ask for, and cycles folds.
"""
from __future__ import annotations

import shutil

import sublime
import sublime_plugin
from LSP.plugin import AbstractPlugin
from LSP.plugin import LspTextCommand
from LSP.plugin import LspWindowCommand
from LSP.plugin import Request
from LSP.plugin import parse_uri
from LSP.plugin import register_plugin
from LSP.plugin import text_document_identifier
from LSP.plugin import text_document_position_params
from LSP.plugin import unregister_plugin

SESSION = 'LSP-org'
SETTINGS = 'LSP-org.sublime-settings'
CLOCK_KEY = 'org-lsp-clock'


def settings() -> sublime.Settings:
    return sublime.load_settings(SETTINGS)


class OrgLsp(AbstractPlugin):
    @classmethod
    def name(cls) -> str:
        return SESSION

    @classmethod
    def on_pre_start(cls, window, initiating_view, workspace_folders, configuration):
        # `org-lsp` from PATH, unless a path is configured.
        if configuration.command and configuration.command[0] == 'org-lsp':
            found = shutil.which('org-lsp')
            if found:
                configuration.command[0] = found
        # This package shows the documents the server asks it to.
        options = configuration.init_options
        options.set('showDocument', True)
        return None

    # The server changed files from elsewhere (the agenda, a capture, a
    # refile), and `autoSave` is on: save them.
    def m_org_lsp_save(self, params) -> None:
        uris = (params or {}).get('uris', [])
        sublime.set_timeout(lambda: save(uris))

    # The clock that runs, for the status bar.
    def m_org_lsp_clock(self, params) -> None:
        sublime.set_timeout(lambda: show_clock(params))


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


def is_org(view) -> bool:
    return bool(view) and view.match_selector(0, 'text.org | text.org-agenda')


class OrgLspRunCommand(LspWindowCommand):
    """Run a server command where the cursor is. `fallback` is the command
    Sublime runs instead where the server says the command does not apply
    (a key that keeps its own meaning)."""

    session_name = SESSION

    def is_enabled(self, **kwargs) -> bool:
        return True

    def run(self, command: str, fallback=None) -> None:
        session = self.session_by_name(SESSION)
        if not session:
            sublime.status_message('org-lsp: open an Org file first')
            return
        view = self.window.active_view()
        if is_org(view):
            context = text_document_position_params(view, view.sel()[0].b)
        else:
            context = {}
        self.execute(session, view, command, context, fallback)

    def execute(self, session, view, command, context, fallback) -> None:
        params = {'command': command, 'arguments': [context] if context else []}
        session.send_request(
            Request('workspace/executeCommand', params),
            lambda result: sublime.set_timeout(lambda: self.done(session, view, command, context, fallback, result)),
            lambda error: sublime.set_timeout(lambda: self.failed(view, fallback, error)),
        )

    def failed(self, view, fallback, error) -> None:
        data = error.get('data') if isinstance(error, dict) else None
        if fallback and isinstance(data, dict) and data.get('notApplicable'):
            view.run_command(fallback['command'], fallback.get('args'))
        else:
            sublime.status_message('org-lsp: {}'.format(error.get('message', error) if isinstance(error, dict) else error))

    def done(self, session, view, command, context, fallback, result) -> None:
        result = result if isinstance(result, dict) else {}
        if isinstance(result.get('save'), list):
            save(result['save'])
        if isinstance(result.get('message'), str):
            sublime.status_message(result['message'])
        if isinstance(result.get('close'), str):
            close(result['close'])
        request = result.get('input')
        if isinstance(request, dict):
            ask(self.window, request, lambda answer: self.answered(session, view, command, context, fallback, request, answer))

    def answered(self, session, view, command, context, fallback, request, answer) -> None:
        context = dict(context)
        context.update(request.get('context') or {})
        context['input'] = answer
        self.execute(session, view, command, context, fallback)


def ask(window, request, then) -> None:
    """Ask what a command asks for: a key or one of a list in the quick
    panel, or text in the input panel."""
    choices = request.get('choices')
    if choices:
        if request.get('kind') == 'key':
            items = ['[{}] {}'.format('SPC' if c['key'] == ' ' else c['key'], c['label']) for c in choices]
        else:
            items = [c['label'] for c in choices]

        def picked(index: int) -> None:
            if index >= 0:
                then(choices[index]['key'])
        window.show_quick_panel(items, picked, placeholder=request.get('prompt', ''))
        return
    window.show_input_panel(request.get('prompt', '') + ':', request.get('value', ''), then, None, None)


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
