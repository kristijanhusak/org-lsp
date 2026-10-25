"""The package's plugin against stand-ins for Sublime Text and sublimelsp/LSP,
which cannot run here: what it sends to the server, and what it does with
the answers.

The stand-ins keep Sublime's order of things: what `set_timeout` puts off
and LSP's answers come once the code that asked has returned, in the order
of their time (`Loop`), and a panel waits for the user, who answers it a
few seconds later, after what those seconds bring (a popup that goes, a
message): a panel shown over another one cancels that one, as Sublime's
do, and an answer to a panel no longer shown is lost. Keys are pressed
through the package's keymap (`Tests.press`), its contexts asked of the
plugin as Sublime asks them.

    python3 editors/sublime/test/test_plugin.py
"""
import heapq
import importlib.util
import io
import json
import os
import stat
import sys
import tarfile
import tempfile
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

    def empty(self):
        return self.a == self.b

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
        # Every popup shown, and whether one shows now.
        self.popups = []
        self.popup_visible = False

    def show_popup(self, content, flags, location, max_width):
        self.popups.append((content, location))
        self.popup_visible = True

    def is_popup_visible(self):
        return self.popup_visible

    def hide_popup(self):
        self.popup_visible = False

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
        return Selection([getattr(self, 'selection', None) or Region(self.cursor)])

    def size(self):
        return len(self.text)

    def fold(self, region):
        for one in region if isinstance(region, list) else [region]:
            self.folds.add(one)

    def unfold(self, region):
        for one in region if isinstance(region, list) else [region]:
            self.folds = {f for f in self.folds if f.end() < one.begin() or f.begin() > one.end()}

    def folded_regions(self):
        return sorted(self.folds, key=lambda region: region.begin())

    def is_folded(self, region):
        return region in self.folds

    def id(self):
        return 1

    def change_count(self):
        return self.changes

    def add_regions(self, key, regions, scope='', icon='', flags=0):
        self.regions = getattr(self, 'regions', {})
        self.regions[key] = (regions, scope, flags)

    def erase_regions(self, key):
        getattr(self, 'regions', {}).pop(key, None)

    def get_regions(self, key):
        return getattr(self, 'regions', {}).get(key, ([], '', 0))[0]

    def style(self):
        return {'foreground': '#d8dee9ff'}

    def run_command(self, name, args=None):
        self.commands.append((name, args))

    def match_selector(self, point, selector):
        scope = 'text.org-agenda' if self.name.endswith('.org-agenda') else \
            'text.org' if self.name.endswith('.org') or getattr(self, 'syntax', '').endswith('/Org.sublime-syntax') \
            else 'text.plain'
        return scope in [part.strip() for part in selector.split('|')]

    def file_name(self):
        return self.name

    def is_loading(self):
        return getattr(self, 'loading', False)

    def assign_syntax(self, syntax):
        self.syntax = syntax

    def is_valid(self):
        return not getattr(self, 'closed', False)

    def set_status(self, key, text):
        self.statuses = getattr(self, 'statuses', {})
        self.statuses[key] = text

    def erase_status(self, key):
        getattr(self, 'statuses', {}).pop(key, None)

    def set_scratch(self, scratch):
        self.scratch = scratch

    def is_dirty(self):
        return False

    def close(self):
        self.closed = True


# How long the user takes to answer a panel, in milliseconds: longer than
# the popup of a message stays.
USER = 5000


class Loop:
    """Sublime's main thread: what `set_timeout` puts off, and LSP's
    answers, run once the code that asked has returned, in the order of
    their time, on a clock of the loop's own (milliseconds)."""

    def __init__(self):
        self.now = 0
        self.queue = []
        self.count = 0

    def later(self, callback, delay=0):
        heapq.heappush(self.queue, (self.now + max(delay, 0), self.count, callback))
        self.count += 1

    def run(self, for_ms=60000):
        """Run what comes in the next `for_ms` (a timer that ticks each
        second goes on, so there is an end)."""
        end = self.now + for_ms
        while self.queue and self.queue[0][0] <= end:
            at, _, callback = heapq.heappop(self.queue)
            self.now = max(self.now, at)
            callback()


class Window:
    def __init__(self, view):
        self.view = view
        self.panels = []
        # The panel shown, and the panels whose answer came too late.
        self.shown_panel = None
        self.lost = []

    def active_view(self):
        return self.view

    def show_quick_panel(self, items, on_select, placeholder=''):
        self.panels.append(('quick', items, placeholder))
        # The user goes down to the item `pick` and presses Enter.
        self.show_panel(lambda: on_select(-1), lambda: on_select(self.pick) if hasattr(self, 'pick') else None)

    def show_input_panel(self, caption, initial, on_done, on_change, on_cancel):
        self.panels.append(('input', caption, initial))
        # The user types `typed` in place of the initial text, and Enter.
        self.show_panel(on_cancel or (lambda: None), lambda: on_done(self.typed) if hasattr(self, 'typed') else None)

    def show_panel(self, cancel, answer):
        """A panel shown: one shown before is cancelled, as Sublime closes
        it, and the user answers this one a few seconds later, if it is
        still shown then."""
        if self.shown_panel:
            shown, self.shown_panel = self.shown_panel, None
            shown['cancel']()
        panel = {'cancel': cancel, 'shown': self.panels[-1]}
        self.shown_panel = panel

        def answered():
            if self.shown_panel is not panel:
                self.lost.append(panel['shown'])
                return
            self.shown_panel = None
            answer()
        sys.modules['sublime'].loop.later(answered, USER)

    def find_open_file(self, path):
        return self.view if path == self.view.name else None

    def views(self):
        return [self.view]

    def num_groups(self):
        return 1

    def active_view_in_group(self, group):
        return self.view

    def open_file(self, path):
        self.opened = View(open(path, encoding='utf-8').read(), path)
        if getattr(self, 'on_open', None):
            self.on_open(self.opened)
        return self.opened


class Session:
    def __init__(self, answers):
        self.answers = answers
        self.sent = []
        self.capabilities = {'experimental.org.commands': [{'command': 'org.paste.subtree', 'clipboard': True}]}

    def get_capability(self, path):
        return self.capabilities.get(path)

    def send_request(self, request, on_result, on_error=None):
        self.sent.append((request.method, request.params))
        kind, value = self.answers.pop(0)
        # The answer comes later, from the server.
        sys.modules['sublime'].loop.later(lambda: (on_result if kind == 'result' else on_error)(value), 20)


