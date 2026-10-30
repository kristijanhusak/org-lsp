" Org files colored by what a pattern finds, before the server's semantic
" tokens arrive and while new ones are on their way: only what the server
" colors the same way, in the groups its tokens show, so that nothing
" changes when they come (tests/nvim/syntax.lua compares the two). The
" rest waits for the server: priorities, table headers, entities, LaTeX,
" emphasis over several lines, what options change. The TODO keywords and
" how headlines look are what the server tells the plugins
" (`org-lsp/syntax`), which load the syntax again when it changes. Neovim
" loads it through the syntax/org.vim of the plugin's root.
"
" A group `orgSyntax_TYPE_MODIFIER` shows the token `TYPE.MODIFIER`
" (`orgSyntax_Any_bold` is `*.bold`), and what follows `__` in a name only
" tells apart patterns of one group (`orgSyntax_code__cell`).
if exists('b:current_syntax')
  finish
endif

syntax case match

" ----- objects

" Emphasis, code and verbatim: PRE MARKER CONTENTS MARKER POST, the
" contents neither starting nor ending with a blank, closed by the first
" marker that can close it on this line or the next one of the paragraph
" (`org-emphasis-regexp-components`: one newline at most), within
" `s:reach` characters of each line; without one, no emphasis, and the
" server's tokens show what is longer. Looking no further keeps a long
" paragraph or line with markers that close nothing (`/usr/bin`, `*nix`)
" from making each redraw scan the rest of it. What emphasis holds is in
" its look. In a table, a cell ends them.
let s:pre = '\%(^\|[-[:space:](''"{]\)\@1<='
let s:post = '\%([-[:space:].,:!?;''")}\\[]\|$\)'
" A line that starts another element, which ends the paragraph.
let s:element = '\%(\s*$\|\*\+ \|\s*[|#:]\|\s*[-+]\%(\s\|$\)\|\s*\d\+[.)]\%(\s\|$\)\)'
let s:reach = '\{-,1000}'
for [s:marker, s:group] in [['\~', 'orgSyntax_code'], ['=', 'orgSyntax_verbatim'], ['\*', 'orgSyntax_Any_bold'], ['/', 'orgSyntax_Any_italic'], ['_', 'orgSyntax_Any_underline'], ['+', 'orgSyntax_Any_strikethrough']]
  let s:closed = '\%(\S\%([^\n]' . s:reach . '\%(\n' . s:element . '\@![^\n]' . s:reach . '\)\=\S\)\{-,1}' . s:marker . s:post . '\)\@='
  execute 'syntax region ' . s:group . ' start=&\%#=1' . s:pre . s:marker . s:closed . '& end=&\S\@1<=' . s:marker . s:post . '\@=&'
  execute 'syntax match ' . s:group . '__cell #' . s:pre . s:marker . '[^|[:space:]]\%([^|]\{-}[^|[:space:]]\)\{-,1}' . s:marker . '\ze' . s:post . '# contained'
endfor

" Timestamps, and ranges of two (`<…>--<…>`), active or inactive as the
" first one is.
let s:date = '\d\{4}-\d\d-\d\d\%( [^]>\n]\{-}\)\?'
let s:range = '\%(--[<[]' . s:date . '[]>]\)\?'
execute 'syntax match orgSyntax_timestamp /<' . s:date . '>' . s:range . '/'
execute 'syntax match orgSyntax_timestamp_inactive /\[' . s:date . '\]' . s:range . '/'

