" Org files colored by what a pattern finds, before the server's semantic
" tokens arrive and while new ones are on their way: only what the server
" colors the same way, in the groups its tokens show, so that nothing
" changes when they come (tests/nvim/syntax.lua compares the two). The
" rest waits for the server: TODO keywords and the headlines they start
" (whether one is done depends on the settings and `#+TODO:`), priorities,
" table headers, entities, LaTeX, emphasis over several lines, what options
" change. Neovim loads it through the syntax/org.vim of the plugin's root.
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
" marker that can close it within the paragraph, on this line or a later
" one (`org-emphasis-regexp-components`); without one, no emphasis. What
" emphasis holds is in its look. In a table, a cell ends them.
let s:pre = '\%(^\|[-[:space:](''"{]\)\@1<='
let s:post = '\%([-[:space:].,:!?;''")}\\[]\|$\)'
" A line that starts another element, which ends the paragraph.
let s:element = '\%(\s*$\|\*\+ \|\s*[|#:]\|\s*[-+]\%(\s\|$\)\|\s*\d\+[.)]\%(\s\|$\)\)'
for [s:marker, s:group] in [['\~', 'orgSyntax_code'], ['=', 'orgSyntax_verbatim'], ['\*', 'orgSyntax_Any_bold'], ['/', 'orgSyntax_Any_italic'], ['_', 'orgSyntax_Any_underline'], ['+', 'orgSyntax_Any_strikethrough']]
  let s:closed = '\%(\S\%(\%([^\n]\|\n' . s:element . '\@!\)\{-}\S\)\{-,1}' . s:marker . s:post . '\)\@='
  execute 'syntax region ' . s:group . ' start=&' . s:pre . s:marker . s:closed . '& end=&\S\@1<=' . s:marker . s:post . '\@=&'
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
" reads the opening line as a paragraph.
function! s:closed_by(end) abort
  return '\%(\%(\n\%(\*\+\s\)\@!.*\)\{-}\n' . a:end . '\)\@='
endfunction

let s:drawer_end = '\s*:END:\s*$'
execute 'syntax region orgSyntaxDrawer matchgroup=orgSyntax_drawer start=/^\s*:[-_[:alnum:]]\+:\s*' . s:closed_by(s:drawer_end) . '/ end=/^' . s:drawer_end . '/ keepend contains=TOP'
" The properties drawer of a headline: right after it, or after its
" planning line.
execute 'syntax region orgSyntaxProperties matchgroup=orgSyntax_drawer start=/^\s*:PROPERTIES:\s*' . s:closed_by(s:drawer_end) . '/ end=/^' . s:drawer_end . '/ keepend contained contains=orgSyntax_property'
syntax match orgSyntax_property /^\s*\zs:[^:[:space:]]\+:\ze\%(\s\|$\)/ contained

" Blocks: their opening and closing lines; the code of source, example and
" export blocks left to its language (org-lsp/src.lua, org_lsp.vim), a
" comment block's text a comment, a verse block's lines left alone, and
" the others' contents as Org.
for [s:name, s:contents] in [['src', 'raw'], ['example', 'raw'], ['export', 'raw'], ['comment', 'comment'], ['quote', 'TOP'], ['center', 'TOP'], ['verse', 'none']]
  let s:end = '\s*#+end_' . s:name . '\s*$'
  let s:group = s:contents ==# 'raw' ? 'orgSyntaxRawBlock' : s:contents ==# 'comment' ? 'orgSyntax_comment' : 'orgSyntaxBlock'
  let s:contains = s:contents =~# '^\(raw\|comment\|none\)$' ? '' : ' contains=' . s:contents
  execute 'syntax region ' . s:group . ' matchgroup=orgSyntax_block start=/\c^\s*#+begin_' . s:name . '\>.*' . s:closed_by(s:end) . '/ end=/\c^' . s:end . '/ keepend' . s:contains
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

" The stars in the color of their level, which repeat after eight
" (`org-cycle-level-faces`), up to an inline task's fifteen; the title
" too, unless it starts with what may be a TODO keyword (`COMMENT`, an
" uppercase word) or a priority, or the headline is archived. Then the
" server colors the rest of the line.
syntax match orgSyntaxHeadlineRest /\S.*/ contained nextgroup=orgSyntaxPlanning,orgSyntaxProperties skipnl
for s:level in range(1, 8)
  execute printf('syntax match orgSyntax_stars_level%d /^\%%(\*\{8}\)*\*\{%d}\ze / nextgroup=orgSyntax_headline_level%d,orgSyntaxHeadlineRest skipwhite', s:level, s:level, s:level)
  execute printf('syntax match orgSyntax_headline_level%d /\%%(\u\{2,}\%%(\s\|$\)\|\[#\|.*:ARCHIVE:\)\@!\S.*/ contained contains=orgSyntax_tag,@orgSyntaxInline nextgroup=orgSyntaxPlanning,orgSyntaxProperties skipnl', s:level)
endfor
" An inline task, which the server colors as one.
syntax match orgSyntaxInlineTask /^\*\{15,}\%( .*\)\?$/
syntax match orgSyntax_tag /\s\@1<=:[[:alnum:]_@#%:]\+:\ze\s*$/ contained

" Blocks and drawers end before the next headline: what is colored again
" from a headline on is colored as from the start.
syntax sync match orgSyntaxSync grouphere NONE /^\*\+\s/

" ----- colors

" Each group linked to the one the plugin gives the token of its selector:
" in Neovim `@lsp.typemod.stars.level1.org` for `orgSyntax_stars_level1`, in
" Vim `OrgLsp_stars_level1`.
function! s:link(group) abort
  let selector = matchstr(a:group, '^orgSyntax_\zs.\{-}\ze\%(__\|$\)')
  if !has('nvim')
    execute 'highlight default link' a:group 'OrgLsp_' . selector
    return
  endif
  let [type; modifier] = split(selector, '_')
  let target = type ==# 'Any' ? printf('@lsp.mod.%s.org', modifier[0])
        \ : empty(modifier) ? printf('@lsp.type.%s.org', type)
        \ : printf('@lsp.typemod.%s.%s.org', type, modifier[0])
  execute 'highlight default link' a:group target
endfunction

for s:line in split(execute('syntax list'), "\n")
  let s:group = matchstr(s:line, '^orgSyntax_\S\+')
  if !empty(s:group)
    call s:link(s:group)
  endif
endfor

" Which syntax for Org the buffer has: the plugins put this one in place of
" Vim's own when 'runtimepath' finds that first.
let b:org_lsp_syntax = 1
let b:current_syntax = 'org'
