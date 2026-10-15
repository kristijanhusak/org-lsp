"""Sublime Text glue for the org-lsp language server, on top of sublimelsp/LSP.

The server does the Org work and describes itself (its commands and their
keys, how its tokens look); Default.sublime-keymap and the settings are
generated from that (generate.mjs). This file runs the server's commands
with the cursor position, asks what they ask for, and cycles folds.
"""
from __future__ import annotations

import colorsys
import html
import io
import os
import re
import shutil
import struct
import tarfile
import threading
import time
import urllib.request
import zipfile

import sublime
import sublime_plugin
from LSP.plugin import AbstractPlugin
from LSP.plugin import apply_text_edits
from LSP.plugin import position_to_offset
from LSP.plugin import LspTextCommand
from LSP.plugin import LspWindowCommand
from LSP.plugin import Notification
from LSP.plugin import Request
from LSP.plugin import Response
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
# How long to wait for a journal file to load, in steps of 100 ms.
JOURNAL_TRIES = 50
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


# The popup of the message shown last, so that hiding it later hides that
# one only.
_shown = {'count': 0}


def say(message, view=None, error=False) -> None:
    """Tell the user `message`: in the status bar, and with `org_messages`
    set to "popup" (the default) in a popup at the cursor of `view` (or the
    active view) that goes after a few seconds."""
    sublime.status_message(message)
    if settings().get('org_messages', 'popup') != 'popup':
        return
    view = view or sublime.active_window().active_view()
    if view is None or not view.sel():
        return
    color = 'color(var(--redish))' if error else 'color(var(--foreground))'
    content = (
        '<body id="org-lsp-message" style="margin: 0.3rem 0.6rem">'
        '<div style="color: var(--bluish); font-size: 0.8rem; font-weight: bold">Org</div>'
        '<div style="color: {}">{}</div></body>'
    ).format(color, html.escape(message).replace('\n', '<br>'))
    _shown['count'] += 1
    shown = _shown['count']
    view.show_popup(content, sublime.HIDE_ON_MOUSE_MOVE_AWAY, view.sel()[0].b, 600)

    def hide() -> None:
        if _shown['count'] == shown and view.is_popup_visible():
            view.hide_popup()
    sublime.set_timeout(hide, 4000)


def key_name(key: str) -> str:
    """How a key of a key input shows: Emacs's names for the blank ones,
    and `C-b` for a control character."""
    names = {' ': 'SPC', '\r': 'RET', '\t': 'TAB'}
    if key in names:
        return names[key]
    if len(key) == 1 and 0 < ord(key) < 32:
        return 'C-' + chr(ord(key) + 96)
    return key


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
    # The plugin of the running session, for the listeners.
    current = None

    def __init__(self, weaksession) -> None:
        super().__init__(weaksession)
        OrgLsp.current = self

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
        # org-lsp's own client: its token types, a command's result.
        options.set('org_client', True)
        options.set('show_document', True)
        # `org-indent-mode`'s prefixes, as phantoms (`org-lsp/virtualIndent`).
        options.set('virtual_indent', True)
        # Lines of the agenda highlighted from edge to edge, as regions
        # (`org-lsp/lineHighlights`).
        options.set('line_highlights', True)
        # Images in place of links and of LaTeX fragments, as phantoms
        # below their lines (`org.link.preview`, `org-lsp/images`).
        options.set('images', settings().get('org_images', True) is not False)
        return None

    # A file opens: the folds the server says it opens with closed
    # (`org_startup_folded`, `#+STARTUP:`, `VISIBILITY` properties), asked
    # for once the server has the file.
    def on_pre_send_notification_async(self, notification) -> None:
        if notification.method != 'textDocument/didOpen':
            return
        uri = notification.params.get('textDocument', {}).get('uri', '')
        session = self.weaksession()
        if session:
            # The server is up: the journal files open before it as plain text.
            sublime.set_timeout(lambda: show_journal_files(session))
        if not uri.endswith('.org') and not is_journal_file(session, parse_uri(uri)[1]):
            return
        if session:
            sublime.set_timeout_async(lambda: ask_startup_folds(session, uri))
            sublime.set_timeout(lambda: ask_startup_previews(session, uri))

    # The server changed files from elsewhere (the agenda, a capture, a
    # refile), and `auto_save` is on: save them.
    def m_org_lsp_save(self, params) -> None:
        uris = (params or {}).get('uris', [])
        sublime.set_timeout(lambda: save(uris))

    # The clock that runs, for the status bar.
    def m_org_lsp_clock(self, params) -> None:
        sublime.set_timeout(lambda: show_clock(params))

    # The relative timer or the countdown, for the status bar.
    def m_org_lsp_timer(self, params) -> None:
        sublime.set_timeout(lambda: show_timer(params))

    # How the lines of a file with `org-indent-mode` show.
    def m_org_lsp_virtualIndent(self, params) -> None:
        sublime.set_timeout(lambda: show_virtual_indent(params))

    # The lines of the agenda highlighted from edge to edge: the clocked entry.
    def m_org_lsp_lineHighlights(self, params) -> None:
        sublime.set_timeout(lambda: show_line_highlights(params))

    # Images in place of LaTeX fragments, made by Emacs.
    def m_org_lsp_images(self, params) -> None:
        sublime.set_timeout(lambda: apply_preview(params))

    # The folds org-journal opens and closes in the entry it shows
    # (`org-journal-hide-entries-p`).
    def m_org_lsp_folds(self, params) -> None:
        sublime.set_timeout(lambda: journal_folds(params or {}, JOURNAL_TRIES))

    # TAB in the agenda (`org-agenda-goto`): the entry's file in another
    # group (`orgOtherWindow`); the org-roam buffer beside the note, the
    # cursor staying in the note. LSP shows any other document as it does.
    def m_window_showDocument(self, params, request_id):
        session = self.weaksession()
        if not session:
            return None
        roam = is_roam_buffer(session, parse_uri((params or {}).get('uri', ''))[1])
        if not (params or {}).get('orgOtherWindow') and not roam:
            # LSP's own handler, with the request's id: the decorated
            # `on_window_show_document` of recent versions returns the
            # response for the session to send, the `m_window_showDocument`
            # of older ones sends it itself.
            default = getattr(session, 'on_window_show_document', None) \
                or getattr(session, 'm_window_showDocument', None)
            return default(params, request_id) if default else None
        window = session.window
        sublime.set_timeout(lambda: show_in_other_group(window, params))
        session.send_response(Response(request_id, {'success': True}))
        return None


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


