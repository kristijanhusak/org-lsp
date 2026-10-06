; A source block's code in its language: `#+begin_src python`. The
; language is the block's first parameter, as Org reads it.
((block
   name: (expr) @_name
   .
   parameter: (expr) @injection.language
   contents: (contents) @injection.content)
 (#match? @_name "^[sS][rR][cC]$"))