def load_plugin():
    sublime = types.ModuleType('sublime')
    sublime.Region = Region
    sublime.OP_EQUAL, sublime.OP_NOT_EQUAL = 0, 1
    sublime.loop = Loop()
    sublime.set_timeout = lambda f, delay=0: sublime.loop.later(f, delay)
    sublime.set_timeout_async = lambda f, delay=0: sublime.loop.later(f, delay)
    sublime.LAYOUT_INLINE = 0
    sublime.LAYOUT_BELOW = 1
    sublime.HIDDEN = 128
    sublime.HIDE_ON_MOUSE_MOVE_AWAY = 2
    sublime.DRAW_NO_OUTLINE = 256
    sublime.clipboard = ''
    sublime.get_clipboard = lambda: sublime.clipboard
    sublime.set_clipboard = lambda text: setattr(sublime, 'clipboard', text)
    sublime.ENCODED_POSITION = 1
    sublime.Phantom = lambda region, html, layout: types.SimpleNamespace(region=region, html=html, layout=layout)

    class PhantomSet:
        made = []

        def __init__(self, view, key):
            self.phantoms = []
            PhantomSet.made.append(self)

        def update(self, phantoms):
            self.phantoms = phantoms
    sublime.PhantomSet = PhantomSet
    sublime.messages = []
    sublime.status_message = sublime.messages.append
    sublime.settings = {}
    sublime.load_settings = lambda name: types.SimpleNamespace(get=lambda k, d=None: sublime.settings.get(k, d))
    sublime.windows = lambda: []
    sublime.platform = lambda: 'linux'
    sublime.arch = lambda: 'x64'
    sublime.packages = tempfile.mkdtemp()
    sublime.packages_path = lambda: sublime.packages
    sublime.cache = tempfile.mkdtemp()
    sublime.cache_path = lambda: sublime.cache
    package = os.path.join(HERE, '..', 'LSP-org')
    sublime.load_resource = lambda name: open(os.path.join(package, name.split('/')[-1]), encoding='utf-8').read()
    sublime.decode_value = json.loads
    sublime.QuickPanelItem = lambda trigger, details='', annotation='', kind=None: (trigger, kind)
    sublime.KIND_AMBIGUOUS = (0, '', '')
    for number, name in enumerate(['REDISH', 'ORANGISH', 'YELLOWISH', 'GREENISH', 'CYANISH', 'BLUISH',
                                   'PURPLISH', 'PINKISH', 'DARK', 'LIGHT']):
        setattr(sublime, 'KIND_ID_COLOR_' + name, 9 + number)
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

    storage = tempfile.mkdtemp()

    class AbstractPlugin:
        def __init__(self, weaksession):
            self.weaksession = weaksession

        @classmethod
        def storage_path(cls):
            return storage
    api.AbstractPlugin = AbstractPlugin
    api.applied = []

    class Done:
        def then(self, callback):
            callback(None)
    api.apply_text_edits = lambda view, edits: (api.applied.append(edits), Done())[1]
    api.position_to_offset = lambda position, view: view.text_point(position['line'], position['character'])
    api.LspTextCommand = Command
    api.LspWindowCommand = Command
    api.Request = lambda method, params: types.SimpleNamespace(method=method, params=params)
    api.Notification = lambda method, params: types.SimpleNamespace(method=method, params=params)
    api.Response = lambda request_id, result: types.SimpleNamespace(request_id=request_id, result=result)
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
        self.idle()
        return session

    def idle(self, for_ms=60000):
        """What Sublime does next, for `for_ms` or until nothing is left to
        do."""
        self.sublime.loop.run(for_ms)

    def press(self, keys, view=None, then=60000):
        """Press `keys` (`ctrl+c a`) in `view` as Sublime does: the last
        binding of the package's keymap for them whose contexts all hold
        runs, and then what follows, for `then` milliseconds."""
        view = view or self.view
        with open(os.path.join(HERE, '..', 'LSP-org', 'Default.sublime-keymap'), encoding='utf-8') as file:
            keymap = json.load(file)
        listener = self.plugin.OrgLspListener()
        # Sublime's own contexts, as they are in a view with only a cursor.
        own = {'auto_complete_visible': False, 'has_next_field': False, 'setting.command_mode': False,
               'selection_empty': view.sel()[0].empty()}

        def holds(context):
            operand = context.get('operand', True)
            equal = context.get('operator', 'equal') == 'equal'
            if context['key'] == 'selector':
                return view.match_selector(0, operand) == equal
            if context['key'] in own:
                return (own[context['key']] == operand) == equal
            operator = self.sublime.OP_EQUAL if equal else self.sublime.OP_NOT_EQUAL
            return listener.on_query_context(view, context['key'], operator, operand, context.get('match_all', False)) is True
        bound = [binding for binding in keymap
                 if binding['keys'] == keys.split(' ') and all(holds(c) for c in binding.get('context', []))]
        self.assertTrue(bound, 'no key {} here'.format(keys))
        binding = bound[-1]
        name = ''.join(part.capitalize() for part in binding['command'].split('_')) + 'Command'
        command = getattr(self.plugin, name)
        if issubclass(command, self.Command) and command.run.__code__.co_varnames[1] == 'edit':
            command(view).run(None, **binding.get('args', {}))
        else:
            command(self.window).run(**binding.get('args', {}))
        self.idle(then)

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
        # And in a popup at the cursor, titled Org, which goes again.
        content, location = self.view.popups[-1]
        self.assertIn('Refiled to D', content)
        self.assertIn('>Org<', content)
        self.assertEqual(location, self.view.cursor)
        self.assertFalse(self.view.popup_visible)

    def test_a_key_asks_and_the_menu_waits_for_the_answer_typed(self):
        # `C-c a`: the agenda's menu, a key to press, which waits while the
        # message said with it goes (its popup, after four seconds); the
        # user goes down to `[t]` and presses Enter.
        self.window.pick = 1
        choices = [{'key': 'a', 'label': 'Agenda for current week or day'},
                   {'key': 't', 'label': 'List of all TODO entries'}]
        session = Session([
            ('result', {'message': 'Agenda files changed', 'input': {
                'kind': 'key', 'prompt': 'Press key for agenda command', 'choices': choices, 'context': {'menu': 1}}}),
            ('result', None)])
        self.Command.current = session
        self.press('ctrl+c a')
        self.assertEqual(self.window.panels, [
            ('quick', ['[a] Agenda for current week or day', '[t] List of all TODO entries'],
             'Press key for agenda command')])
        self.assertEqual(self.window.lost, [])
        self.assertFalse(self.view.popup_visible)
        self.assertEqual([params['command'] for _, params in session.sent], ['org.agenda.dispatch'] * 2)
        answered = session.sent[1][1]['arguments'][0]
        self.assertEqual((answered['input'], answered['menu']), ('t', 1))

    def test_a_question_asked_over_another_gives_that_one_up(self):
        # A key that asks while the menu of another waits: Sublime closes
        # the menu, which gives its command up, and the answer goes to the
        # question shown.
        self.window.pick = 0
        menu = {'kind': 'key', 'prompt': 'Press key for agenda command', 'choices': [{'key': 'a', 'label': 'Agenda'}]}
        states = {'kind': 'key', 'prompt': 'Select a TODO state', 'choices': [{'key': 'd', 'label': 'DONE'}]}
        session = Session([('result', {'input': menu}), ('result', {'input': states}), ('result', None)])
        self.Command.current = session
        self.press('ctrl+c a', then=100)
        self.press('ctrl+c ctrl+t')
        self.assertEqual([panel[2] for panel in self.window.panels], ['Press key for agenda command', 'Select a TODO state'])
        self.assertEqual(self.window.lost, [self.window.panels[0]])
        self.assertEqual([(params['command'], (params['arguments'][0] if params['arguments'] else {}).get('input'))
                          for _, params in session.sent],
                         [('org.agenda.dispatch', None), ('org.todo', None), ('org.todo', 'd')])

    def test_sends_the_selection_as_the_region(self):
        self.view.selection = Region(self.view.text_point(1, 0), self.view.text_point(2, 1))
        session = self.run_command([('result', None)], command='org.meta.right')
        self.assertEqual(session.sent[0][1]['arguments'][0]['range'], {
            'start': {'line': 1, 'character': 0}, 'end': {'line': 2, 'character': 1},
        })
        self.assertEqual(session.sent[0][1]['arguments'][0]['position'], {'line': 2, 'character': 1})

    def test_messages_go_to_the_status_bar_only_when_asked(self):
        self.sublime.settings['org_messages'] = 'status'
        self.run_command([('result', {'message': 'Effort is now 1:00'})], command='org.effort.increase')
        self.assertIn('Effort is now 1:00', self.sublime.messages)
        self.assertEqual(self.view.popups, [])

    def test_applies_the_edit_a_result_carries(self):
        edits = [{'range': {'start': {'line': 0, 'character': 3}, 'end': {'line': 0, 'character': 3}}, 'newText': '\n'}]
        self.run_command([('result', {'edits': edits, 'cursor': {'line': 1, 'character': 0}})], command='org.return')
        self.assertEqual(sys.modules['LSP.plugin'].applied, [edits])
        self.assertEqual(self.view.cursor, self.view.text_point(1, 0))

    def test_cut_and_copied_text_goes_to_the_clipboard_and_pastes_from_it(self):
        self.sublime.clipboard = '* X\n'
        session = self.run_command([('result', {'copy': '** B\n'})], command='org.copy.subtree')
        self.assertEqual(self.sublime.clipboard, '** B\n')
        self.assertNotIn('clipboard', session.sent[-1][1]['arguments'][0])
        session = self.run_command([('result', None)], command='org.paste.subtree')
        self.assertEqual(session.sent[-1][1]['arguments'][0]['clipboard'], '** B\n')

    def test_shows_a_sparse_tree(self):
        self.view.text = '* A\nan apple\n* B\nnothing\n'
        sparse = {'lines': [1], 'highlights': [{'start': {'line': 1, 'character': 3}, 'end': {'line': 1, 'character': 8}}]}
        session = self.run_command(
            [('result', {'sparse': sparse}), ('result', [{'startLine': 0, 'endLine': 1}, {'startLine': 2, 'endLine': 3}])],
            command='org.sparse.tree')
        self.assertEqual(session.sent[-1][0], 'textDocument/foldingRange')
        regions, scope, _ = self.view.regions['org-lsp-sparse']
        self.assertEqual([self.view.substr(region) for region in regions], ['apple'])
        self.assertEqual(self.view.folds, {self.plugin.fold_region(self.view, {'startLine': 2, 'endLine': 3})})
        self.plugin.OrgLspListener().on_modified(self.view)
        self.assertNotIn('org-lsp-sparse', self.view.regions)

    def test_keeps_sparse_highlights_until_cleared(self):
        # `keepHighlights`: the highlights stay through changes, until the
        # server sends none (`C-c C-c`), the folds left as they are.
        self.view.text = '* A\nan apple\n* B\nnothing\n'
        sparse = {'lines': [0], 'highlights': [{'start': {'line': 0, 'character': 0}, 'end': {'line': 0, 'character': 1}}],
                  'keepHighlights': True}
        self.run_command([('result', {'sparse': sparse}), ('result', [])], command='org.sparse.tree')
        self.plugin.OrgLspListener().on_modified(self.view)
        self.assertIn('org-lsp-sparse', self.view.regions)
        cleared = {'keep': True, 'unfold': [], 'highlights': []}
        self.run_command([('result', {'sparse': cleared, 'message': 'Temporary highlights/overlays removed from current buffer'})],
                         command='org.checkbox.toggle')
        self.assertNotIn('org-lsp-sparse', self.view.regions)

    def test_tells_the_closed_folds(self):
        self.view.text = '* A\nan apple\n* B\nnothing\nmore\n'
        self.view.folds = {self.plugin.fold_region(self.view, {'startLine': 2, 'endLine': 4})}
        self.assertEqual(self.plugin.closed_folds(self.view), [[2, 4]])

    def test_reveals_the_context(self):
        self.view.text = '* A\nan apple\n* B\nnothing\n'
        first = self.plugin.fold_region(self.view, {'startLine': 0, 'endLine': 1})
        second = self.plugin.fold_region(self.view, {'startLine': 2, 'endLine': 3})
        self.view.folds = {first, second}
        sparse = {'lines': [0, 1], 'highlights': [], 'keep': True, 'unfold': [0]}
        self.run_command([('result', {'sparse': sparse})], command='org.reveal')
        self.assertEqual(self.view.folds, {second})

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

    def test_adds_a_colon_only_to_prompts_without_one(self):
        self.window.typed = ''
        self.run_command(
            [('result', {'input': {'kind': 'text', 'prompt': 'Keep how many minutes (default 90): ', 'value': ''}}),
             ('result', None)],
            command='org.clock.in')
        self.run_command(
            [('result', {'input': {'kind': 'text', 'prompt': 'Really? ', 'value': ''}}), ('result', None)],
            command='org.clock.in')
        self.assertEqual([panel[1] for panel in self.window.panels],
                         ['Keep how many minutes (default 90): ', 'Really? '])

    def test_offers_the_completions_of_a_prompt(self):
        # A completion picked is the answer.
        self.window.pick = 2
        input = {'kind': 'text', 'prompt': 'Node', 'value': '', 'completions': ['B note', 'A note'], 'select': True}
        session = self.run_command([('result', {'input': input}), ('result', None)], command='org.roam.node.find')
        self.assertEqual(self.window.panels[0], ('quick', ['Type an answer…', 'B note', 'A note'], 'Node'))
        self.assertEqual(session.sent[1][1]['arguments'][0]['input'], 'A note')
        # The first item: anything typed, here nothing (the default).
        self.window.panels.clear()
        self.window.pick, self.window.typed = 0, ''
        input = {'kind': 'text', 'prompt': 'Property [Owner]', 'value': '', 'completions': ['Owner', 'Effort']}
        session = self.run_command([('result', {'input': input}), ('result', None)], command='org.property.set')
        self.assertEqual(self.window.panels[1], ('input', 'Property [Owner]:', ''))
        self.assertEqual(session.sent[1][1]['arguments'][0]['input'], '')
        # Tags: the one picked joins those there, in the input panel.
        self.window.panels.clear()
        self.window.pick, self.window.typed = 1, ':work:home:'
        input = {'kind': 'text', 'prompt': 'Tags', 'value': ':work:', 'completions': ['home'], 'separator': ':'}
        session = self.run_command([('result', {'input': input}), ('result', None)], command='org.tags.set')
        self.assertEqual(self.window.panels[1], ('input', 'Tags:', ':work:home:'))
        self.assertEqual(session.sent[1][1]['arguments'][0]['input'], ':work:home:')

    def test_offers_todo_keywords_in_their_colors(self):
        # As `org-fast-todo-selection` shows them: the kind of each item in
        # the keyword's color, its own face or that of a TODO or done keyword.
        self.window.pick = 0
        choices = [
            {'key': 't', 'label': 'TODO', 'highlight': 'todoKeyword'},
            {'key': 'w', 'label': 'WAIT', 'highlight': 'todoKeyword', 'face': {'foreground': 'orange'}},
            {'key': 'n', 'label': 'NEXT', 'highlight': 'todoKeyword', 'face': {'foreground': '#3366ff'}},
            {'key': 'd', 'label': 'DONE', 'highlight': 'todoKeyword.done'},
            {'key': ' ', 'label': 'clear'},
        ]
        self.run_command(
            [('result', {'input': {'kind': 'key', 'prompt': 'Select a TODO state', 'choices': choices}}),
             ('result', None)],
            command='org.todo')
        S = self.sublime
        self.assertEqual(self.window.panels[0][1], [
            ('[t] TODO', (S.KIND_ID_COLOR_REDISH, 't', '')),
            ('[w] WAIT', (S.KIND_ID_COLOR_ORANGISH, 'w', '')),
            ('[n] NEXT', (S.KIND_ID_COLOR_BLUISH, 'n', '')),
            ('[d] DONE', (S.KIND_ID_COLOR_GREENISH, 'd', '')),
            ('[SPC] clear', S.KIND_AMBIGUOUS),
        ])

    def test_starts_the_server_for_the_agenda_when_no_org_file_is_open(self):
        self.view.name = '/notes/readme.txt'
        session = Session([('result', None)])
        self.Command.current = None

        # LSP starts the server for the agenda file that is opened.
        def started(view):
            self.assertTrue(view.match_selector(0, 'text.org-agenda'))
            self.Command.current = session
        self.window.on_open = started
        self.plugin.OrgLspRunCommand(self.window).run(command='org.agenda.list')
        self.idle()
        self.assertEqual(session.sent, [('workspace/executeCommand', {'command': 'org.agenda.list', 'arguments': []})])
        self.assertTrue(self.window.opened.closed)
        # In an Org file, the key waits for its server.
        self.view.name = '/notes/work.org'
        self.Command.current = None
        self.window.opened = None
        self.plugin.OrgLspRunCommand(self.window).run(command='org.agenda.list')
        self.idle()
        self.assertIsNone(self.window.opened)
        self.assertIn('org-lsp: the server is starting', self.sublime.messages)

    def test_shows_org_indent_mode_prefixes_as_phantoms(self):
        self.sublime.windows = lambda: [self.window]
        self.plugin.show_virtual_indent({'uri': 'file:///notes/work.org', 'version': 1,
                                         'lines': [[0, 0, 2, 0], [1, 2, 2, 0], [3, 4, 6, 1], [99, 2, 2, 0]]})
        phantoms = self.sublime.PhantomSet.made[-1].phantoms
        self.assertEqual([(p.region.a, p.html.count('&nbsp;')) for p in phantoms],
                         [(self.view.text_point(1, 0), 2), (self.view.text_point(3, 0), 4)])
        self.plugin.show_virtual_indent({'uri': 'file:///notes/work.org', 'version': 2, 'lines': []})
        self.assertEqual(self.sublime.PhantomSet.made[-1].phantoms, [])
        self.assertEqual(len(self.sublime.PhantomSet.made), 1)

    def test_shows_images_below_the_lines_of_their_links(self):
        self.sublime.windows = lambda: [self.window]
        folder = tempfile.mkdtemp()
        picture = os.path.join(folder, 'red.png')
        with open(picture, 'wb') as out:
            out.write(b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR' + (1200).to_bytes(4, 'big') + (300).to_bytes(4, 'big'))
        self.view.text = '* A\nSee [[file:red.png]] here.\nend'
        link = {'start': {'line': 1, 'character': 4}, 'end': {'line': 1, 'character': 21}}
        preview = {'kind': 'link', 'uri': 'file:///notes/work.org', 'scope': 'image at point', 'range': link,
                   'remove': None, 'includeLinked': False, 'items': [{'range': link, 'path': picture}]}
        self.view.cursor = self.view.text_point(1, 6)
        self.run_command([('result', {'preview': preview})], command='org.link.preview')
        phantoms = self.sublime.PhantomSet.made[-1].phantoms
        self.assertEqual(len(phantoms), 1)
        self.assertEqual(phantoms[0].layout, self.sublime.LAYOUT_BELOW)
        self.assertEqual(phantoms[0].region.a, self.view.text_point(1, 21))
        # As wide as it may be, its proportions kept.
        self.assertIn('src="file://{}" width="800" height="200"'.format(picture), phantoms[0].html)
        self.assertIn('[image at point] Displaying 1 images inline', self.sublime.messages)
        # Again at the link: taken away.
        self.run_command([('result', {'preview': preview})], command='org.link.preview')
        self.assertEqual(self.sublime.PhantomSet.made[-1].phantoms, [])
        self.assertIn('[image at point] Inline link previews turned off (removed 1 images)', self.sublime.messages)
        # Shown again, it goes when its text changes.
        self.run_command([('result', {'preview': preview})], command='org.link.preview')
        self.assertEqual(len(self.sublime.PhantomSet.made[-1].phantoms), 1)
        self.view.text = self.view.text.replace('red.png', 'blue.png')
        self.plugin.OrgLspListener().on_modified(self.view)
        self.assertEqual(self.sublime.PhantomSet.made[-1].phantoms, [])

    def test_draws_latex_in_the_colors_of_the_text(self):
        self.view.cursor = self.view.text_point(1, 1)
        session = self.run_command([('result', {'message': 'Creating LaTeX previews...'})], command='org.latex.preview')
        context = session.sent[0][1]['arguments'][0]
        self.assertEqual((context['foreground'], context['background']), ('#d8dee9', 'Transparent'))

    def test_highlights_lines_from_edge_to_edge(self):
        self.sublime.windows = lambda: [self.window]
        self.plugin.show_line_highlights({'uri': 'file:///notes/work.org',
                                          'lines': [{'line': 1, 'highlight': 'agendaClocking'},
                                                    {'line': 99, 'highlight': 'agendaClocking'}]})
        regions, scope, _ = self.view.regions['org-lsp-line-agendaClocking']
        start = self.view.text_point(1, 0)
        self.assertEqual(regions, [Region(start, self.view.text_point(2, 0))], 'the line and its newline')
        self.assertIn('markup.highlight.clocking.agenda.org', scope)
        self.plugin.show_line_highlights({'uri': 'file:///notes/work.org', 'lines': []})
        self.assertNotIn('org-lsp-line-agendaClocking', self.view.regions)

    def test_hides_the_lines_a_result_names_in_folds(self):
        self.sublime.windows = lambda: [self.window]
        self.run_command([('result', {'hiddenLines': {'uri': 'file:///notes/work.org', 'lines': [1, 99]}})],
                         command='org.custom.properties.toggle')
        start = self.view.text_point(1, 0)
        hidden = Region(start - 1, self.view.line(start).end())
        self.assertEqual(self.view.folded_regions(), [hidden], 'the line, from the end of the one before')
        self.run_command([('result', {'hiddenLines': {'uri': 'file:///notes/work.org', 'lines': []}})],
                         command='org.custom.properties.toggle')
        self.assertEqual(self.view.folded_regions(), [], 'shown again')
        self.assertNotIn('org-lsp-hidden-lines', self.view.regions)

    def test_a_key_keeps_its_own_meaning_where_the_command_does_not_apply(self):
        fallback = {'command': 'move', 'args': {'by': 'lines', 'forward': False, 'extend': True}}
        self.run_command([('error', {'code': -32803, 'message': 'Not here', 'data': {'notApplicable': True}})],
                         command='org.shift.up', fallback=fallback)
        self.assertEqual(self.view.commands, [('move', fallback['args'])])
        self.run_command([('error', {'code': -32803, 'message': 'No clock'})], command='org.clock.out')
        self.assertIn('org-lsp: No clock', self.sublime.messages)
        self.assertIn('org-lsp: No clock', self.view.popups[-1][0])

    def cycle(self, line, before=None, **args):
        self.view.cursor = self.view.text_point(line, 0)
        self.Command.current = Session([('result', before or {'integrateLists': False}), ('result', RANGES)])
        self.plugin.OrgLspCycleCommand(self.view).run(None, **args)
        self.idle()

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

    def test_tab_first_asks_the_server_what_it_does(self):
        # At the start of the file, with `org_cycle_global_at_bob`, as S-TAB.
        self.cycle(0, before={'global': True})
        self.assertEqual(self.Command.current.sent[0][0], 'workspace/executeCommand')
        self.assertEqual(self.Command.current.sent[0][1]['command'], 'org.cycle')
        self.assertEqual(self.view.commands, [('org_lsp_cycle_global', None)])

    def test_cycles_the_file_as_shift_tab_does(self):
        def cycle():
            self.Command.current = Session([('result', RANGES)])
            self.plugin.OrgLspCycleGlobalCommand(self.view).run(None)
            self.idle()
        cycle()
        self.assertEqual(self.folded_rows(), [0, 6])
        cycle()
        self.assertEqual(self.folded_rows(), [2, 4, 6])
        cycle()
        self.assertEqual(self.folded_rows(), [])

    def test_a_file_opens_with_the_folds_the_server_says_closed(self):
        session = Session([('result', {'folds': [{'startLine': 2, 'endLine': 3}, {'startLine': 0, 'endLine': 5}]}),
                           ('result', None), ('result', None)])
        session.window = self.window
        plugin = self.plugin.OrgLsp(lambda: session)
        opened = types.SimpleNamespace(method='textDocument/didOpen', params={'textDocument': {'uri': 'file:///notes/work.org'}})
        plugin.on_pre_send_notification_async(opened)
        self.idle()
        self.assertEqual(session.sent[0], ('org-lsp/startupFolds', {'textDocument': {'uri': 'file:///notes/work.org'}}))
        self.assertEqual(self.folded_rows(), [0, 2])
        # And the previews it opens with (`#+STARTUP: inlineimages`, `latexpreview`).
        self.assertEqual([(method, params['command'], params['arguments'][0].get('startup'))
                          for method, params in session.sent[1:]],
                         [('workspace/executeCommand', 'org.link.preview', True),
                          ('workspace/executeCommand', 'org.latex.preview', True)])
        # Other notifications and other files ask nothing.
        plugin.on_pre_send_notification_async(types.SimpleNamespace(method='textDocument/didChange', params={}))
        self.assertEqual(len(session.sent), 3)

    def test_journal_files_are_org_and_fold_as_org_journal_says(self):
        """A file of org-journal's directory without an extension opens as
        Org, and once loaded, the folds the server names open and close."""
        journal = View('* Monday, 05/18/26\n** TODO call Bob\nnote\n** 10:30 ', '/journal/20260518')
        journal.loading = True
        self.window.view = journal
        self.sublime.windows = lambda: [self.window]
        session = Session([('result', {'folds': []}), ('result', None), ('result', None)])
        session.window = self.window
        session.capabilities['experimental.orgJournal.directory'] = '/journal/'
        plugin = self.plugin.OrgLsp(lambda: session)
        self.plugin.OrgLspListener().on_load(journal)
        self.assertEqual(journal.syntax, 'Packages/LSP-org/Org.sublime-syntax')
        # One opened before the server was up: when the server sees a file.
        other = View('* Sunday, 05/17/26\n', '/journal/20260517')
        self.window.views = lambda: [journal, other]
        opened = types.SimpleNamespace(method='textDocument/didOpen', params={'textDocument': {'uri': 'file:///journal/20260518'}})
        plugin.on_pre_send_notification_async(opened)
        self.idle()
        self.assertEqual(other.syntax, 'Packages/LSP-org/Org.sublime-syntax')
        self.assertEqual(session.sent[0][0], 'org-lsp/startupFolds')
        # Files with an extension, or elsewhere, are as they are.
        for name in ('/journal/20260518.txt', '/notes/20260518'):
            view = View('', name)
            self.plugin.OrgLspListener().on_load(view)
            self.assertFalse(hasattr(view, 'syntax'))
        journal.folds = {Region(17, 34)}
        plugin.m_org_lsp_folds({'uri': 'file:///journal/20260518',
                                'open': [{'startLine': 0, 'endLine': 3}],
                                'close': [{'startLine': 1, 'endLine': 2}]})
        self.assertEqual(journal.folds, {Region(17, 34)})
        journal.loading = False
        self.idle()
        self.assertEqual([journal.rowcol(f.begin())[0] for f in journal.folds], [1])

    def test_runs_the_command_the_server_asks_for(self):
        # A capture an org-protocol URL started (`org-lsp/runCommand`): run
        # where the cursor is, with the server's fields on top.
        self.view.cursor = self.view.text_point(2, 1)
        session = Session([('result', None)])
        self.Command.current = session
        session.window = self.window
        self.window.run_command = lambda name, args: self.plugin.OrgLspRunCommand(self.window).run(**args)
        plugin = self.plugin.OrgLsp(lambda: session)
        argument = {'keys': 'w', 'orgProtocol': {'props': [[':link', 'https://orgmode.org/']]}}
        plugin.m_org_lsp_runCommand({'command': 'org.capture', 'argument': argument})
        self.idle()
        method, params = session.sent[0]
        self.assertEqual((method, params['command']), ('workspace/executeCommand', 'org.capture'))
        sent = params['arguments'][0]
        self.assertEqual(sent['position'], {'line': 2, 'character': 1})
        self.assertEqual((sent['keys'], sent['orgProtocol']), ('w', argument['orgProtocol']))
        # The package says it runs them.
        given = {}
        configuration = types.SimpleNamespace(command=['other'], init_options=types.SimpleNamespace(set=given.__setitem__))
        self.plugin.OrgLsp.on_pre_start(None, None, [], configuration)
        self.assertIs(given['run_command'], True)

    def test_shows_the_timer_in_the_status_bar(self):
        later = []
        self.sublime.set_timeout = lambda f, delay=0: f() if delay == 0 else later.append(f)
        self.sublime.windows = lambda: [self.window]
        clock = [1000.0]
        self.addCleanup(setattr, self.plugin, 'time', self.plugin.time)
        self.plugin.time = types.SimpleNamespace(time=lambda: clock[0])
        plugin = self.plugin.OrgLsp(lambda: None)
        plugin.m_org_lsp_timer({'seconds': 323, 'countdown': False, 'paused': False})
        self.assertEqual(self.view.statuses['org-lsp-timer'], '<0:05:23>')
        # A second later, it shows the next one.
        clock[0] += 1
        later.pop(0)()
        self.assertEqual(self.view.statuses['org-lsp-timer'], '<0:05:24>')
        # A countdown counts down; once stopped, nothing shows.
        plugin.m_org_lsp_timer({'seconds': 60, 'countdown': True, 'paused': False})
        clock[0] += 2
        later.pop()()
        self.assertEqual(self.view.statuses['org-lsp-timer'], '<0:00:58>')
        plugin.m_org_lsp_timer(None)
        self.assertNotIn('org-lsp-timer', self.view.statuses)

    def test_the_contexts_of_the_keys(self):
        listener = self.plugin.OrgLspListener()
        self.assertTrue(listener.on_query_context(self.view, 'org_lsp.keys', 0, 'emacs', True))
        self.sublime.settings['org_keys'] = 'vim'
        self.assertFalse(listener.on_query_context(self.view, 'org_lsp.keys', 0, 'emacs', True))
        self.assertFalse(listener.on_query_context(self.view, 'org_lsp.capture', 0, True, True))
        capture = View('* TODO ', '/tmp/CAPTURE-1.org')
        self.assertTrue(listener.on_query_context(capture, 'org_lsp.capture', 0, True, True))
        self.assertIsNone(listener.on_query_context(self.view, 'other', 0, True, True))


    def test_tab_opens_the_entry_in_another_group(self):
        class Groups:
            """A window of groups, each a list of file names."""

            def __init__(self, groups, active=0):
                self.groups = groups
                self.active = active
                self.layout_set = None
                self.opened = None

            def active_group(self):
                return self.active

            def num_groups(self):
                return len(self.groups)

            def views_in_group(self, group):
                return [types.SimpleNamespace(file_name=lambda name=name: name) for name in self.groups[group]]

            def layout(self):
                return {'cols': [0.0, 1.0], 'rows': [0.0, 1.0], 'cells': [[0, 0, 1, 1]]}

            def set_layout(self, layout):
                self.layout_set = layout
                self.groups.append([])

            def focus_group(self, group):
                self.active = group

            def open_file(self, name, flags=0, group=-1):
                self.opened = (name, flags, group)

        other_group = self.plugin.other_group
        # The group that has the file, then an empty one.
        self.assertEqual(other_group(Groups([['agenda'], ['/a.org'], []]), '/a.org'), 1)
        self.assertEqual(other_group(Groups([['agenda'], ['/b.org'], []]), '/a.org'), 2)
        # Else a new column to the right.
        alone = Groups([['agenda']])
        self.assertEqual(other_group(alone, '/a.org'), 1)
        self.assertEqual(alone.layout_set, {
            'cols': [0.0, 0.5, 1.0], 'rows': [0.0, 1.0], 'cells': [[0, 0, 1, 1], [1, 0, 2, 1]]})
        window = Groups([['agenda']])
        selection = {'start': {'line': 2, 'character': 0}}
        self.plugin.show_in_other_group(window, {'uri': 'file:///a.org', 'selection': selection})
        self.assertEqual(window.opened, ('/a.org:3:1', self.sublime.ENCODED_POSITION, 1))
        self.assertEqual(window.active, 1)
        # Follow mode (`takeFocus: false`): the agenda keeps the focus.
        window = Groups([['agenda'], ['/a.org']])
        agenda = types.SimpleNamespace(name='agenda')
        window.active_view = lambda: agenda
        focused = []
        window.focus_view = focused.append
        params = {'uri': 'file:///a.org', 'selection': selection, 'takeFocus': False}
        self.plugin.show_in_other_group(window, params)
        self.assertEqual(window.opened, ('/a.org:3:1', self.sublime.ENCODED_POSITION, 1))
        self.assertEqual(focused, [agenda])

    def test_other_documents_are_shown_as_lsp_shows_them(self):
        """The agenda itself, and any document but an entry's from TAB, go
        to LSP's own handler, which takes the request's id: the decorated
        `on_window_show_document` of recent versions answers with the
        response the session sends, the `m_window_showDocument` of older
        ones sends it itself."""
        agenda = {'uri': 'file:///tmp/Agenda.org-agenda', 'takeFocus': True}

        class Recent:
            def on_window_show_document(self, params, request_id):
                return ('response', params, request_id)

        class Older:
            def m_window_showDocument(self, params, request_id):
                self.shown = (params, request_id)

        recent = Recent()
        handler = self.plugin.OrgLsp(lambda: recent)
        self.assertEqual(handler.m_window_showDocument(agenda, 7), ('response', agenda, 7))
        older = Older()
        handler = self.plugin.OrgLsp(lambda: older)
        self.assertIsNone(handler.m_window_showDocument(agenda, 8))
        self.assertEqual(older.shown, (agenda, 8))

    def test_shows_the_org_roam_buffer_beside_the_note_and_hides_it(self):
        """The org-roam buffer opens in another group, the cursor staying in
        the note; the toggle says whether it shows, for the server to hide
        it."""
        responses = []
        session = types.SimpleNamespace(
            window=self.window, get_capability=lambda path: None, send_response=responses.append)
        shown = []
        self.plugin.show_in_other_group = lambda window, params: shown.append(params)
        handler = self.plugin.OrgLsp(lambda: session)
        params = {'uri': 'file:///cache/roam/org-roam-buffer.org', 'takeFocus': False}
        self.assertIsNone(handler.m_window_showDocument(params, 4))
        self.idle()
        self.assertEqual(shown, [params])
        self.assertEqual((responses[0].request_id, responses[0].result), (4, {'success': True}))
        session = self.run_command([('result', None)], command='org.roam.buffer.toggle')
        self.assertIs(session.sent[0][1]['arguments'][0]['visible'], False)
        buffer = View('#+title: A\n', '/cache/roam/org-roam-buffer.org')
        self.window.view = buffer
        self.sublime.windows = lambda: [self.window]
        session = self.run_command([('result', {'close': 'file:///cache/roam/org-roam-buffer.org'})],
                                   command='org.roam.buffer.toggle')
        self.assertIs(session.sent[0][1]['arguments'][0]['visible'], True)
        self.assertTrue(buffer.closed)

    def test_tells_the_server_where_the_cursor_is_while_the_org_roam_buffer_shows(self):
        sent = []
        session = types.SimpleNamespace(send_notification=sent.append, get_capability=lambda path: None)
        self.plugin.OrgLsp(lambda: session)
        note = View(TEXT, '/roam/a.org')
        buffer = View('', '/cache/roam/org-roam-buffer.org')
        groups = {0: note, 1: None}
        window = types.SimpleNamespace(num_groups=lambda: 2, active_view_in_group=groups.get)
        note.window = buffer.window = lambda: window
        # Hidden: nothing to follow.
        note.cursor = note.text_point(2, 1)
        self.plugin.tell_roam_cursor(note)
        self.assertEqual(sent, [])
        groups[1] = buffer
        self.plugin.tell_roam_cursor(note)
        # The same line again, and the buffer itself, say nothing.
        note.cursor = note.text_point(2, 3)
        self.plugin.tell_roam_cursor(note)
        self.plugin.tell_roam_cursor(buffer)
        self.assertEqual([(note.method, note.params) for note in sent], [('org-lsp/cursor', {
            'textDocument': {'uri': 'file:///roam/a.org'}, 'position': {'line': 2, 'character': 1}})])

    def test_shows_the_outline_path_of_the_agenda_line(self):
        sent = []
        answer = {'echo': 'work.org/Projects'}
        session = types.SimpleNamespace(send_request=lambda request, on_result: (sent.append(request), on_result(answer)))
        self.plugin.OrgLsp(lambda: session)
        agenda = View('  work:  TODO Write\n  work:  TODO Draft\n', '/tmp/org-lsp-1/Agenda.org-agenda')
        agenda.cursor = agenda.text_point(1, 3)
        del self.sublime.messages[:]
        self.plugin.tell_agenda_cursor(agenda)
        # The same line again asks nothing.
        self.plugin.tell_agenda_cursor(agenda)
        self.assertEqual([(request.method, request.params['position']) for request in sent],
                         [('org-lsp/agendaCursor', {'line': 1, 'character': 0})])
        # `org-agenda-show-outline-path`: in the status bar, no popup.
        self.assertEqual(self.sublime.messages, ['work.org/Projects'])
        self.assertEqual(agenda.popups, [])

    def test_images_show_as_wide_as_the_server_says(self):
        view = types.SimpleNamespace(em_width=lambda: 8.0)
        entry = {'width': 400, 'height': 100}
        shown = lambda sizing: self.plugin.image_shown(view, dict(entry, sizing=sizing), 800)
        self.assertEqual(shown({}), (400, 100))
        # `org-image-actual-width`: pixels, or a part of the view.
        self.assertEqual(shown({'width': {'pixels': 200}}), (200, 50))
        self.assertEqual(shown({'width': {'fraction': 0.75}}), (600, 150))
        # `org-image-max-width`: 40 columns of 8 pixels, for one at its own width.
        self.assertEqual(shown({'maxWidth': {'columns': 40}}), (320, 80))
        self.assertEqual(shown({'maxWidth': {'columns': 70}}), (400, 100))

    def test_tells_the_server_how_wide_the_agenda_is(self):
        sent = []
        session = types.SimpleNamespace(send_notification=sent.append)
        self.plugin.OrgLsp(lambda: session)
        agenda = types.SimpleNamespace(
            file_name=lambda: '/tmp/org-lsp-1/Agenda.org-agenda',
            em_width=lambda: 8.0,
            viewport_extent=lambda: (803.0, 400.0),
        )
        window = types.SimpleNamespace(num_groups=lambda: 2, active_view_in_group=lambda group: [None, agenda][group])
        self.plugin.tell_agenda_width(window)
        self.plugin.tell_agenda_width(window)
        self.assertEqual([(note.method, note.params) for note in sent], [('org-lsp/agendaWidth', {'width': 100})])

def archive(binary=b'#!/bin/sh\n'):
    """The release's archive for Linux: `org-lsp` in a tar.gz."""
    out = io.BytesIO()
    with tarfile.open(fileobj=out, mode='w:gz') as files:
        info = tarfile.TarInfo('org-lsp')
        info.size = len(binary)
        files.addfile(info, io.BytesIO(binary))
    return out.getvalue()


class Server(unittest.TestCase):
    def setUp(self):
        self.plugin, self.sublime, _ = load_plugin()
        self.storage = self.plugin.OrgLsp.storage_path()
        self.empty = tempfile.mkdtemp()
        os.environ['PATH'], self.path = self.empty, os.environ.get('PATH', '')
        self.home = os.environ.get('HOME')
        os.environ['HOME'] = self.empty

    def tearDown(self):
        os.environ['PATH'] = self.path
        if self.home is not None:
            os.environ['HOME'] = self.home

    def test_names_its_settings_whatever_lsp_makes_of_the_name(self):
        _, path = self.plugin.OrgLsp.configuration()
        self.assertEqual(path, 'Packages/LSP-org/LSP-org.sublime-settings')
        self.assertEqual(self.plugin.OrgLsp.name(), 'LSP-org')

    def test_shows_the_agenda_document_in_the_agenda_syntax(self):
        # LSP opens an `org-agenda:` document in the syntax its scheme
        # names; without one, as plain text, which the session's selector
        # does not match.
        package = os.path.join(HERE, '..', 'LSP-org')
        with open(os.path.join(package, 'LSP-org.sublime-settings')) as f:
            settings = json.load(f)
        syntax = settings['syntax_map']['org-agenda']
        self.assertTrue(syntax.startswith('Packages/LSP-org/'))
        with open(os.path.join(package, syntax[len('Packages/LSP-org/'):])) as f:
            self.assertIn('scope: text.org-agenda', f.read())
        self.assertIn('text.org-agenda', settings['selector'])
        # And LSP attaches only views of the schemes the settings name.
        self.assertEqual(settings['schemes'], ['file', 'org-agenda'])
        # LSP writes a refreshed agenda with `insert`, which indents each
        # line by the one before it unless the syntax turns that off.
        with open(os.path.join(package, 'Org Agenda.sublime-settings')) as f:
            agenda_settings = f.read()
        self.assertIn('"auto_indent": false', agenda_settings)
        self.assertIn('"smart_indent": false', agenda_settings)

    def test_finds_the_server_on_the_search_path_with_cargo_s_folder(self):
        cargo = os.path.join(self.empty, '.cargo', 'bin')
        os.makedirs(cargo)
        binary = os.path.join(cargo, 'org-lsp')
        open(binary, 'w').close()
        os.chmod(binary, 0o755)
        self.assertEqual(self.plugin.server_path(self.storage), binary)
        self.assertFalse(self.plugin.OrgLsp.needs_update_or_installation())
        configuration = types.SimpleNamespace(command=['org-lsp'], init_options=types.SimpleNamespace(set=lambda k, v: None))
        self.plugin.OrgLsp.on_pre_start(None, None, [], configuration)
        self.assertEqual(configuration.command, [binary])

    def test_a_configured_command_is_used_as_it_is(self):
        self.sublime.settings['command'] = ['~/bin/org-lsp']
        self.assertEqual(self.plugin.server_path(self.storage), os.path.join(self.empty, 'bin', 'org-lsp'))

    def test_finds_the_build_of_the_checkout_it_is_linked_from(self):
        checkout = tempfile.mkdtemp()
        os.makedirs(os.path.join(checkout, 'editors', 'sublime', 'LSP-org'))
        os.makedirs(os.path.join(checkout, 'target', 'debug'))
        build = os.path.join(checkout, 'target', 'debug', 'org-lsp')
        open(build, 'w').close()
        os.symlink(os.path.join(checkout, 'editors', 'sublime', 'LSP-org'), os.path.join(self.sublime.packages, 'LSP-org'))
        self.assertEqual(self.plugin.server_path(self.storage), build)

    def test_downloads_its_own_release_when_there_is_none(self):
        self.assertTrue(self.plugin.OrgLsp.needs_update_or_installation())
        self.assertIsNotNone(self.plugin.OrgLsp.can_start(None, None, [], None))
        # What packages before kept, with no release in its path, is not
        # the package's release.
        unversioned = os.path.join(self.storage, 'LSP-org', 'org-lsp')
        os.makedirs(os.path.dirname(unversioned), exist_ok=True)
        open(unversioned, 'w').close()
        self.assertTrue(self.plugin.OrgLsp.needs_update_or_installation())
        asked = []

        class Response(io.BytesIO):
            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

        def urlopen(url, timeout=None):
            asked.append(url)
            return Response(archive(b'server'))
        self.plugin.urllib.request.urlopen = urlopen
        self.plugin.OrgLsp.install_or_update()
        release = 'v' + self.plugin.VERSION
        self.assertEqual(self.plugin.RELEASE, release)
        self.assertEqual(asked, ['https://github.com/kristijanhusak/org-lsp/releases/download/{}/org-lsp-linux-x86_64.tar.gz'.format(release)])
        binary = os.path.join(self.storage, 'LSP-org', release, 'org-lsp')
        with open(binary, 'rb') as written:
            self.assertEqual(written.read(), b'server')
        self.assertTrue(os.stat(binary).st_mode & stat.S_IXUSR)
        self.assertFalse(os.path.exists(unversioned))
        self.assertEqual(self.plugin.server_path(self.storage), binary)
        self.assertFalse(self.plugin.OrgLsp.needs_update_or_installation())
        self.assertIsNone(self.plugin.OrgLsp.can_start(None, None, [], None))

        # The package updated to another release: its own server is
        # downloaded, and the old one goes.
        self.plugin.RELEASE = 'v9.9.9'
        self.assertTrue(self.plugin.OrgLsp.needs_update_or_installation())
        self.plugin.OrgLsp.install_or_update()
        self.assertEqual(asked[-1], 'https://github.com/kristijanhusak/org-lsp/releases/download/v9.9.9/org-lsp-linux-x86_64.tar.gz')
        newer = os.path.join(self.storage, 'LSP-org', 'v9.9.9', 'org-lsp')
        self.assertEqual(self.plugin.server_path(self.storage), newer)
        self.assertFalse(os.path.exists(binary))
        self.assertFalse(self.plugin.OrgLsp.needs_update_or_installation())

    def test_a_download_is_kept_when_the_old_server_cannot_go(self):
        # On Windows the server of packages before, still running, cannot
        # be removed: the new one is downloaded all the same.
        unversioned = os.path.join(self.storage, 'LSP-org', 'org-lsp')
        os.makedirs(os.path.dirname(unversioned), exist_ok=True)
        open(unversioned, 'w').close()

        class Response(io.BytesIO):
            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

        self.plugin.urllib.request.urlopen = lambda url, timeout=None: Response(archive(b'server'))
        remove = self.plugin.os.remove

        def refuse(path):
            raise PermissionError(path)
        self.plugin.os.remove = refuse
        try:
            self.plugin.OrgLsp.install_or_update()
        finally:
            self.plugin.os.remove = remove
        binary = os.path.join(self.storage, 'LSP-org', self.plugin.RELEASE, 'org-lsp')
        self.assertEqual(self.plugin.server_path(self.storage), binary)
        self.assertTrue(os.path.exists(unversioned))
        self.assertFalse(self.plugin.OrgLsp.needs_update_or_installation())


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
