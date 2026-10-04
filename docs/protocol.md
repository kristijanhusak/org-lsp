# Protocol

org-lsp uses standard LSP only. This page lists what it offers and the
names an editor integration needs: the semantic token legend, the code
action kinds and the initialization options.

## Capabilities

| Request | Notes |
| --- | --- |
| `textDocument/didOpen`, `didChange`, `didClose` | Incremental sync. |
| `textDocument/semanticTokens/full`, `/range` | See the legend below. No deltas. |
| `textDocument/foldingRange` | Subtrees, drawers, blocks, LaTeX environments and multi-line list items. Whole lines only. |
| `textDocument/documentSymbol` | The outline of headlines. Hierarchical when the client supports it, flat otherwise. |
| `textDocument/codeAction` | Org commands; see below. |

**Position encoding.** The server picks UTF-8 if the client offers it, then
UTF-32, and falls back to UTF-16.

**Line endings.** `\n`, `\r\n` and a lone `\r` all end a line for positions.

## Initialization options

Passed as `initializationOptions`. Unknown or malformed options are ignored.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `todoKeywords` | `string[]` | `["TODO", "|", "DONE"]` | The TODO sequence, written like Org's `#+TODO:` line. `|` separates the states that count as done; without it the last keyword is the only done state. Fast-access suffixes such as `TODO(t)` are accepted and dropped. |

A document that has `#+TODO:`, `#+SEQ_TODO:` or `#+TYP_TODO:` lines uses
those instead.

## Semantic tokens

Tokens never overlap and never span lines. Nested markup is flattened into
modifiers: in `*bold /both/*` the inner text carries `bold` and `italic`.
Emphasis markers are part of the token they delimit.

### Token types

| Type | Covers |
| --- | --- |
| `headline` | Headline title text. Carries a `levelN` modifier. |
| `stars` | The stars of a headline. Carries a `levelN` modifier. |
| `todoKeyword` | A TODO keyword; `done` when it is a done state. |
| `priority` | A priority cookie, `[#A]`. |
| `tag` | The tags of a headline, `:a:b:`, as one token. |
| `planning` | `SCHEDULED:`, `DEADLINE:`, `CLOSED:` and `CLOCK:`. |
| `timestamp` | A timestamp or timestamp range; `inactive` for `[…]`. |
| `drawer` | The `:NAME:` and `:END:` lines of a drawer. |
| `property` | The `:KEY:` of a property. |
| `keyword` | The `#+KEY:` of a keyword line. |
| `block` | The `#+BEGIN…` line, with its parameters, and the `#+END…` line. |
| `comment` | Comment lines and the contents of comment blocks. |
| `bullet` | A list bullet or counter. |
| `checkbox` | A checkbox; `checked` or `partial`. |
| `link` | A link of any form, and citations. |
| `code` | `~code~` and inline source blocks. |
| `verbatim` | `=verbatim=` and fixed-width lines. |
| `text` | Plain text that carries emphasis modifiers. |
| `tableDelimiter` | Table borders and rule lines. |
| `footnote` | Footnote references and definition labels. |
| `cookie` | A statistics cookie, `[1/3]`. |
| `target` | `<<target>>` and `<<<radio target>>>`. |
| `latex` | LaTeX fragments and environments. |
| `entity` | An entity such as `\alpha`. |
| `macro` | `{{{macro}}}`. |

`property`, `keyword`, `comment` and `macro` are standard LSP token types;
the rest are specific to this server. Plain text gets no token, and neither
do the contents of source and example blocks.

### Token modifiers

| Modifier | On |
| --- | --- |
| `level1` … `level8` | `headline` and `stars`. Levels above eight wrap around. |
| `done` | `todoKeyword` |
| `inactive` | `timestamp` |
| `checked`, `partial` | `checkbox` |
| `bold`, `italic`, `underline`, `strikethrough` | `text` and `headline` |

## Code actions

Org commands are code actions with stable kinds. To bind a key, request
code actions with `context.only` set to the kind and apply the single
result. `only` matches hierarchically, so `org` selects every action.

| Kind | Effect |
| --- | --- |
| `org.todo.next` | Move the entry's TODO keyword one state forward. |
| `org.todo.previous` | Move it one state backward. |

The entry is the innermost headline whose subtree contains the start of the
requested range, so the cursor may be anywhere in the entry's body. The
cycle runs through "no keyword" between the last and first states. Before
the first headline there are no actions.

Each action carries its edit. The edit is versioned (`documentChanges`) when
the client supports that, and a plain `changes` map otherwise.

The kinds sit under their own `org` root rather than under `source` or
`refactor`. `source` actions are meant for whole files, and editors hide
them from the cursor menu.

## Writing an editor integration

An integration should stay free of Org logic. Its whole job:

1. Start `org-lsp` for `org` files.
2. Map the token types and modifiers above to the editor's highlight groups.
3. Bind keys to code action kinds.
4. Open and close folds; the server only says where they are.

`lua/org-lsp/` and `editors/vscode/` are the two existing ones.