def roam_buffer_prefix(session) -> str:
    """How the files of the org-roam buffer are named, as the server's
    manifest says."""
    get = getattr(session, 'get_capability', None) if session else None
    return (get('experimental.org.roamBuffer.prefix') if get else None) or 'org-roam-buffer'


def is_roam_buffer(session, path: str) -> bool:
    return os.path.basename(path or '').startswith(roam_buffer_prefix(session))


def roam_buffer_shown(session, window) -> bool:
    """Whether a group of `window` shows the persistent org-roam buffer."""
    name = roam_buffer_prefix(session) + '.org'
    for group in range(window.num_groups()):
        view = window.active_view_in_group(group)
        if view and os.path.basename(view.file_name() or '') == name:
            return True
    return False


roam_lines = {}


def tell_roam_cursor(view) -> None:
    """Tell the server the line of an Org file the cursor is on, each time
    it changes, while the org-roam buffer shows: the buffer follows the node
    the cursor is in (`org-lsp/cursor`)."""
    plugin = OrgLsp.current
    session = plugin.weaksession() if plugin else None
    window = view.window()
    if not session or not window or len(view.sel()) == 0 or is_roam_buffer(session, view.file_name()):
        return
    point = view.sel()[0].b
    line = view.rowcol(point)[0]
    if roam_lines.get(view.id()) == line or not roam_buffer_shown(session, window):
        return
    roam_lines[view.id()] = line
    session.send_notification(Notification('org-lsp/cursor', text_document_position_params(view, point)))


def other_group(window, path: str) -> int:
    """The group an entry's file opens in from the agenda's (the active
    one) with TAB: one that has the file, else an empty one, else a new one
    to the right of the others."""
    agenda = window.active_group()
    others = [group for group in range(window.num_groups()) if group != agenda]
    for group in others:
        if any(view.file_name() == path for view in window.views_in_group(group)):
            return group
    for group in others:
        if not window.views_in_group(group):
            return group
    # A new column on the right, the others squeezed into the left half.
    layout = window.layout()
    cols = [col / 2 for col in layout['cols']] + [1.0]
    cells = layout['cells'] + [[len(cols) - 2, 0, len(cols) - 1, len(layout['rows']) - 1]]
    window.set_layout({'cols': cols, 'rows': layout['rows'], 'cells': cells})
    return len(cells) - 1


def show_in_other_group(window, params) -> None:
    _, path = parse_uri(params['uri'])
    group = other_group(window, path)
    start = (params.get('selection') or {}).get('start') or {'line': 0, 'character': 0}
    # Follow mode shows the entry and leaves the cursor in the agenda.
    previous = window.active_view() if params.get('takeFocus') is False else None
    window.focus_group(group)
    tell_agenda_width(window)
    window.open_file('{}:{}:{}'.format(path, start['line'] + 1, start['character'] + 1),
                     sublime.ENCODED_POSITION, group=group)
    if previous:
        window.focus_view(previous)


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


