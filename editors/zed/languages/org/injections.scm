; The code of a source or export block in its language:
; `#+begin_src python`, `#+begin_export html`. The language is the
; block's first parameter, as Org reads it. The grammar has no node for
; inline source blocks (`src_python{…}`), so their code is not colored.
((block
   name: (expr) @_name
   .
   parameter: (expr) @injection.language
   contents: (contents) @injection.content)
 (#match? @_name "^([sS][rR][cC]|[eE][xX][pP][oO][rR][tT])$"))
