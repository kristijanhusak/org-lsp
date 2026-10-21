vim9script
# yegappan/lsp's ApplyTextEdits on an empty, unloaded file.
import autoload 'lsp/textedit.vim' as te
var nr = bufadd('/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/w1/empty.org')
te.ApplyTextEdits(nr, [{range: {start: {line: 0, character: 0}, end: {line: 0, character: 0}}, newText: "* TODO X\n"}])
echomsg 'TE ' .. string(getbufline(nr, 1, '$')) .. ' mod=' .. getbufvar(nr, '&mod')
var nr2 = bufadd('/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/w1/empty2.org')
te.ApplyTextEdits(nr2, [{range: {start: {line: 0, character: 0}, end: {line: 0, character: 0}}, newText: "* TODO X\n"}])
echomsg 'TE2 ' .. string(getbufline(nr2, 1, '$')) .. ' mod=' .. getbufvar(nr2, '&mod')