# ----- images

# The most an image takes, in pixels; a larger one is shown smaller.
IMAGE_MAX_WIDTH = 800
IMAGE_MAX_HEIGHT = 400

# The previews each view shows, by view id, as Emacs keeps its overlays:
# `{key, kind, path, text, width, height, sizing}`, where `key` names a
# region that follows the edits around its text and `sizing` is how wide
# the server says it shows (`width`) or at most (`maxWidth`).
image_previews = {}
image_phantoms = {}
image_keys = [0]


def image_size(path: str):
    """The width and height of the PNG, GIF or JPEG image at `path`, which
    Sublime Text's HTML can show, from its header; None for others. Only
    the start of the file is read: a JPEG's size comes after its metadata,
    a thumbnail at most."""
    try:
        with open(path, 'rb') as image:
            data = image.read(1 << 18)
    except OSError:
        return None
    if data[:8] == b'\x89PNG\r\n\x1a\n' and len(data) >= 24:
        return struct.unpack('>II', data[16:24])
    if data[:4] == b'GIF8' and len(data) >= 10:
        return struct.unpack('<HH', data[6:10])
    if data[:2] == b'\xff\xd8':
        # The frame's header, among the segments.
        at = 2
        while at + 9 < len(data) and data[at] == 0xff:
            marker = data[at + 1]
            if 0xc0 <= marker <= 0xcf and marker not in (0xc4, 0xc8, 0xcc):
                height, width = struct.unpack('>HH', data[at + 5:at + 9])
                return width, height
            at += 2 + struct.unpack('>H', data[at + 2:at + 4])[0]
    return None


def file_url(path: str) -> str:
    return 'file://' + urllib.request.pathname2url(path)


def latex_colors(view) -> dict:
    """The colors LaTeX is drawn in: the color scheme's text, on no background."""
    style = view.style() if view and hasattr(view, 'style') else {}
    foreground = (style or {}).get('foreground')
    colors = {'background': 'Transparent'}
    if isinstance(foreground, str) and re.match(r'^#[0-9a-fA-F]{6}', foreground):
        colors['foreground'] = foreground[:7]
    return colors


def ask_startup_previews(session, uri: str) -> None:
    """The previews a file opens with: `#+STARTUP: inlineimages` (or
    `org_startup_with_link_previews`), `latexpreview`. Those of LaTeX come
    in `org-lsp/images` once Emacs has made them."""
    if settings().get('org_images', True) is False:
        return
    context = {'textDocument': {'uri': uri}, 'position': {'line': 0, 'character': 0}, 'startup': True}

    def shown(result) -> None:
        if isinstance(result, dict) and isinstance(result.get('preview'), dict):
            sublime.set_timeout(lambda: apply_preview(result['preview'], quiet=True))
    session.send_request(Request('workspace/executeCommand', {'command': 'org.link.preview', 'arguments': [context]}),
                         shown, lambda error: None)
    latex = dict(context)
    latex.update(latex_colors(view_for(uri)))
    session.send_request(Request('workspace/executeCommand', {'command': 'org.latex.preview', 'arguments': [latex]}),
                         lambda result: None, lambda error: None)


def apply_preview(preview, quiet=False) -> None:
    """What a preview command or `org-lsp/images` says: `{kind, uri, scope,
    range, remove, includeLinked, items: [{range, path}]}`. `remove` true
    takes away the previews of `kind` in `range`; false shows `items`
    there, anew; null (a link or a fragment at the cursor) toggles them.
    Says what Emacs says, unless `quiet`."""
    view = view_for((preview or {}).get('uri', ''))
    if not view:
        return
    start = position_to_offset(preview['range']['start'], view)
    end = position_to_offset(preview['range']['end'], view)
    entries = image_previews.setdefault(view.id(), [])

    def overlapping(entry) -> bool:
        regions = view.get_regions(entry['key'])
        if entry['kind'] != preview.get('kind') or not regions:
            return False
        region = regions[0]
        before = region.end() < start or (region.end() == start and region.begin() < start)
        return not before and not region.begin() >= end

    def tell(message: str) -> None:
        if not quiet:
            say(message, view)
    old = [entry for entry in entries if overlapping(entry)]
    for entry in old:
        view.erase_regions(entry['key'])
        entries.remove(entry)
    remove = preview.get('remove')
    link = preview.get('kind') == 'link'
    scope = preview.get('scope', '')
    if remove is True or (remove is None and old):
        if link:
            tell('[{}] Inline link previews turned off (removed {} images)'.format(scope, len(old)))
        elif scope == 'buffer':
            tell('LaTeX previews removed from buffer')
        elif old and remove is None:
            tell('LaTeX preview removed')
    else:
        count = 0
        for item in preview.get('items') or []:
            size = image_size(item['path'])
            if not size or not size[0] or not size[1]:
                continue
            region = sublime.Region(position_to_offset(item['range']['start'], view),
                                    position_to_offset(item['range']['end'], view))
            image_keys[0] += 1
            key = 'org-lsp-image-{}'.format(image_keys[0])
            view.add_regions(key, [region], '', '', sublime.HIDDEN)
            entries.append({'key': key, 'kind': preview['kind'], 'path': item['path'], 'text': view.substr(region),
                            'width': size[0], 'height': size[1],
                            'sizing': {'width': item.get('width'), 'maxWidth': item.get('maxWidth')}})
            count += 1
        if link:
            if count:
                with_description = ' (including images with description)' if preview.get('includeLinked') else ''
                tell('[{}] Displaying {} images inline{}'.format(scope, count, with_description))
            elif scope == 'buffer':
                tell('[{}] No images to display inline'.format(scope))
            else:
                tell("[{}] No images to display inline.  Use `C-u C-u' or 11 argument to preview the whole "
                     'buffer'.format(scope))
    draw_images(view)


