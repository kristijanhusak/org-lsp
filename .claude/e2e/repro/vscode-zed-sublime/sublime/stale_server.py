"""LSP-org with the downloaded server of an older release: after Package
Control updates the package (a new VERSION/RELEASE), the server of the old
release is still used and never downloaded again.
Runs the plugin with editors/sublime/test/test_plugin.py's fake sublime/LSP."""
import io, os, sys, tempfile
sys.path.insert(0, '/home/user/org-lsp/editors/sublime/test')
import test_plugin as t

plugin, sublime, _ = t.load_plugin()
empty = tempfile.mkdtemp()
os.environ['PATH'] = empty
os.environ['HOME'] = empty
asked = []

class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

def urlopen(url, timeout=None):
    asked.append(url)
    return Response(t.archive(('server of ' + url.split('/')[-2]).encode()))

plugin.urllib.request.urlopen = urlopen
storage = plugin.OrgLsp.storage_path()
print('needs install at first:', plugin.OrgLsp.needs_update_or_installation())
plugin.OrgLsp.install_or_update()
print('downloaded:', open(plugin.server_path(storage), 'rb').read())
# Package Control updates the package: its VERSION, and so RELEASE, changes.
plugin.VERSION = '0.2.0'
plugin.RELEASE = 'v' + plugin.VERSION
print('after the package update, needs update:', plugin.OrgLsp.needs_update_or_installation())
print('server started:', open(plugin.server_path(storage), 'rb').read(), '(package release', plugin.RELEASE + ')')
print('urls fetched:', asked)
