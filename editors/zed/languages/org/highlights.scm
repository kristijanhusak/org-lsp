; What Zed shows before the server's semantic tokens arrive, or without
; them ("semantic_tokens": "off"): the structure only.
(headline (stars) @punctuation.special)
(headline (item) @title)
(tag_list (tag) @tag)
(property_drawer) @comment
(property name: (expr) @property)
(drawer name: (expr) @keyword)
(directive name: (expr) @keyword)
(block name: (expr) @keyword)
(dynamic_block name: (expr) @keyword)
(timestamp) @string.special
(plan) @keyword
(comment) @comment
(listitem bullet: (bullet) @punctuation.list_marker)
(checkbox) @punctuation.list_marker
(fndef label: (expr) @link_uri)