def image_shown(view, entry, most):
    """The width and height in pixels the image of `entry` shows at, in a
    view at most `most` wide: as wide as the server says
    (`org-image-actual-width`), else its own width up to the most it says
    (`org-image-max-width`), never wider than `most` nor higher than
    `IMAGE_MAX_HEIGHT`, keeping its proportions."""
    sizing = entry.get('sizing') or {}

    def pixels(size):
        if not isinstance(size, dict):
            return None
        if size.get('pixels') is not None:
            return size['pixels']
        if size.get('fraction') is not None:
            return size['fraction'] * most
        if size.get('columns') is not None:
            return size['columns'] * view.em_width()
        return None

    wanted = pixels(sizing.get('width'))
    if wanted:
        scale = wanted / entry['width']
    else:
        limit = pixels(sizing.get('maxWidth'))
        scale = min(1.0, limit / entry['width']) if limit else 1.0
    scale = min(scale, most / entry['width'], IMAGE_MAX_HEIGHT / entry['height'])
    return max(1, round(entry['width'] * scale)), max(1, round(entry['height'] * scale))


def draw_images(view) -> None:
    """Show the previews of `view` as phantoms below the lines of their text;
    one whose text changed goes, as Emacs takes away an overlay then."""
    entries = image_previews.get(view.id(), [])
    # No wider than the view.
    shown = view.viewport_extent()[0] if hasattr(view, 'viewport_extent') else 0
    most = min(IMAGE_MAX_WIDTH, max(100, int(shown) - 40)) if shown else IMAGE_MAX_WIDTH
    phantoms = []
    for entry in list(entries):
        regions = view.get_regions(entry['key'])
        if not regions or view.substr(regions[0]) != entry['text']:
            view.erase_regions(entry['key'])
            entries.remove(entry)
            continue
        width, height = image_shown(view, entry, most)
        content = '<body style="margin: 0.2rem 0; padding: 0"><img src="{}" width="{}" height="{}"></body>'.format(
            html.escape(file_url(entry['path'])), width, height)
        phantoms.append(sublime.Phantom(sublime.Region(regions[0].end()), content, sublime.LAYOUT_BELOW))
    phantom_set = image_phantoms.get(view.id())
    if phantom_set is None:
        phantom_set = image_phantoms[view.id()] = sublime.PhantomSet(view, 'org-lsp-images')
    phantom_set.update(phantoms)


# The scope of each kind of line highlight: a region colour the colour
# scheme has, and the scope of the server's token of the same name.
LINE_HIGHLIGHT_SCOPES = {
    'agendaClocking': 'region.bluish markup.highlight.clocking.agenda.org',
}


def show_line_highlights(params) -> None:
    """Highlight the lines the server names from edge to edge
    (`org-lsp/lineHighlights`), as Emacs's overlays cover them: regions
    that take in the line's newline, which colour the line to the edge."""
    view = view_for((params or {}).get('uri', ''))
    if not view:
        return
    rows = view.rowcol(view.size())[0] + 1
    regions = {name: [] for name in LINE_HIGHLIGHT_SCOPES}
    for line in (params or {}).get('lines', []):
        if line.get('highlight') in regions and line.get('line', rows) < rows:
            start = view.text_point(line['line'], 0)
            end = min(view.line(start).end() + 1, view.size())
            regions[line['highlight']].append(sublime.Region(start, end))
    for name, scope in LINE_HIGHLIGHT_SCOPES.items():
        key = 'org-lsp-line-' + name
        if regions[name]:
            view.add_regions(key, regions[name], scope, '', sublime.DRAW_NO_OUTLINE)
        else:
            view.erase_regions(key)


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


