"""The package's plugin against stand-ins for Sublime Text and sublimelsp/LSP,
which cannot run here: what it sends to the server, and what it does with
the answers.

    python3 editors/sublime/test/test_plugin.py
"""
import importlib.util
import json
import os
import sys
import types
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))


class Region:
    def __init__(self, a, b=None):
        self.a = a
        self.b = a if b is None else b

    def begin(self):
        return min(self.a, self.b)

    def end(self):
        return max(self.a, self.b)

    def __eq__(self, other):
        return (self.begin(), self.end()) == (other.begin(), other.end())

    def __hash__(self):
        return hash((self.begin(), self.end()))

    def __repr__(self):
        return 'Region({}, {})'.format(self.a, self.b)


class View:
    def __init__(self, text, name='/notes/work.org'):
        self.text = text
        self.name = name
        self.folds = set()
        self.cursor = 0
        self.commands = []
        self.changes = 0

    def lines(self):
        return self.text.split('\n')

    def text_point(self, row, col):
        return sum(len(line) + 1 for line in self.lines()[:row]) + col

    def rowcol(self, point):
        row = self.text[:point].count('\n')
        return row, point - self.text_point(row, 0)

    def line(self, point):
        row, _ = self.rowcol(point)
        start = self.text_point(row, 0)
        return Region(start, start + len(self.lines()[row]))

    def substr(self, region):
        return self.text[region.begin():region.end()]

    def sel(self):
        view = self

        class Selection(list):
            def clear(self):
                pass

            def add(self, region):
                view.cursor = region.b
        return Selection([Region(self.cursor)])

    def size(self):
        return len(self.text)

    def fold(self, region):
        self.folds.add(region)

    def unfold(self, region):
        self.folds = {f for f in self.folds if f.end() < region.begin() or f.begin() > region.end()}

    def is_folded(self, region):
        return region in self.folds

    def id(self):
        return 1

    def change_count(self):
        return self.changes

    def run_command(self, name, args=None):
        self.commands.append((name, args))

    def match_selector(self, point, selector):
        scope = 'text.org-agenda' if self.name.endswith('.org-agenda') else 'text.org'
        return scope in [part.strip() for part in selector.split('|')]

    def file_name(self):
        return self.name


class Window:
    def __init__(self, view):
        self.view = view
        self.panels = []

    def active_view(self):
        return self.view

    def show_quick_panel(self, items, on_select, placeholder=''):
        self.panels.append(('quick', items, placeholder))
        on_select(self.pick)

    def show_input_panel(self, caption, initial, on_done, on_change, on_cancel):
        self.panels.append(('input', caption, initial))
        on_done(self.typed)

    def find_open_file(self, path):
        return self.view if path == self.view.name else None

    def views(self):
        return [self.view]


class Session:
    def __init__(self, answers):
        self.answers = answers
        self.sent = []

    def send_request(self, request, on_result, on_error=None):
        self.sent.append((request.method, request.params))
        kind, value = self.answers.pop(0)
        (on_result if kind == 'result' else on_error)(value)