" Links: `[[target]]`, its brackets and target in the colors the server
" gives them, and `[[target][description]]`, whose description may hold
" brackets.
syntax region orgSyntax_linkUrl matchgroup=orgSyntax_link start=/\[\[\ze[^][]\+\]\]/ end=/\]\]/ oneline
syntax region orgSyntax_linkDescription matchgroup=orgSyntax_link start=/\[\[[^][]\+\]\[\ze.\{-1,}\]\]/ end=/\]\]/ oneline
" A plain link (`org-link-plain-re`) where a word starts, which ends in
" neither punctuation nor a blank, but may end in a slash.
syntax match orgSyntax_linkUrl /[$%']\@1<!\<https\?:[^][[:space:]()<>]*\%([^][[:space:]()<>[:punct:]]\|\/\)/

" Footnote references, and inline definitions with brackets balanced in
" them.
syntax match orgSyntax_footnote /\[fn:[-_[:alnum:]]\+\]/
syntax match orgSyntax_footnote /\[fn:[-_[:alnum:]]*:\%([^][]\|\[\%([^][]\|\[[^][]*\]\)*\]\)*\]/
syntax match orgSyntax_macro /{{{[[:alpha:]][-_[:alnum:]]*\%((.\{-})\)\?}}}/
syntax match orgSyntax_target /<\@1<!<<[^<>[:space:]]\%([^<>]*[^<>[:space:]]\)\?>>>\@!/

" Statistics cookies, complete ones in the color of done.
syntax match orgSyntax_cookie /\[\d*\/\d*\]\|\[\d*%\]/
syntax match orgSyntax_cookie_done /\[\(\d\+\)\/\1\]\|\[100%\]/

" LaTeX fragments, which the server colors: nothing in them is Org's.
syntax match orgSyntaxLatex /\\(.\{-}\\)\|\\\[.\{-}\\\]\|\$\$.\{-}\$\$/
syntax match orgSyntaxLatex /\$\@1<!\$[^[:space:].,;$]\%([^$]\{-}[^[:space:].,$]\)\{-,1}\$\%([-[:space:].,?;:'")]\|$\)\@=/

syntax cluster orgSyntaxInline contains=orgSyntax_timestamp,orgSyntax_timestamp_inactive,orgSyntax_linkUrl,orgSyntax_linkDescription,orgSyntax_code,orgSyntax_verbatim,orgSyntax_Any_bold,orgSyntax_Any_italic,orgSyntax_Any_underline,orgSyntax_Any_strikethrough,orgSyntax_footnote,orgSyntax_macro,orgSyntax_target,orgSyntax_cookie,orgSyntax_cookie_done,orgSyntaxLatex
" What a table cell holds: no statistics cookies.
syntax cluster orgSyntaxCell contains=orgSyntax_timestamp,orgSyntax_timestamp_inactive,orgSyntax_linkUrl,orgSyntax_linkDescription,orgSyntax_code__cell,orgSyntax_verbatim__cell,orgSyntax_Any_bold__cell,orgSyntax_Any_italic__cell,orgSyntax_Any_underline__cell,orgSyntax_Any_strikethrough__cell,orgSyntax_footnote,orgSyntax_macro,orgSyntax_target,orgSyntaxLatex

" ----- lists, tables, fixed width

syntax match orgSyntax_bullet /^\s*\zs[-+]\ze\%(\s\|$\)/ nextgroup=orgSyntaxCheckbox,orgSyntax_descriptionTerm skipwhite
syntax match orgSyntax_bullet /^\s\+\zs\*\ze\%(\s\|$\)/ nextgroup=orgSyntaxCheckbox,orgSyntax_descriptionTerm skipwhite
" Ordered items have no description term.
syntax match orgSyntax_bullet /^\s*\zs\d\+[.)]\ze\%(\s\|$\)/ nextgroup=orgSyntax_checkbox,orgSyntax_checkbox_checked,orgSyntax_checkbox_partial skipwhite
syntax match orgSyntax_checkbox /\[ \]\ze\%(\s\|$\)/ contained
syntax match orgSyntax_checkbox_checked /\[X\]\ze\%(\s\|$\)/ contained
syntax match orgSyntax_checkbox_partial /\[-\]\ze\%(\s\|$\)/ contained
syntax match orgSyntaxCheckbox /\[[ X-]\]\ze\%(\s\|$\)/ contained contains=orgSyntax_checkbox,orgSyntax_checkbox_checked,orgSyntax_checkbox_partial nextgroup=orgSyntax_descriptionTerm skipwhite
" A description list's term: up to the first `::` with a blank before it
" and a blank or the end of the line after it, as Org's rule finds it.
syntax match orgSyntax_descriptionTerm /\%(\[[ X-]\]\s\)\@!\S.\{-}\s\+::\ze\%(\s\|$\)/ contained contains=@orgSyntaxInline

syntax match orgSyntaxTable /^\s*|.*/ contains=orgSyntax_tableDelimiter,@orgSyntaxCell
syntax match orgSyntax_tableDelimiter /|/ contained
syntax match orgSyntax_tableDelimiter /^\s*\zs|-.*/
" A table.el table, which the server leaves alone.
syntax match orgSyntaxTableEl /^\s*+-.*/

syntax match orgSyntax_verbatim /^\s*\zs:\%(\s.*\)\?$/

" ----- planning, clocks

" The line after a headline that starts with a planning keyword.
syntax match orgSyntaxPlanning /\s*\%(SCHEDULED\|DEADLINE\|CLOSED\):.*/ contained contains=orgSyntax_planning,orgSyntax_timestamp,orgSyntax_timestamp_inactive nextgroup=orgSyntaxProperties skipnl
" A clock line: an inactive timestamp or range and its duration, alone.
execute 'syntax match orgSyntaxClock /^\s*CLOCK: \+\[' . s:date . '\]\%(--\[' . s:date . '\] \+=> \+\d\+:\d\d\)\?\s*$/ contains=orgSyntax_planning,orgSyntax_timestamp_inactive'
syntax match orgSyntax_planning /\<\%(SCHEDULED\|DEADLINE\|CLOSED\|CLOCK\):/ contained

" ----- drawers and blocks

" What has its closing line before the next headline: without one, Org
" reads the opening line as a paragraph. Only the next `lines` lines are
" looked at, so that lines opening what nothing closes do not each scan
" the rest of the file; the server's tokens show a longer one.
function! s:closed_by(end, lines) abort
  return '\%(\%(\n\%(\*\+\s\)\@!.*\)\{-,' . a:lines . '}\n' . a:end . '\)\@='
endfunction

let s:drawer_end = '\s*:END:\s*$'
execute 'syntax region orgSyntaxDrawer matchgroup=orgSyntax_drawer start=/\%#=1^\s*:[-_[:alnum:]]\+:\s*' . s:closed_by(s:drawer_end, 200) . '/ end=/^' . s:drawer_end . '/ keepend contains=TOP'
" The properties drawer of a headline: right after it, or after its
" planning line.
execute 'syntax region orgSyntaxProperties matchgroup=orgSyntax_drawer start=/\%#=1^\s*:PROPERTIES:\s*' . s:closed_by(s:drawer_end, 200) . '/ end=/^' . s:drawer_end . '/ keepend contained contains=orgSyntax_property'
syntax match orgSyntax_property /^\s*\zs:[^:[:space:]]\+:\ze\%(\s\|$\)/ contained

" Blocks: their opening and closing lines; the code of source, example and
" export blocks left to its language (org-lsp/src.lua, org_lsp.vim), a
" comment block's text a comment, a verse block's lines left alone, and
" the others' contents as Org.
for [s:name, s:contents] in [['src', 'raw'], ['example', 'raw'], ['export', 'raw'], ['comment', 'comment'], ['quote', 'TOP'], ['center', 'TOP'], ['verse', 'none']]
  let s:end = '\s*#+end_' . s:name . '\s*$'
  let s:group = s:contents ==# 'raw' ? 'orgSyntaxRawBlock' : s:contents ==# 'comment' ? 'orgSyntax_comment' : 'orgSyntaxBlock'
  let s:contains = s:contents =~# '^\(raw\|comment\|none\)$' ? '' : ' contains=' . s:contents
  execute 'syntax region ' . s:group . ' matchgroup=orgSyntax_block start=/\%#=1\c^\s*#+begin_' . s:name . '\>.*' . s:closed_by(s:end, 500) . '/ end=/\c^' . s:end . '/ keepend' . s:contains
endfor

" ----- keywords and comments

" A `#+KEY: value` line: its key, and the values of `#+TITLE:` and the
" document's information. Other lines that start with `#+` are left alone.
syntax match orgSyntaxKeywordLine /\c^\s*#+\%(\%(begin\|end\)[_:]\)\@!.*/ contains=orgSyntax_keyword
syntax match orgSyntax_keyword /^\s*\zs#+[[:alnum:]_-]\+:/ contained nextgroup=orgSyntax_documentTitle,orgSyntax_documentInfo skipwhite
syntax match orgSyntax_documentTitle /\%(#+[tT][iI][tT][lL][eE]:\s*\)\@<=\S.*/ contained
syntax match orgSyntax_documentInfo /\%(#+\%([sS][uU][bB][tT][iI][tT][lL][eE]\|[aA][uU][tT][hH][oO][rR]\|[eE][mM][aA][iI][lL]\|[dD][aA][tT][eE]\):\s*\)\@<=\S.*/ contained
syntax match orgSyntax_comment /^\s*\zs#\%(\s.*\)\?$/

" ----- headlines

" What the server said of the buffer's TODO keywords and headlines
" (`org-lsp/syntax`), kept by the plugins in `b:org_lsp_syntax_info`.
" Before it has, the keywords of the buffer's `#+TODO:` lines, and the
" rest as the last file was told (`g:org_lsp_syntax_info`).
function! s:keywords_of(lines) abort
  let [todo, done] = [[], []]
  for line in a:lines
    let words = split(matchstr(line, '\c^\s*#+\%(seq_\|typ_\)\=todo:\zs.*'))
    if empty(words)
      continue
    endif
    " Without `|`, the last keyword of a sequence is the done one.
    let bar = index(words, '|')
    let [now, later] = bar < 0 ? [words[:-2], words[-1:]] : [words[:bar - 1], words[bar + 1:]]
    call extend(todo, map(now, {_, w -> substitute(w, '(.*', '', '')}))
    call extend(done, map(later, {_, w -> substitute(w, '(.*', '', '')}))
  endfor
  return empty(todo) && empty(done) ? {} : {'todo': todo, 'done': done}
endfunction

let s:info = get(b:, 'org_lsp_syntax_info', {})
if empty(s:info)
  let s:info = extend(copy(get(g:, 'org_lsp_syntax_info', {})), s:keywords_of(getline(1, '$')))
endif
let s:todo = get(s:info, 'todo', ['TODO'])
let s:todo = type(s:todo) == v:t_list ? filter(copy(s:todo), '!empty(v:val)') : []
let s:done = get(s:info, 'done', ['DONE'])
let s:done = type(s:done) == v:t_list ? filter(copy(s:done), '!empty(v:val)') : []
let s:faces = get(s:info, 'faces', {})
let s:faces = type(s:faces) == v:t_dict ? s:faces : {}
let s:todo_faces = type(get(s:faces, 'todo')) == v:t_dict ? s:faces.todo : {}
let s:tag_faces = type(get(s:faces, 'tag')) == v:t_dict ? s:faces.tag : {}

" The groups of keywords and tags with a face of their own, which look as
" the server's token with that face over it: `[selector, face]` by group.
let s:faced = {}
function! s:faced_group(group, selector, kind, name) abort
  let group = a:group . '__face_' . a:kind . '_' . substitute(a:name, '\W', '_', 'g')
  let s:faced[group] = [a:selector, a:kind ==# 'todo' ? s:todo_faces[a:name] : s:tag_faces[a:name]]
  return group
endfunction

" A word of `words` as a pattern, as written.
function! s:any(words) abort
  return '\%(' . join(map(copy(a:words), {_, w -> escape(w, '\/.*$^~[]')}), '\|') . '\)'
endfunction

" Which of the level faces the stars of a headline take
" (`org-get-level-face`): that of its level, reduced with
" `org-odd-levels-only`, past `org-n-level-faces` the first again
" (`org-cycle-level-faces`) or the last.
let s:odd = get(s:info, 'oddLevels', v:false) is v:true
let s:faces_n = max([1, min([8, get(s:info, 'levelFaces', 8)])])
let s:cycle = get(s:info, 'cycleLevelFaces', v:true) isnot v:false
function! s:level_face(stars) abort
  let level = max([1, s:odd ? 1 + a:stars / 2 : a:stars])
  return s:cycle ? (level - 1) % s:faces_n + 1 : min([level, s:faces_n])
endfunction

" The stars in the color of their level, as many patterns as it takes to
" tell the levels apart; then a TODO keyword, a priority and the title.
" Past a keyword done, the title is `org-headline-done`
" (`org-fontify-done-headline`), past one not done `org-headline-todo`
" (`org-fontify-todo-headline`), or else the color of the level. What the
" server colors otherwise is left to it: priorities, `COMMENT`, archived
" headlines and inline tasks.
let s:period = s:odd ? 2 * s:faces_n : s:faces_n
let s:stars = []
if s:cycle
  for s:count in range(1, s:period)
    call add(s:stars, [printf('\%%(\*\{%d}\)*\*\{%d}', s:period, s:count), s:level_face(s:count)])
  endfor
else
  let s:last = max([1, s:odd ? 2 * s:faces_n - 2 : s:faces_n])
  for s:count in range(1, s:last - 1)
    call add(s:stars, [printf('\*\{%d}', s:count), s:level_face(s:count)])
  endfor
  call add(s:stars, [printf('\*\{%d,}', s:last), s:faces_n])
endif

let s:keywords = s:any(s:todo + s:done + ['COMMENT'])
let s:not_archived = '\%(.*:ARCHIVE:\)\@!'
let s:after = 'nextgroup=orgSyntaxPlanning,orgSyntaxProperties skipnl'
let s:todo_title = get(s:info, 'todoHeadline', v:false) is v:true ? 'orgSyntax_Any_todo' : ''
let s:done_title = get(s:info, 'doneHeadline', v:true) isnot v:false ? 'orgSyntax_Any_done' : ''

syntax match orgSyntaxHeadlineRest /\S.*/ contained nextgroup=orgSyntaxPlanning,orgSyntaxProperties skipnl
for s:level in range(1, s:faces_n)
  let s:title = 'orgSyntax_headline_level' . s:level
  " The keywords, each with a face of its own apart.
  let s:keyword_groups = []
  for [s:words, s:selector, s:state] in [[s:todo, 'todoKeyword', 't'], [s:done, 'todoKeyword_done', 'd']]
    let s:group = 'orgSyntax_' . s:selector . '__l' . s:level
    let s:then = printf('nextgroup=orgSyntaxHeadlineRest,orgSyntaxPriority__%s%d,%s skipwhite', s:state, s:level,
          \ s:state ==# 'd' && !empty(s:done_title) ? s:done_title : s:state ==# 't' && !empty(s:todo_title) ? s:todo_title : s:title . '__after')
    let s:plain = filter(copy(s:words), '!has_key(s:todo_faces, v:val)')
    if !empty(s:plain)
      execute printf('syntax match %s /%s%s\ze\%%( \|$\)/ contained %s', s:group, s:not_archived, s:any(s:plain), s:then)
      call add(s:keyword_groups, s:group)
    endif
    for s:word in filter(copy(s:words), 'has_key(s:todo_faces, v:val)')
      let s:faced_name = s:faced_group(s:group, s:selector, 'todo', s:word)
      execute printf('syntax match %s /%s%s\ze\%%( \|$\)/ contained %s', s:faced_name, s:not_archived, s:any([s:word]), s:then)
      call add(s:keyword_groups, s:faced_name)
    endfor
    execute printf('syntax match orgSyntaxPriority__%s%d /\[#\w\+\]/ contained %s', s:state, s:level, substitute(s:then, 'orgSyntaxPriority__\w\+,', '', ''))
  endfor
  execute printf('syntax match orgSyntaxPriority__p%d /%s\[#\w\+\]/ contained nextgroup=orgSyntaxHeadlineRest,%s__after skipwhite', s:level, s:not_archived, s:title)
  " The title right after the stars, and after a keyword or priority.
  execute printf('syntax match %s /\%%(%s\%%( \|$\)\|\[#\|.*:ARCHIVE:\)\@!\S.*/ contained contains=orgSyntax_tag,@orgSyntaxInline %s', s:title, s:keywords, s:after)
  execute printf('syntax match %s__after /\%%(COMMENT\%%(\s\|$\)\|\[#\)\@!\S.*/ contained contains=orgSyntax_tag,@orgSyntaxInline %s', s:title, s:after)
  for [s:pattern, s:face] in filter(copy(s:stars), 'v:val[1] == s:level')
    execute printf('syntax match orgSyntax_stars_level%d /^%s\ze / nextgroup=orgSyntaxHeadlineRest,%s,orgSyntaxPriority__p%d,%s skipwhite',
          \ s:level, s:pattern, s:title, s:level, join(s:keyword_groups, ','))
  endfor
endfor
" A title past a keyword in the look of the keyword's state: what it holds
" takes that look over its own, as the tokens do, but tags with a face of
" their own.
for s:state_title in filter([s:todo_title, s:done_title], '!empty(v:val)')
  execute printf('syntax match %s /\%%(COMMENT\%%(\s\|$\)\|\[#\)\@!\S.*/ contained contains=%s__tag %s', s:state_title, s:state_title, s:after)
  execute printf('syntax match %s__tag /\s\@1<=:[[:alnum:]_@#%%:]\+:\ze\s*$/ contained', s:state_title)
endfor
" An inline task, which the server colors as one.
if get(s:info, 'inlinetask', 15) isnot v:null
  execute printf('syntax match orgSyntaxInlineTask /^\*\{%d,}\%%( .*\)\?$/', get(s:info, 'inlinetask', 15))
endif
syntax match orgSyntax_tag /\s\@1<=:[[:alnum:]_@#%:]\+:\ze\s*$/ contained
" `org-tag-faces`: each tag with a face of its own in it.
for s:tag in keys(s:tag_faces)
  for s:tags in ['orgSyntax_tag'] + map(filter([s:todo_title, s:done_title], '!empty(v:val)'), 'v:val . "__tag"')
    let s:selector = matchstr(s:tags, '^orgSyntax_\zs.\{-}\ze\%(__\|$\)')
    execute printf('syntax match %s /:\@1<=%s\ze:/ contained containedin=%s', s:faced_group(s:tags, s:selector, 'tag', s:tag), s:any([s:tag]), s:tags)
  endfor
endfor

" Blocks and drawers end before the next headline: what is colored again
" from a headline on is colored as from the start.
syntax sync match orgSyntaxSync grouphere NONE /^\*\+\s/

" ----- colors

" Each group linked to the one the plugin gives the token of its selector:
" in Neovim `@lsp.typemod.stars.level1.org` for `orgSyntax_stars_level1`, in
" Vim `OrgLsp_stars_level1`.
function! s:target(selector) abort
  if !has('nvim')
    return 'OrgLsp_' . a:selector
  endif
  let [type; modifier] = split(a:selector, '_')
  return type ==# 'Any' ? printf('@lsp.mod.%s.org', modifier[0])
        \ : empty(modifier) ? printf('@lsp.type.%s.org', type)
        \ : printf('@lsp.typemod.%s.%s.org', type, modifier[0])
endfunction

function! s:link(group) abort
  let selector = matchstr(a:group, '^orgSyntax_\zs.\{-}\ze\%(__\|$\)')
  execute 'highlight default link' a:group s:target(selector)
endfunction

" A group of a keyword or tag with a face of its own: its token's look
" with the face over it, as the plugins put the face over the token, the
" face's colors in place of the look's and its attributes added.
function! s:merge(group, selector, face) abort
  if has('nvim')
    let look = nvim_get_hl(0, {'name': s:target(a:selector), 'link': v:false})
    for [key, attribute] in [['foreground', 'fg'], ['background', 'bg']]
      if has_key(a:face, key)
        let look[attribute] = a:face[key]
      endif
    endfor
    for attribute in ['bold', 'italic', 'underline', 'strikethrough']
      if get(a:face, attribute, v:false) is v:true
        let look[attribute] = v:true
      endif
    endfor
    silent! call nvim_set_hl(0, a:group, look)
    return
  endif
  let look = get(exists('*hlget') ? hlget(s:target(a:selector), v:true) : [], 0, {})
  call filter(look, 'index(["id", "name", "linksto", "default", "cleared"], v:key) < 0')
  let look.name = a:group
  for [key, gui, cterm] in [['foreground', 'guifg', 'ctermfg'], ['background', 'guibg', 'ctermbg']]
    if has_key(a:face, key)
      let look[gui] = a:face[key]
      " A color name the terminal knows, without 'termguicolors'.
      if a:face[key] =~# '^\a\+$'
        let look[cterm] = a:face[key]
      endif
    endif
  endfor
  for attribute in ['bold', 'italic', 'underline', 'strikethrough']
    if get(a:face, attribute, v:false) is v:true
      for mode in ['gui', 'cterm', 'term']
        let look[mode] = extend(get(look, mode, {}), {attribute: v:true})
      endfor
    endif
  endfor
  silent! call hlset([look])
endfunction

" In Vim the groups of the tokens' looks are the plugin's, made from the
" server's manifest: made now if they are not yet, for the syntax to color
" before the server has started (ALE starts it late).
if !has('nvim') && exists('g:loaded_org_lsp') && !hlexists('OrgLsp_stars_level1')
  silent! call org_lsp#define_highlights()
endif

for s:line in split(execute('syntax list'), "\n")
  let s:group = matchstr(s:line, '^orgSyntax_\S\+')
  if has_key(s:faced, s:group)
    call s:merge(s:group, s:faced[s:group][0], s:faced[s:group][1])
  elseif !empty(s:group)
    call s:link(s:group)
  endif
endfor

" Which syntax for Org the buffer has: the plugins put this one in place of
" Vim's own when 'runtimepath' finds that first.
let b:org_lsp_syntax = 1
" How many times it was loaded: what was made in the syntax before is gone.
let b:org_lsp_syntax_loads = get(b:, 'org_lsp_syntax_loads', 0) + 1
let b:current_syntax = 'org'