TIMER_KEY = 'org-lsp-timer'
timer_state = {}


def timer_text() -> str:
    """What Emacs's mode line shows while the relative timer or a countdown
    is on (`org-timer-update-mode-line`), such as `<0:05:23>`; empty when
    none is."""
    if not timer_state:
        return ''
    seconds = timer_state['seconds']
    if not timer_state['paused']:
        passed = int(time.time() - timer_state['received'])
        seconds = max(0, seconds - passed) if timer_state['countdown'] else seconds + passed
    return '<{}:{:02d}:{:02d}>'.format(seconds // 3600, seconds % 3600 // 60, seconds % 60)


def show_timer(params) -> None:
    timer_state.clear()
    if params and isinstance(params.get('seconds'), int):
        timer_state.update(seconds=params['seconds'], countdown=params.get('countdown') is True,
                           paused=params.get('paused') is True, received=time.time())
    tick_timer(timer_state.get('received'))


def tick_timer(report) -> None:
    """Show the timer in every view's status bar, each second while it
    runs, until another report comes."""
    if timer_state.get('received') != report:
        return
    text = timer_text()
    for window in sublime.windows():
        for view in window.views():
            if text:
                view.set_status(TIMER_KEY, text)
            else:
                view.erase_status(TIMER_KEY)
    if timer_state and not timer_state['paused']:
        sublime.set_timeout(lambda: tick_timer(report), 1000)


def starter_path() -> str:
    """The file that starts the server when no Org file is open: an agenda
    file, which LSP starts the server for."""
    return os.path.join(sublime.cache_path(), PACKAGE, 'Starting.org-agenda')


def reads_folds(session, command: str) -> bool:
    """Whether the server's manifest says `command` reads the closed folds."""
    get = getattr(session, 'get_capability', None)
    commands = (get('experimental.org.commands') if get else None) or []
    return any(isinstance(entry, dict) and entry.get('command') == command and entry.get('folds')
               for entry in commands)


def closed_folds(view) -> list:
    """The folds of `view`, as their first and last lines, from 0."""
    return [[view.rowcol(region.begin())[0], view.rowcol(region.end())[0]] for region in view.folded_regions()]


def reads_clipboard(session, command: str) -> bool:
    """Whether the server's manifest says `command` reads the clipboard."""
    get = getattr(session, 'get_capability', None)
    commands = (get('experimental.org.commands') if get else None) or []
    return any(isinstance(entry, dict) and entry.get('command') == command and entry.get('clipboard')
               for entry in commands)


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

    def run(self, command: str, fallback=None, count=None) -> None:
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
            selection = view.sel()[0]
            context = text_document_position_params(view, selection.b)
            # The selection, as Emacs's region.
            if not selection.empty():
                context['range'] = {
                    'start': text_document_position_params(view, selection.begin())['position'],
                    'end': text_document_position_params(view, selection.end())['position'],
                }
            # What the command reads as Emacs's latest kill: the clipboard.
            if reads_clipboard(session, command):
                context['clipboard'] = sublime.get_clipboard()
            # The closed folds, for what is visible.
            if reads_folds(session, command):
                context['folded'] = closed_folds(view)
            # LaTeX is drawn in the colors of the text.
            if command == 'org.latex.preview':
                context.update(latex_colors(view))
            # Emacs's prefix argument, from the key's arguments.
            if isinstance(count, int):
                context['count'] = count
        else:
            context = {}
        # The org-roam buffer is hidden when it shows.
        if command == 'org.roam.buffer.toggle':
            context['visible'] = roam_buffer_shown(session, self.window)
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
            say('org-lsp: {}'.format(error.get('message', error) if isinstance(error, dict) else error), view, error=True)

    def done(self, session, view, command, context, fallback, result) -> None:
        result = result if isinstance(result, dict) else {}
        # The edit and the cursor, carried in the result (`org.return`).
        if isinstance(result.get('edits'), list):
            apply_carried(view, result)
        # What was cut or copied goes to the clipboard.
        if isinstance(result.get('copy'), str):
            sublime.set_clipboard(result['copy'])
        # A sparse tree: everything folded but the lines shown.
        if isinstance(result.get('sparse'), dict):
            show_sparse(session, view, result['sparse'])
        if isinstance(result.get('save'), list):
            save(result['save'])
        if isinstance(result.get('message'), str):
            say(result['message'], view)
        # Images of links, shown or taken away.
        if isinstance(result.get('preview'), dict):
            apply_preview(result['preview'])
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


# Colors that keyword faces are given by name most, as RGB.
COLOR_NAMES = {
    'red': (255, 0, 0), 'orange': (255, 165, 0), 'yellow': (255, 255, 0), 'gold': (255, 215, 0),
    'green': (0, 128, 0), 'forestgreen': (34, 139, 34), 'cyan': (0, 255, 255), 'blue': (0, 0, 255),
    'purple': (160, 32, 240), 'magenta': (255, 0, 255), 'violet': (238, 130, 238), 'pink': (255, 192, 203),
    'brown': (165, 42, 42), 'gray': (190, 190, 190), 'grey': (190, 190, 190), 'white': (255, 255, 255),
    'black': (0, 0, 0),
}

# The colored kinds of the quick panel, by the hue they start at.
KIND_HUES = [(15, 'REDISH'), (45, 'ORANGISH'), (70, 'YELLOWISH'), (160, 'GREENISH'), (200, 'CYANISH'),
             (250, 'BLUISH'), (290, 'PURPLISH'), (345, 'PINKISH'), (360, 'REDISH')]


def color_kind(color):
    """The name of the colored kind nearest `color` (`#rrggbb` or a name),
    or None for a color it cannot read."""
    color = (color or '').strip().lower().replace(' ', '')
    if re.match(r'^#[0-9a-f]{6}$', color):
        rgb = tuple(int(color[i:i + 2], 16) for i in (1, 3, 5))
    elif re.match(r'^#[0-9a-f]{3}$', color):
        rgb = tuple(int(c * 2, 16) for c in color[1:])
    elif color in COLOR_NAMES:
        rgb = COLOR_NAMES[color]
    else:
        return None
    hue, lightness, saturation = colorsys.rgb_to_hls(*(c / 255 for c in rgb))
    if saturation < 0.2:
        return 'LIGHT' if lightness > 0.5 else 'DARK'
    return next(name for start, name in KIND_HUES if hue * 360 < start)


def choice_kind(choice):
    """The kind of the quick panel item of `choice`: colored as its face, or
    as a TODO (red) or done (green) keyword."""
    name = color_kind((choice.get('face') or {}).get('foreground'))
    if name is None and 'highlight' in choice:
        name = 'GREENISH' if choice['highlight'].endswith('.done') else 'REDISH'
    if name is None:
        return getattr(sublime, 'KIND_AMBIGUOUS', (0, '', ''))
    return (getattr(sublime, 'KIND_ID_COLOR_' + name), key_name(choice['key']), '')


def ask(window, request, then, cancelled=None) -> None:
    """Ask what a command asks for: a key or one of a list in the quick
    panel, or text in the input panel. `cancelled` runs when it is given
    up."""
    choices = request.get('choices')
    if choices:
        if request.get('kind') == 'key':
            items = ['[{}] {}'.format(key_name(c['key']), c['label']) for c in choices]
        else:
            items = [c['label'] for c in choices]
        # TODO keywords in their colors, as `org-fast-todo-selection` shows
        # them: the quick panel colors only an item's kind, by its hue.
        if hasattr(sublime, 'QuickPanelItem') and any('highlight' in c for c in choices):
            items = [sublime.QuickPanelItem(item, kind=choice_kind(c)) for item, c in zip(items, choices)]

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
            say('org-lsp: no keys are bound here')
            return

        def picked(index: int) -> None:
            if index >= 0:
                _, _, command, args = items[index]
                target = self.window if command == 'org_lsp_run' else view
                target.run_command(command, args)
        self.window.show_quick_panel([[title, keys] for keys, title, _, _ in items], picked, placeholder='Org keys')


# ----- visibility cycling

def journal_directory(session) -> str:
    """The directory of org-journal's files, as the server says
    (`org_journal_dir`), or None when org-journal is off."""
    get = getattr(session, 'get_capability', None) if session else None
    return (get('experimental.orgJournal.directory') if get else None) or None


def is_journal_file(session, path: str) -> bool:
    """A file of org-journal's directory without an extension
    (`org-journal-file-format` is `%Y%m%d` unless set), which is Org all
    the same."""
    directory = journal_directory(session)
    if not directory or not path or not path.startswith(directory):
        return False
    return '.' not in path[len(directory):]


def show_journal_files(session) -> None:
    for window in sublime.windows():
        for view in window.views():
            show_journal_file(session, view)


def show_journal_file(session, view) -> None:
    if is_journal_file(session, view.file_name() or '') and not is_org(view):
        view.assign_syntax('Packages/{}/Org.sublime-syntax'.format(PACKAGE))


def journal_folds(params, tries: int) -> None:
    """Open, then close, the folds of the journal file the server just
    showed, once it has loaded."""
    view = view_for(params.get('uri', ''))
    if not view or view.is_loading():
        if tries > 0:
            sublime.set_timeout(lambda: journal_folds(params, tries - 1), 100)
        return
    for r in params.get('open', []):
        view.unfold(fold_region(view, r))
    for r in params.get('close', []):
        view.fold(fold_region(view, r))


# The last cycle: (view id, change count, line, state); the step after
# CHILDREN depends on the last command having been the same cycle.
last_cycle = None


SPARSE_KEY = 'org-lsp-sparse'


def show_sparse(session, view, sparse) -> None:
    """A sparse tree (`org-sparse-tree`): every fold closed but those with
    one of `sparse['lines']` inside, as `org-occur` shows its matches; the
    matches highlighted until the text changes. With `keep`
    (`org-reveal`), only the folds that start on the lines of `unfold`
    open; the others stay as they are."""
    if sparse.get('keep'):
        for line in sparse.get('unfold', []):
            start = view.line(view.text_point(line, 0)).b
            view.unfold(sublime.Region(start, start + 1))
        return
    regions = [sublime.Region(position_to_offset(r['start'], view), position_to_offset(r['end'], view))
               for r in sparse.get('highlights', [])]
    if regions:
        view.add_regions(SPARSE_KEY, regions, 'region.yellowish markup.highlight.sparse.org', '',
                         sublime.DRAW_NO_OUTLINE)
    else:
        view.erase_regions(SPARSE_KEY)
    lines = sparse.get('lines', [])

    def fold(ranges) -> None:
        view.unfold(sublime.Region(0, view.size()))
        for r in ranges or []:
            if r['endLine'] > r['startLine'] and not any(r['startLine'] < line <= r['endLine'] for line in lines):
                view.fold(fold_region(view, r))
    params = {'textDocument': text_document_identifier(view)}
    session.send_request(Request('textDocument/foldingRange', params),
                         lambda ranges: sublime.set_timeout(lambda: fold(ranges)))


def ask_startup_folds(session, uri: str) -> None:
    params = {'textDocument': {'uri': uri}}
    session.send_request(Request('org-lsp/startupFolds', params),
                         lambda result: sublime.set_timeout(lambda: fold_at_startup(session.window, uri, result)))


def fold_at_startup(window, uri: str, result) -> None:
    """Close the folds a file opens with closed, as the server says."""
    view = window.find_open_file(parse_uri(uri)[1]) if window else None
    for r in (result or {}).get('folds', []) if view else []:
        view.fold(fold_region(view, r))


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


def apply_carried(view, result) -> None:
    """Apply the edits a command's result carries, and put the cursor
    where it says."""
    cursor = result.get('cursor')

    def place(_) -> None:
        if cursor:
            point = position_to_offset(cursor, view)
            view.sel().clear()
            view.sel().add(sublime.Region(point))
    apply_text_edits(view, result['edits']).then(lambda done: sublime.set_timeout(lambda: place(done)))


ITEM = re.compile(r'^(\s*([-+]|\d+[.)])|\s+\*)(\s|$)')


def is_item(view, line) -> bool:
    """Whether line `line` starts a list item."""
    return ITEM.match(view.substr(view.line(view.text_point(line, 0)))) is not None


def children_of(outer, ranges):
    inside = [r for r in ranges if r['startLine'] > outer['startLine'] and r['endLine'] <= outer['endLine']]
    return [r for r in inside if not any(
        o is not r and o['startLine'] < r['startLine'] and r['endLine'] <= o['endLine'] for o in inside)]


def at_table(view) -> bool:
    """Whether the cursor is on a line of a table."""
    line = view.substr(view.line(view.sel()[0].b))
    return line.lstrip(' \t').startswith('|')


class OrgLspCycleCommand(LspTextCommand):
    """Org's TAB (`org-cycle`) on the fold that starts at the cursor's line:
    FOLDED, CHILDREN, SUBTREE. On a line that starts no fold, `fallback`
    runs (Tab's own), or else the innermost fold around the cursor cycles."""

    session_name = SESSION

    def run(self, edit, fallback=None) -> None:
        session = self.session_by_name(SESSION)
        if not session:
            return
        # In a table, TAB goes to the next field (`org-table-next-field`).
        if at_table(self.view):
            self.view.window().run_command('org_lsp_run', {'command': 'org.table.next.field'})
            return
        # What TAB does before it folds (`org.cycle`): a new, empty entry
        # changes level, the start of the file cycles globally.
        context = text_document_position_params(self.view, self.view.sel()[0].b)

        def failed(error) -> None:
            message = error.get('message', error) if isinstance(error, dict) else error
            say('org-lsp: {}'.format(message), self.view, error=True)
        session.send_request(
            Request('workspace/executeCommand', {'command': 'org.cycle', 'arguments': [context]}),
            lambda result: sublime.set_timeout(lambda: self.before(session, result, fallback)),
            lambda error: sublime.set_timeout(lambda: failed(error)))

    def before(self, session, result, fallback) -> None:
        result = result if isinstance(result, dict) else {}
        if isinstance(result.get('edits'), list):
            apply_carried(self.view, result)
            return
        if result.get('global') is True:
            self.view.run_command('org_lsp_cycle_global')
            return
        integrate = result.get('integrateLists') is True
        params = {'textDocument': text_document_identifier(self.view)}
        session.send_request(
            Request('textDocument/foldingRange', params),
            lambda ranges: sublime.set_timeout(lambda: self.cycle(ranges or [], fallback, integrate)))

    def cycle(self, ranges, fallback, integrate=False) -> None:
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
            # Lists stay open but with `org_cycle_include_plain_lists` `integrate`.
            if not integrate and not is_item(view, start):
                children = [child for child in children if not is_item(view, child['startLine'])]
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
        # In a table, S-TAB goes to the previous field.
        if at_table(self.view):
            self.view.window().run_command('org_lsp_run', {'command': 'org.table.previous.field'})
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


# The width last told to the server.
told_width = [0]


def agenda_view(window):
    """The agenda, where it is in front in its group."""
    for group in range(window.num_groups()):
        view = window.active_view_in_group(group)
        if view and (view.file_name() or '').endswith('.org-agenda'):
            return view
    return None


def tell_agenda_width(window) -> None:
    """Tell the server how many characters wide the agenda is, for
    `org_agenda_tags_column` `"auto"`: the tags then end at its right edge, and
    the server writes the agenda anew when the width changes."""
    plugin = OrgLsp.current
    session = plugin.weaksession() if plugin else None
    if not session or not window:
        return
    view = agenda_view(window)
    if not view or view.em_width() <= 0:
        return
    width = int(view.viewport_extent()[0] // view.em_width())
    if width > 0 and width != told_width[0]:
        told_width[0] = width
        session.send_notification(Notification('org-lsp/agendaWidth', {'width': width}))


agenda_lines = {}


def tell_agenda_cursor(view) -> None:
    """Tell the server the line of the agenda the cursor is on, each time it
    changes, for follow mode (`F`), which the server keeps; the outline
    path of its entry it answers with goes in the status bar
    (`org-agenda-show-outline-path`)."""
    plugin = OrgLsp.current
    session = plugin.weaksession() if plugin else None
    if not session or len(view.sel()) == 0:
        return
    line = view.rowcol(view.sel()[0].b)[0]
    if agenda_lines.get(view.id()) == line:
        return
    agenda_lines[view.id()] = line
    params = text_document_position_params(view, view.text_point(line, 0))
    session.send_request(Request('org-lsp/agendaCursor', params), echo_outline_path)


def echo_outline_path(result) -> None:
    if isinstance(result, dict) and isinstance(result.get('echo'), str):
        sublime.status_message(result['echo'])


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

    # Org takes the highlighting of a sparse tree away at the next change,
    # and a preview whose text changes.
    def on_modified(self, view) -> None:
        view.erase_regions(SPARSE_KEY)
        if image_previews.get(view.id()):
            draw_images(view)

    def on_close(self, view) -> None:
        image_previews.pop(view.id(), None)
        image_phantoms.pop(view.id(), None)

    def on_load(self, view) -> None:
        # The agenda is a file the server writes, to read, not to edit.
        if (view.file_name() or '').endswith('.org-agenda'):
            view.set_read_only(True)
            view.set_scratch(True)
            tell_agenda_width(view.window())
        elif OrgLsp.current:
            show_journal_file(OrgLsp.current.weaksession(), view)

    def on_selection_modified_async(self, view) -> None:
        if (view.file_name() or '').endswith('.org-agenda'):
            tell_agenda_cursor(view)
        elif (view.file_name() or '').endswith('.org'):
            tell_roam_cursor(view)

    # The agenda's width: when it comes to the front, and when the layout
    # or the side bar changes.
    def on_activated_async(self, view) -> None:
        if (view.file_name() or '').endswith('.org-agenda'):
            tell_agenda_width(view.window())

    def on_post_window_command(self, window, command, args) -> None:
        if command in ('set_layout', 'toggle_side_bar', 'toggle_minimap', 'new_pane', 'close_pane'):
            tell_agenda_width(window)