def load_plugin():
    sublime = types.ModuleType('sublime')
    sublime.Region = Region
    sublime.OP_EQUAL, sublime.OP_NOT_EQUAL = 0, 1
    sublime.set_timeout = lambda f, delay=0: f()
    sublime.messages = []
    sublime.status_message = sublime.messages.append
    sublime.settings = {}
    sublime.load_settings = lambda name: types.SimpleNamespace(get=lambda k, d=None: sublime.settings.get(k, d))
    sublime.windows = lambda: []
    package = os.path.join(HERE, '..', 'LSP-org')
    sublime.load_resource = lambda name: open(os.path.join(package, name.split('/')[-1]), encoding='utf-8').read()
    sublime.decode_value = json.loads
    sublime_plugin = types.ModuleType('sublime_plugin')
    sublime_plugin.EventListener = object
    sublime_plugin.WindowCommand = object
    lsp = types.ModuleType('LSP')
    api = types.ModuleType('LSP.plugin')

    class Command:
        session_name = ''
        current = None

        def __init__(self, target):
            self.window = target
            self.view = target

        def session_by_name(self, name):
            return Command.current

    api.AbstractPlugin = object
    api.applied = []

    class Done:
        def then(self, callback):
            callback(None)
    api.apply_text_edits = lambda view, edits: (api.applied.append(edits), Done())[1]
    api.position_to_offset = lambda position, view: view.text_point(position['line'], position['character'])
    api.LspTextCommand = Command
    api.LspWindowCommand = Command
    api.Request = lambda method, params: types.SimpleNamespace(method=method, params=params)
    api.parse_uri = lambda uri: ('file', uri[len('file://'):])
    api.register_plugin = api.unregister_plugin = lambda plugin: None
    api.text_document_identifier = lambda view: {'uri': 'file://' + view.name}
    api.text_document_position_params = lambda view, point: {
        'textDocument': {'uri': 'file://' + view.name},
        'position': dict(zip(('line', 'character'), view.rowcol(point))),
    }
    sys.modules.update({'sublime': sublime, 'sublime_plugin': sublime_plugin, 'LSP': lsp, 'LSP.plugin': api})
    spec = importlib.util.spec_from_file_location('plugin', os.path.join(HERE, '..', 'LSP-org', 'plugin.py'))
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)
    return plugin, sublime, Command


TEXT = '* A\ntext\n** B\nb text\n** C\nc text\n* D\nd text'
RANGES = [
    {'startLine': 0, 'endLine': 5},
    {'startLine': 2, 'endLine': 3},
    {'startLine': 4, 'endLine': 5},
    {'startLine': 6, 'endLine': 7},
]


