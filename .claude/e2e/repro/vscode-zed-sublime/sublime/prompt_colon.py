"""The caption LSP-org's input panel shows for prompts the server gives as
Emacs words them (ending in ': ' or '? ')."""
import sys, types
sys.path.insert(0, '/home/user/org-lsp/editors/sublime/test')
import test_plugin as t
plugin, sublime, _ = t.load_plugin()
shown = []
window = types.SimpleNamespace(show_input_panel=lambda caption, initial, done, change, cancel: shown.append(caption),
                               show_quick_panel=lambda items, picked, placeholder='': shown.append(('quick', placeholder)))
for prompt in ['Keep how many minutes (default 90): ', 'EFFORT value [1:00]: ', 'Number of clones to produce', 'Tags :work:']:
    plugin.ask(window, {'kind': 'text', 'prompt': prompt}, lambda a: None)
for caption in shown:
    print(repr(caption))
