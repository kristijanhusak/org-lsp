; Org's colors in Helix, which has no semantic tokens: a query for the
; grammar Helix uses for Org (milisims/tree-sitter-org), in place of
; Helix's own. scripts/install.sh puts it in ~/.config/helix/runtime,
; which Helix reads before its own runtime; docs/helix.org lists its
; scopes.
;
; It colors what the server's semantic tokens color in the other editors
; (server/org-lsp/src/legend.rs), with the scopes of Helix's themes. A
; scope is named after what it colors (`diff.minus.todo`), and a theme
; that has only its start (`diff.minus`) colors it with that. The grammar
; parses headlines, planning lines, drawers, blocks, lists and tables,
; and leaves text as words (`expr`), so what Org finds in text (emphasis,
; links, timestamps, footnotes) is matched here word by word, within a
; line.
;
; Of two patterns that capture the same node, Helix uses the later; a
; capture that only names a node for a predicate (`@_name`) leaves it
; without a color, so the patterns that have one come before those that
; color the same node.

; Headlines, by level: Emacs's `org-level-N` faces, which go round after
; eight levels. Themes that color `markup.heading.1` to `.6` tell levels
; apart; the others color every level as `markup.heading`.
((headline (stars) @_stars) @markup.heading.1
 (#match? @_stars "^(\\*{8})*\\*$"))
((headline (stars) @_stars) @markup.heading.2
 (#match? @_stars "^(\\*{8})*\\*{2}$"))
((headline (stars) @_stars) @markup.heading.3
 (#match? @_stars "^(\\*{8})*\\*{3}$"))
((headline (stars) @_stars) @markup.heading.4
 (#match? @_stars "^(\\*{8})*\\*{4}$"))
((headline (stars) @_stars) @markup.heading.5
 (#match? @_stars "^(\\*{8})*\\*{5}$"))
((headline (stars) @_stars) @markup.heading.6
 (#match? @_stars "^(\\*{8})*\\*{6}$"))
((headline (stars) @_stars) @markup.heading.7
 (#match? @_stars "^(\\*{8})*\\*{7}$"))
((headline (stars) @_stars) @markup.heading.8
 (#match? @_stars "^\\*{8}+$"))

; A subtree tagged ARCHIVE (`org-archived`) and an inline task
; (`org-inlinetask`), shaded.
((headline tags: (tag_list (tag) @_tag)) @comment.archived
 (#eq? @_tag "ARCHIVE"))
((headline (stars) @_stars) @comment.inlinetask
 (#match? @_stars "^\\*{15,}$"))

; A done headline past its keyword: `org-headline-done`.
((headline
   item: (item . (expr) @_keyword) @comment.done)
 (#any-of? @_keyword "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED"))

; The priority, first in the title or after a keyword.
((headline
   item: (item . (expr) @_keyword . (expr) @constant.numeric.priority))
 (#match? @_keyword "^[A-Z]+$")
 (#match? @constant.numeric.priority "^\\[#[A-Z0-9]+\\]$"))
((headline
   item: (item . (expr) @constant.numeric.priority))
 (#match? @constant.numeric.priority "^\\[#[A-Z0-9]+\\]$"))

; TODO keywords. The grammar does not know which words are keywords
; (`org-todo-keywords`, `#+TODO:`), so these are the usual ones.
((headline
   item: (item . (expr) @diff.minus.todo))
 (#any-of? @diff.minus.todo
   "TODO" "NEXT" "STARTED" "WAIT" "WAITING" "HOLD" "PROJ" "SOMEDAY" "INPROGRESS"))
((headline
   item: (item . (expr) @diff.plus.done))
 (#any-of? @diff.plus.done "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED"))

; Statistics cookies in headlines and items; complete ones as done.
((item (expr "[") @constant.numeric.cookie)
 (#match? @constant.numeric.cookie "^\\[(\\d*/\\d*|\\d*%)\\]$"))
((listitem contents: (paragraph (expr "[") @constant.numeric.cookie))
 (#match? @constant.numeric.cookie "^\\[(\\d*/\\d*|\\d*%)\\]$"))
((item (expr "[") @diff.plus.cookie)
 (#match? @diff.plus.cookie "^\\[(100%|1/1|2/2|3/3|4/4|5/5|6/6|7/7|8/8|9/9|10/10|11/11|12/12|13/13|14/14|15/15|16/16|17/17|18/18|19/19|20/20)\\]$"))
((listitem contents: (paragraph (expr "[") @diff.plus.cookie))
 (#match? @diff.plus.cookie "^\\[(100%|1/1|2/2|3/3|4/4|5/5|6/6|7/7|8/8|9/9|10/10|11/11|12/12|13/13|14/14|15/15|16/16|17/17|18/18|19/19|20/20)\\]$"))

(tag_list) @tag

; Planning lines and clocks: their keywords as `org-special-keyword`,
; active timestamps as `org-date`, inactive ones shaded.
(entry
  name: (entry_name) @keyword.planning
  ":" @keyword.planning)
(timestamp) @constant.numeric.timestamp
((timestamp "[") @comment.timestamp)
((timestamp "[%%") @comment.timestamp)
((contents . (expr ":") @keyword.planning)
 (#eq? @keyword.planning "CLOCK:"))
((contents "\n" . (expr ":") @keyword.planning)
 (#eq? @keyword.planning "CLOCK:"))

; Drawers (`org-drawer`) and the names of properties.
(property_drawer [":properties:" ":end:"] @function.drawer)
(drawer [":" ":end:"] @function.drawer)
(drawer name: (expr) @function.drawer)
(property ":" @variable.other.member.property)
(property name: (expr) @variable.other.member.property)

; `#+KEYWORD:` lines (`org-meta-line`); the title and the information of
; the document.
((directive
   name: (expr) @_name
   value: (value) @markup.heading.title)
 (#match? @_name "(?i)^title$"))
((directive
   name: (expr) @_name
   value: (value) @string.info)
 (#match? @_name "(?i)^(subtitle|author|email|date)$"))
(directive ["#+" ":"] @comment.keyword)
(directive name: (expr) @comment.keyword)

(comment) @comment

; Blocks: their lines as `org-block-begin-line`, quote and verse blocks
; as quotes, comment blocks as comments, example blocks as raw text; the
; code of source blocks in its language's colors (injections.scm).
((block
   name: (expr) @_name
   contents: (contents) @markup.quote)
 (#match? @_name "(?i)^(quote|verse)$"))
((block
   name: (expr) @_name
   contents: (contents) @comment)
 (#match? @_name "(?i)^comment$"))
((block
   name: (expr) @_name
   contents: (contents) @markup.raw.block)
 (#match? @_name "(?i)^example$"))
(block ["#+begin_" "#+end_"] @comment.block)
(block name: (expr) @comment.block)
(block parameter: (expr) @comment.block)
(block end_name: (expr) @comment.block)
(dynamic_block ["#+begin:" "#+end:"] @comment.block)
(dynamic_block name: (expr) @comment.block)
(dynamic_block parameter: (expr) @comment.block)

(latex_env) @markup.raw.block

; Lists: bullets, checkboxes, and the term of a description list
; (`- term :: text`).
(listitem bullet: (bullet) @markup.list)
(checkbox) @markup.list.unchecked
((checkbox status: (expr) @_status) @markup.list.checked
 (#match? @_status "^[xX]$"))
((listitem
   contents: (paragraph
     . (expr)+ @markup.bold.term
     . (expr ":") @_separator))
 (#eq? @_separator "::"))

; Tables: their bars and rules, and the formulas' line.
(row "|" @punctuation.special.table)
(cell "|" @punctuation.special.table)
(hr) @punctuation.special.table
(formula "#+tblfm:" @comment.keyword)

; What Org finds in text. A thing of more than one word goes from the
; word that opens it to the word that closes it, on one line.

; Timestamps: active ones as in planning lines, inactive ones shaded.
((expr "<") @_open @constant.numeric.timestamp
 . (expr)* @_middle @constant.numeric.timestamp
 . (expr ">") @_close @constant.numeric.timestamp
 (#match? @_open "<\\d{4}-\\d\\d-\\d\\d[^<>]*$")
 (#not-match? @_middle "[<>]")
 (#match? @_close "^[^<>]*>"))
((expr "[") @_open @comment.timestamp
 . (expr)* @_middle @comment.timestamp
 . (expr "]") @_close @comment.timestamp
 (#match? @_open "\\[\\d{4}-\\d\\d-\\d\\d[^\\[\\]]*$")
 (#not-match? @_middle "[\\[\\]]")
 (#match? @_close "^[^\\[\\]]*\\]"))
((expr "<") @constant.numeric.timestamp
 (#match? @constant.numeric.timestamp "<\\d{4}-\\d\\d-\\d\\d[^<>]*>"))
((expr "[") @comment.timestamp
 (#match? @comment.timestamp "\\[\\d{4}-\\d\\d-\\d\\d[^\\[\\]]*\\]"))

; Links: `[[target][description]]`, `<https://…>` and plain URLs.
((expr "[") @_open @markup.link.url
 . (expr)* @_middle @markup.link.url
 . (expr "]") @_close @markup.link.url
 (#match? @_open "\\[\\[([^\\]]|\\][^\\]])*$")
 (#not-match? @_middle "\\]\\]")
 (#match? @_close "\\]\\]"))
((expr "[") @markup.link.url
 (#match? @markup.link.url "\\[\\[.*\\]\\]"))
((expr ":") @markup.link.url
 (#match? @markup.link.url "^[(<'\"]*(https?|ftp|mailto|file|doi|id|attachment):[^\\s]+$"))

; Footnotes: definitions, and references (`[fn:1]`, `[fn:name]`) in a
; word.
(fndef ["[fn:" "]"] @markup.link.url.footnote)
(fndef label: (expr) @markup.link.url.footnote)
((expr
   "[" @markup.link.url.footnote
   . "str" @_fn @markup.link.url.footnote
   . ":" @markup.link.url.footnote
   . _ @markup.link.url.footnote
   . "]" @markup.link.url.footnote)
 (#eq? @_fn "fn"))

; Targets, macros, entities and LaTeX fragments.
((expr "<") @label.target
 (#match? @label.target "<<[^<>]+>>"))
((expr "<") @_open @label.target
 . (expr)* @_middle @label.target
 . (expr ">") @_close @label.target
 (#match? @_open "^<<[^<>]+$")
 (#not-match? @_middle "[<>]")
 (#match? @_close "^[^<>]+>>"))
((expr "{") @function.macro
 (#match? @function.macro "\\{\\{\\{.*\\}\\}\\}"))
((expr "\\") @constant.character.escape.entity
 (#match? @constant.character.escape.entity "^\\\\[a-zA-Z]+[0-9]*(\\{\\})?[-.,:;!?'\")}\\]]*$"))
((expr "$") @markup.raw.inline.latex
 (#match? @markup.raw.inline.latex "^\\$[^$\\s]([^$]*[^$\\s])?\\$[-.,:;!?'\")}\\]]*$"))
((expr "\\") @_open @markup.raw.inline.latex
 . (expr)* @markup.raw.inline.latex
 . (expr "\\") @_close @markup.raw.inline.latex
 (#match? @_open "^\\\\\\(")
 (#match? @_close "\\\\\\)[-.,:;!?'\")}\\]]*$"))

; Emphasis: `*bold*`, `/italic/`, `_underline_`, `+strike-through+`,
; `=verbatim=` and `~code~`, the markers with what may stand around them
; (`org-emphasis-regexp-components`). A word in one kind of emphasis
; inside another keeps its own kind.
((expr "*") @_open @markup.bold
 . (expr)* @_middle @markup.bold
 . (expr "*") @_close @markup.bold
 (#match? @_open "^[-('\"{]*\\*[^*\\s][^*]*$")
 (#not-match? @_middle "\\*")
 (#match? @_close "^[^*]*[^*\\s]\\*[-.,:;!?'\")}\\[\\\\]*$"))
((expr "/") @_open @markup.italic
 . (expr)* @_middle @markup.italic
 . (expr "/") @_close @markup.italic
 (#match? @_open "^[-('\"{]*/[^/\\s][^/]*$")
 (#not-match? @_middle "/")
 (#match? @_close "^[^/]*[^/\\s]/[-.,:;!?'\")}\\[\\\\]*$"))
((expr "_") @_open @markup.underline
 . (expr)* @_middle @markup.underline
 . (expr "_") @_close @markup.underline
 (#match? @_open "^[-('\"{]*_[^_\\s][^_]*$")
 (#not-match? @_middle "_")
 (#match? @_close "^[^_]*[^_\\s]_[-.,:;!?'\")}\\[\\\\]*$"))
((expr "+") @_open @markup.strikethrough
 . (expr)* @_middle @markup.strikethrough
 . (expr "+") @_close @markup.strikethrough
 (#match? @_open "^[-('\"{]*\\+[^+\\s][^+]*$")
 (#not-match? @_middle "\\+")
 (#match? @_close "^[^+]*[^+\\s]\\+[-.,:;!?'\")}\\[\\\\]*$"))
((expr "=") @_open @markup.raw.inline.verbatim
 . (expr)* @_middle @markup.raw.inline.verbatim
 . (expr "=") @_close @markup.raw.inline.verbatim
 (#match? @_open "^[-('\"{]*=[^=\\s][^=]*$")
 (#not-match? @_middle "=")
 (#match? @_close "^[^=]*[^=\\s]=[-.,:;!?'\")}\\[\\\\]*$"))
((expr "~") @_open @markup.raw.inline.code
 . (expr)* @_middle @markup.raw.inline.code
 . (expr "~") @_close @markup.raw.inline.code
 (#match? @_open "^[-('\"{]*~[^~\\s][^~]*$")
 (#not-match? @_middle "~")
 (#match? @_close "^[^~]*[^~\\s]~[-.,:;!?'\")}\\[\\\\]*$"))
((expr "*") @markup.bold
 (#match? @markup.bold "^[-('\"{]*\\*[^*\\s]([^*]*[^*\\s])?\\*[-.,:;!?'\")}\\[\\\\]*$"))
((expr "/") @markup.italic
 (#match? @markup.italic "^[-('\"{]*/[^/\\s]([^/]*[^/\\s])?/[-.,:;!?'\")}\\[\\\\]*$"))
((expr "_") @markup.underline
 (#match? @markup.underline "^[-('\"{]*_[^_\\s]([^_]*[^_\\s])?_[-.,:;!?'\")}\\[\\\\]*$"))
((expr "+") @markup.strikethrough
 (#match? @markup.strikethrough "^[-('\"{]*\\+[^+\\s]([^+]*[^+\\s])?\\+[-.,:;!?'\")}\\[\\\\]*$"))
((expr "=") @markup.raw.inline.verbatim
 (#match? @markup.raw.inline.verbatim "^[-('\"{]*=[^=\\s]([^=]*[^=\\s])?=[-.,:;!?'\")}\\[\\\\]*$"))
((expr "~") @markup.raw.inline.code
 (#match? @markup.raw.inline.code "^[-('\"{]*~[^~\\s]([^~]*[^~\\s])?~[-.,:;!?'\")}\\[\\\\]*$"))

; Where Org sees no markup: a comment, an example block and a fixed-width
; line (`: text`) show as they are, Emacs's `org-comment`, `org-block`
; and `org-code`. Last, to win over the words' patterns above.
(comment (expr) @comment)
((block
   name: (expr) @_name @comment.block
   contents: (contents (expr) @markup.raw.block))
 (#match? @_name "(?i)^example$"))
; A comment line the grammar reads as text (after a `#+KEYWORD:` line).
((paragraph (directive) . (expr) @_hash @comment . (expr)* @comment . "\n")
 (#eq? @_hash "#"))
(("\n" . (expr) @_hash @comment . (expr)* @comment . "\n")
 (#eq? @_hash "#"))
; Fixed-width lines (`: text`), as verbatim.
((paragraph . (expr ":") @_colon @markup.raw.inline.fixed . (expr)* @markup.raw.inline.fixed . "\n")
 (#eq? @_colon ":"))
(("\n" . (expr ":") @_colon @markup.raw.inline.fixed . (expr)* @markup.raw.inline.fixed . "\n")
 (#eq? @_colon ":"))