class Tests(unittest.TestCase):
    def setUp(self):
        self.plugin, self.sublime, self.Command = load_plugin()
        self.view = View(TEXT)
        self.window = Window(self.view)

    def run_command(self, answers, **args):
        session = Session(answers)
        self.Command.current = session
        self.plugin.OrgLspRunCommand(self.window).run(**args)
        return session

    def test_runs_a_command_where_the_cursor_is_and_answers_what_it_asks(self):
        self.view.cursor = self.view.text_point(2, 1)
        self.window.pick = 1
        choices = [{'key': 'a', 'label': 'A'}, {'key': 'd', 'label': 'D'}]
        session = self.run_command(
            [('result', {'input': {'kind': 'choice', 'prompt': 'Refile to', 'choices': choices, 'context': {'step': 2}}}),
             ('result', {'message': 'Refiled to D'})],
            command='org.refile')
        self.assertEqual(session.sent[0], ('workspace/executeCommand', {
            'command': 'org.refile',
            'arguments': [{'textDocument': {'uri': 'file:///notes/work.org'}, 'position': {'line': 2, 'character': 1}}],
        }))
        self.assertEqual(session.sent[1][1]['arguments'][0]['input'], 'd')
        self.assertEqual(session.sent[1][1]['arguments'][0]['step'], 2)
        self.assertEqual(self.window.panels[0], ('quick', ['A', 'D'], 'Refile to'))
        self.assertIn('Refiled to D', self.sublime.messages)

    def test_applies_the_edit_a_result_carries(self):
        edits = [{'range': {'start': {'line': 0, 'character': 3}, 'end': {'line': 0, 'character': 3}}, 'newText': '\n'}]
        self.run_command([('result', {'edits': edits, 'cursor': {'line': 1, 'character': 0}})], command='org.return')
        self.assertEqual(sys.modules['LSP.plugin'].applied, [edits])
        self.assertEqual(self.view.cursor, self.view.text_point(1, 0))

    def test_asks_for_text_and_keys(self):
        self.window.typed = '+1d'
        session = self.run_command(
            [('result', {'input': {'kind': 'date', 'prompt': 'Deadline', 'value': ''}}), ('result', None)],
            command='org.deadline')
        self.assertEqual(self.window.panels[0], ('input', 'Deadline:', ''))
        self.assertEqual(session.sent[1][1]['arguments'][0]['input'], '+1d')
        self.window.pick = 0
        self.run_command(
            [('result', {'input': {'kind': 'key', 'prompt': 'Template', 'choices': [{'key': 't', 'label': 'Task'}]}}),
             ('result', None)],
            command='org.capture')
        self.assertEqual(self.window.panels[1], ('quick', ['[t] Task'], 'Template'))

    def test_a_key_keeps_its_own_meaning_where_the_command_does_not_apply(self):
        fallback = {'command': 'move', 'args': {'by': 'lines', 'forward': False, 'extend': True}}
        self.run_command([('error', {'code': -32803, 'message': 'Not here', 'data': {'notApplicable': True}})],
                         command='org.shift.up', fallback=fallback)
        self.assertEqual(self.view.commands, [('move', fallback['args'])])
        self.run_command([('error', {'code': -32803, 'message': 'No clock'})], command='org.clock.out')
        self.assertIn('org-lsp: No clock', self.sublime.messages)

    def cycle(self, line, **args):
        self.view.cursor = self.view.text_point(line, 0)
        self.Command.current = Session([('result', RANGES)])
        self.plugin.OrgLspCycleCommand(self.view).run(None, **args)

    def folded_rows(self):
        return sorted(self.view.rowcol(f.begin())[0] for f in self.view.folds)

    def test_cycles_a_subtree_as_tab_does(self):
        self.cycle(0)
        self.assertEqual(self.folded_rows(), [0])
        self.cycle(0)
        self.assertEqual(self.folded_rows(), [2, 4])
        self.cycle(0)
        self.assertEqual(self.folded_rows(), [])
        self.cycle(0)
        self.assertEqual(self.folded_rows(), [0])
        # Off a fold, Tab is Tab.
        fallback = {'command': 'insert_best_completion', 'args': {'default': '\t', 'exact': False}}
        self.cycle(1, fallback=fallback)
        self.assertEqual(self.view.commands, [('insert_best_completion', fallback['args'])])

    def test_cycles_the_file_as_shift_tab_does(self):
        def cycle():
            self.Command.current = Session([('result', RANGES)])
            self.plugin.OrgLspCycleGlobalCommand(self.view).run(None)
        cycle()
        self.assertEqual(self.folded_rows(), [0, 6])
        cycle()
        self.assertEqual(self.folded_rows(), [2, 4, 6])
        cycle()
        self.assertEqual(self.folded_rows(), [])

    def test_the_contexts_of_the_keys(self):
        listener = self.plugin.OrgLspListener()
        self.assertTrue(listener.on_query_context(self.view, 'org_lsp.keys', 0, 'emacs', True))
        self.sublime.settings['org_keys'] = 'vim'
        self.assertFalse(listener.on_query_context(self.view, 'org_lsp.keys', 0, 'emacs', True))
        self.assertFalse(listener.on_query_context(self.view, 'org_lsp.capture', 0, True, True))
        capture = View('* TODO ', '/tmp/CAPTURE-1.org')
        self.assertTrue(listener.on_query_context(capture, 'org_lsp.capture', 0, True, True))
        self.assertIsNone(listener.on_query_context(self.view, 'other', 0, True, True))


class Help(unittest.TestCase):
    def setUp(self):
        self.plugin, self.sublime, _ = load_plugin()

    def test_lists_the_keys_of_the_view_with_the_preset(self):
        items = self.plugin.help_items(View('* TODO x'))
        shown = [(keys, title) for keys, title, _, _ in items]
        self.assertIn(('Ctrl+C Ctrl+T', 'Change the TODO state'), shown)
        self.assertIn(('Tab', 'Cycle the visibility of the subtree, block or drawer at the cursor'), shown)
        self.assertNotIn('Capture: finish', [title for _, title in shown])
        agenda = [title for _, title, _, _ in self.plugin.help_items(View('', '/tmp/Agenda.org-agenda'))]
        self.assertIn('Agenda: rebuild', agenda)
        self.sublime.settings['org_keys'] = 'custom'
        self.assertEqual(self.plugin.help_items(View('* x')), [])


if __name__ == '__main__':
    unittest.main()
