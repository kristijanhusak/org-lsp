# Protocol

org-lsp uses standard LSP. This page says what it offers and lists the
names an editor integration deals with: options, commands, the manifest,
the agenda document and the semantic tokens.

## Capabilities

| Request | Notes |
| --- | --- |
| `textDocument/didOpen`, `didChange`, `didSave`, `didClose` | Incremental sync. A save rebuilds an open agenda. |
| `textDocument/semanticTokens/full`, `/range` | For Org files and the agenda. See [Semantic tokens](#semantic-tokens). No deltas. |
| `textDocument/foldingRange` | Subtrees, drawers, blocks, LaTeX environments and multi-line list items. Whole lines only. A subtree takes the blank lines before the next headline along, as Emacs hides them; see `cycleSeparatorLines`. |
| `textDocument/documentSymbol` | The outline of headlines. Hierarchical when the client supports it, flat otherwise. |
| `textDocument/codeAction` | The commands that apply at the cursor. See [Commands](#commands). |
| `workspace/executeCommand` | Every command. See [Commands](#commands). |
| `textDocument/definition` | In the agenda: from a line to its entry. |
| `workspace/textDocumentContent` | The text of the agenda. See [The agenda document](#the-agenda-document). |

The server sends these requests to the client:

| Request | When |
| --- | --- |
| `workspace/applyEdit` | A command changes an Org file. |
| `window/showDocument` | To show the agenda, or the file of an agenda entry. |
| `window/showMessageRequest` | The agenda dispatcher asks which view to open. |
| `workspace/textDocumentContent/refresh` | The agenda has new text. |

**Position encoding.** The server picks UTF-8 if the client offers it, then
UTF-32, and falls back to UTF-16.

**Line endings.** `\n`, `\r\n` and a lone `\r` all end a line for positions.

## Options

Passed as `initializationOptions`. The names are Org's variable names in
camel case; the agenda's lose their `org-agenda-` prefix and live under
`agenda`. Defaults are Org's. Unknown or malformed options are ignored.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `agendaFiles` | `string[]` | `[]` | Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built from. `~/` is the home directory. A directory stands for the Org files directly in it. |
| `todoKeywords` | `string[]` | `["TODO", "|", "DONE"]` | The TODO sequence, written like Org's `#+TODO:` line. `|` separates the states that count as done; without it the last keyword is the only done state. `TODO(t)`, `WAIT(w@/!)` and the like are accepted: `!` and `@` make the keyword log its changes. |
| `logDone` | `false`, `"time"`, `"note"` | `false` | `org-log-done`: write `CLOSED:` when an entry is done. |
| `logRepeat` | `false`, `"time"`, `"note"` | `"time"` | `org-log-repeat`: record when a repeating entry is done. |
| `logIntoDrawer` | `boolean`, `string` | `false` | `org-log-into-drawer`: `true` for `LOGBOOK`, or a drawer name. |
| `tagsColumn` | `number` | `-77` | `org-tags-column`: where tags are aligned after a TODO change. |
| `todoRepeatToState` | `string`, `true` | first keyword | `org-todo-repeat-to-state`: the state a repeating entry returns to; `true` for the state it had. |
| `priorityHighest` | letter or `0`–`64` | `"A"` | `org-priority-highest` |
| `priorityLowest` | letter or `0`–`64` | `"C"` | `org-priority-lowest` |
| `priorityDefault` | letter or `0`–`64` | `"B"` | `org-priority-default`: the priority of an entry without a cookie. |
| `priorityStartCycleWithDefault` | `boolean` | `true` | `org-priority-start-cycle-with-default`: raising or lowering an entry without a priority starts at the default. |
| `timestampRoundingMinutes` | `number` | `5` | The second number of `org-time-stamp-rounding-minutes`: the step in which the minutes of a timestamp change. |
| `readDatePreferFuture` | `boolean`, `"time"` | `true` | `org-read-date-prefer-future`: a date typed without a year or month is the next one to come; with `"time"`, a time that has passed today is tomorrow's. |
| `readDateForceCompatibleDates` | `boolean` | `true` | `org-read-date-force-compatible-dates`: a typed year is kept within 1970 to 2037. |
| `cycleSeparatorLines` | `number` | `2` | `org-cycle-separator-lines`: with at least this many blank lines after a subtree, the last one stays outside its fold. Negative: all of them stay outside. `0`: none does. Blank lines at the end of the file are never folded. |
| `agenda` | object | | See below. |
| `virtualDocuments` | `boolean` | from capabilities | Whether the client fetches the agenda with `workspace/textDocumentContent`. A client that announces that capability need not set this. |

A document that has `#+TODO:`, `#+SEQ_TODO:` or `#+TYP_TODO:` lines uses
those keywords instead of `todoKeywords`, and one with a `#+PRIORITIES:`
line uses its priorities instead of the three `priority…` options; the
agenda sorts each entry by the priorities of its own file.

Without `agendaFiles`, the agenda is built from the Org files under the
workspace folders (hidden directories, `node_modules` and `target`
excluded). Without workspace folders too, it is built from the Org files
that are open. A file that is open in the editor is always read from the
editor, not from disk.

Options under `agenda`:

| Option | Type | Default | Org variable |
| --- | --- | --- | --- |
| `span` | `"day"`, `"week"`, `"fortnight"`, `"month"`, `"year"`, or a number of days | `"week"` | `org-agenda-span` |
| `startOnWeekday` | `0`–`6` (0 is Sunday), or `false` | `1` | `org-agenda-start-on-weekday` |
| `deadlineWarningDays` | `number` | `14` | `org-deadline-warning-days` |
| `scheduledDelayDays` | `number` | `0` | `org-scheduled-delay-days` |
| `scheduledPastDays` | `number` | `10000` | `org-scheduled-past-days` |
| `deadlinePastDays` | `number` | `10000` | `org-deadline-past-days` |
| `skipScheduledIfDone` | `boolean` | `false` | `org-agenda-skip-scheduled-if-done` |
| `skipDeadlineIfDone` | `boolean` | `false` | `org-agenda-skip-deadline-if-done` |
| `skipTimestampIfDone` | `boolean` | `false` | `org-agenda-skip-timestamp-if-done` |
| `showFutureRepeats` | `boolean`, `"next"` | `true` | `org-agenda-show-future-repeats` |
| `showAllDates` | `boolean` | `true` | `org-agenda-show-all-dates` |
| `tagsColumn` | `number` | `-80` | `org-agenda-tags-column`. Org's own default follows the window width, which a server does not know. |
| `useTimeGrid` | `boolean` | `true` | `org-agenda-use-time-grid` |
| `timeGrid` | object: `daily`, `today`, `weekly`, `requireTimed`, `removeMatch` (booleans), `times` (numbers such as `800`), `trailer`, `line`, `now` (strings) | Org's | `org-agenda-time-grid`, `org-agenda-current-time-string` |
| `showCurrentTimeInGrid` | `boolean` | `true` | `org-agenda-show-current-time-in-grid` |
| `weekendDays` | `number[]` | `[6, 0]` | `org-agenda-weekend-days` |

## Commands

Commands run through `workspace/executeCommand`. The one argument says
where the cursor is, in a document the server knows:

```json
{
  "command": "org.todo.next",
  "arguments": [{ "textDocument": { "uri": "file:///home/me/org/work.org" }, "position": { "line": 3, "character": 0 } }]
}
```

This is `TextDocumentPositionParams`, with one optional field more:
`count`, a number the user typed before the key (Emacs's prefix argument,
Vim's count). The commands that move something by steps take it as the
number of steps; the others ignore it. Commands that open a view work
without the argument.

A command that cannot run answers with error code -32803
(`RequestFailed`) and a message meant for the user, such as
`No agenda is open`. When the command means nothing at the cursor (a
timestamp key off a timestamp, a headline command before the first
headline), the error's `data` is `{ "notApplicable": true }`. An editor
that bound the command to a key with a meaning of its own (see `native`
in [The manifest](#the-manifest)) then lets the key do that instead:
Vim's `<C-a>` on a timestamp changes the date and elsewhere increments a
number.

| Command | Does |
| --- | --- |
| `org.agenda.dispatch` | Asks which agenda view to open. |
| `org.agenda.list` | Opens the agenda for the current week or day. |
| `org.agenda.todos` | Opens the list of all TODO entries. |
| `org.agenda.later`, `org.agenda.earlier` | Moves the agenda one span forward or backward. |
| `org.agenda.today` | Moves the agenda to today. |
| `org.agenda.day`, `.week`, `.fortnight`, `.month`, `.year` | Changes the span, keeping the day under the cursor in view. |
| `org.agenda.redo` | Rebuilds the agenda. |
| `org.agenda.goto` | Shows the entry of the agenda line under the cursor. |
| `org.todo.next`, `org.todo.previous` | Changes the TODO state of the entry at the cursor, in an Org file or on an agenda line. |
| `org.priority.up`, `org.priority.down` | Raises or lowers the priority of the entry at the cursor, in an Org file or on an agenda line. |
| `org.timestamp.up`, `org.timestamp.down` | Changes the part of the timestamp the cursor is on. Org files only. |
| `org.date.later`, `org.date.earlier` | Moves a date by days: in an Org file the timestamp at the cursor, on an agenda line the date the line is listed for. |
| `org.shift.up`, `.down`, `.left`, `.right` | Org's shifted cursor keys in an Org file: on a timestamp they change it, on a headline they change its priority (up, down) or TODO state (left, right). |
| `org.schedule`, `org.deadline` | Asks for a date and sets the entry's `SCHEDULED:` or `DEADLINE:`. Org files only, for now. |
| `org.timestamp.insert`, `org.timestamp.insert.inactive` | Asks for a date and inserts a timestamp at the cursor, or changes the one there. Org files only. |
| `org.date.preview` | Not for users: what an answer to a date prompt means; see below. Not in the manifest. |

**TODO state changes** do what Org's `org-todo` does. The cycle runs
through "no keyword" between the last and first states. The tags of the
headline return to their column, and statistics cookies (`[1/3]`, `[33%]`)
in the parent headline are recounted. `CLOSED:` and state notes are
written as `logDone` and the keywords ask; where Org would prompt for the
text of a note, the note is written without text. An entry with a repeater
that is marked done moves to its next date and is open again, with
`LAST_REPEAT` and a state note recorded.

The entry is the innermost headline whose subtree contains the cursor, so
the cursor may be anywhere in the entry's body.

**Priorities** change as `org-priority-up` and `-down` do them, within
the file's priorities. Past the highest or lowest the cookie is removed;
the same key pressed again goes on from the other end. "Pressed again"
means the same command on the same entry with no other command in
between; the server cannot see the cursor move, as Emacs can.

**Timestamps** change as `org-timestamp-up` and `-down` do them
(`S-<up>`, `S-<down>` in Emacs): the year, month, day, hour or minute
the cursor is on, the end of a time range together with its start, a
repeater or warning period, or, on a bracket, active against inactive.
Minutes move in steps of `timestampRoundingMinutes` unless a count is
given. On a `CLOCK:` line the duration is recomputed. In a file,
`org.date.later` and `-earlier` are `org-timestamp-up-day` and
`-down-day`. On an agenda line they are `org-agenda-date-later` and
`-earlier`: the first step forward takes a date that has passed to
today, and a date range moves as a whole.

**Commands that ask for something.** In the manifest such a command has
`"input": { "kind", "prompt" }`. Run without an answer, it changes
nothing and returns what to ask:

```json
{ "input": { "kind": "date", "prompt": "Schedule", "value": "10:00" } }
```

`value` is the text the prompt starts with, as Emacs fills it in: the
time of the date being changed (`10:00+1:30` for a range), or nothing.
The editor asks with its own input box and runs the command again with
the answer as `input` in the argument:

```json
{ "textDocument": { "uri": "…" }, "position": { … }, "input": "fri 10:00" }
```

A `date` answer is read as Org's date prompt reads it (`org-read-date`):
`+2d`, `fri`, `2/15`, `feb 15 3pm`, `w12`, `10:00-11:30`, `++1w`, and so
on, against the date being changed or else today. While it is typed, an
editor can show what it means with `org.date.preview`, whose argument is
the same plus `"command"`, the command being answered; it returns
`{ "timestamp": "<2026-05-22 Fri 10:00>" }`, or fails while the answer
means nothing.

**Scheduling** works as `org-schedule` and `org-deadline` do: the
timestamp goes on the planning line under the headline, which is made
when there is none; a `CLOSED:` timestamp is taken out; the repeater of
the old date stays. **Timestamps** work as `org-timestamp` does: on a
timestamp the command changes it (keeping its repeater), elsewhere it
inserts one; right after inserting one, the same command adds the end of
a date range (`<…>--<…>`). After a timestamp is inserted, the server
moves the cursor after it with `window/showDocument`, as Emacs leaves
it. Changes are not logged (`org-log-reschedule` and the like) yet.

**Code actions.** The commands that apply at the cursor are also offered
as code actions, with the command name as the action's kind. In an Org
file the actions carry their edit (versioned when the client supports
`documentChanges`); in the agenda they carry the command to run. A client
can therefore also bind a key by requesting code actions with
`context.only` set to a kind, which matches hierarchically: `org` selects
every action.

**Edits to files that are not open.** A command run from the agenda edits
the entry's file with `workspace/applyEdit`. Editors apply such an edit to
a buffer and leave saving to the user, as Emacs does. The agenda follows
the edit at once and again when the file is saved.

**Reporting edits at once.** The server builds on the text it was last
told about. A key held down sends the next command before an editor that
batches its `didChange` notifications (Neovim waits 150 ms) has reported
the previous edit, and that press is lost. An integration should send the
change as soon as it has applied a `workspace/applyEdit`.

## The manifest

What an editor integration has to know beyond LSP's own fields, the server
announces itself, so that integrations carry no tables about Org. The
manifest is `capabilities.experimental.org` in the `initialize` result, and
`org-lsp manifest` prints it for integrations that are generated ahead of
time.

```json
{
  "commands": [
    {
      "command": "org.todo.next",
      "title": "Cycle TODO state forward",
      "edits": true,
      "keys": [
        { "key": "C-c C-t", "in": "org", "preset": "emacs" },
        { "key": "cit", "in": "org", "preset": "vim" },
        { "key": "t", "in": "agenda", "preset": "vim" }
      ]
    }
  ],
  "agenda": {
    "scheme": "org-agenda",
    "uri": "org-agenda:/Org%20Agenda",
    "fileName": "Agenda.org-agenda",
    "languageId": "org-agenda"
  },
  "tokenTypes": [{ "id": "headline", "description": "The title text of a headline", "standard": false }],
  "tokenModifiers": [{ "id": "level1", "description": "Headline level 1" }],
  "highlights": [
    { "token": "headline.level1", "vim": ["@org.headline.level1", "Title"], "textmate": ["markup.heading.1.org"] }
  ]
}
```

- **`commands`**: every command with its title and default keys. `edits`
  marks commands that change an Org file, for editors that can repeat the
  last change. A key applies `in` Org files (`org`), the agenda (`agenda`)
  or everywhere (`global`). There are two presets: `emacs` has Org's own
  keys in Emacs notation (`C-c C-t`, `RET`, `v d`), `vim` has
  nvim-orgmode's defaults in Vim notation (`cit`, `<CR>`, `vd`). A key
  marked `"native": true` keeps its own meaning where the command does
  not apply, such as `<C-a>` or `S-<up>`; see `notApplicable` under
  [Commands](#commands).
- **`agenda`**: how the agenda document is named; see below.
- **`tokenTypes`**, **`tokenModifiers`**: the semantic token legend with
  descriptions. `standard` types are LSP's own.
- **`highlights`**: how tokens should look by default. `token` is a type, a
  `type.modifier`, or `*.modifier` for any type. `vim` lists highlight
  groups, the first defined one to be used: the group nvim-orgmode
  colorschemes define, then a standard group. `textmate` lists scopes for
  editors whose themes match scopes, most specific first.

## The agenda document

The agenda is one document that the server owns, as it is one buffer in
Emacs: a new view replaces the old one. Its language identifier is
`org-agenda`. The server remembers which entry each line stands for, so
semantic tokens, go-to-definition, code actions and commands work on it
like on any document.

A client that announces `workspace.textDocumentContent` (LSP 3.18) is
shown `org-agenda:/Org%20Agenda` and fetches its text with
`workspace/textDocumentContent`. Other clients are shown a file,
`Agenda.org-agenda` in a temporary directory, which the server rewrites
and removes when it stops.

Opening a view goes like this:

1. The client runs `org.agenda.list`.
2. The server sends `window/showDocument` with the agenda's URI and, as
   the selection, the line of today's date.
3. The client opens the document, which makes it fetch the text.

When the view changes while the client has it open:

1. The server sends `workspace/textDocumentContent/refresh`.
2. The client fetches the text again and, since the document is open,
   reports the change with `textDocument/didChange`.
3. If the cursor is to move (the view went to another week, say), the
   server sends `window/showDocument` with the new line only now, when the
   client's copy matches. Sent earlier, the line number would be applied to
   the old text.

For step 3 the client must sync the agenda document like any other:
`didOpen` when it shows it, `didChange` when it replaces the text,
`didClose` when it closes it. After `didClose` the server stops rebuilding
the view.

**The cursor follows an entry.** A change of priority or date can move an
entry to another line, or another day. The server then moves the cursor
to the entry's new line the same way, once the client shows the new text,
so that the next press of the key reaches the same entry. Emacs instead
leaves the line where it is until the agenda is rebuilt. A TODO change
leaves the cursor where it is, as in Emacs.

**Lines of a view the client still shows.** A key pressed before the
client has fetched the new text names a line of the old one. The server
keeps the old view until the client's copy has changed, and resolves the
line against it. Likewise, while the cursor is on its way to an entry's
new line, a command from the line it is leaving is meant for the entry.

## Semantic tokens

Tokens never overlap and never span lines. Nested markup is flattened into
modifiers: in `*bold /both/*` the inner text carries `bold` and `italic`.
Emphasis markers are part of the token they delimit. The manifest has the
full legend with descriptions; this is an overview.

### In Org files

| Type | Covers |
| --- | --- |
| `headline` | Headline title text. Carries a `levelN` modifier. |
| `stars` | The stars of a headline. Carries a `levelN` modifier. |
| `todoKeyword` | A TODO keyword; `done` when it is a done state. |
| `priority` | A priority cookie, `[#A]`. |
| `tag` | The tags of a headline, `:a:b:`, as one token. |
| `planning` | `SCHEDULED:`, `DEADLINE:`, `CLOSED:`, `CLOCK:` and a headline's `COMMENT`. |
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

Plain text gets no token, and neither do the contents of source and
example blocks.

### In the agenda

Keywords, priorities, tags and links inside an agenda line have the types
above. The rest of a line has one of these, which follow Org's agenda
faces:

| Type | Covers | Modifiers |
| --- | --- | --- |
| `agendaStructure` | The title line of a view. | |
| `agendaDate` | A date line. | `today`, `weekend` |
| `agendaScheduled` | A scheduled entry. | `today`; `past` when scheduled before today |
| `agendaDeadline` | An entry with a deadline. | `imminent` when due or overdue, `distant` when far away |
| `agendaEvent` | An entry with a plain timestamp. | `range` inside a date range |
| `agendaDone` | An entry that is done. | |
| `agendaTimeGrid` | A line of the time grid. | `current` for the current time |

### Modifiers

| Modifier | On |
| --- | --- |
| `level1` … `level8` | `headline` and `stars`. Levels above eight wrap around. |
| `done` | `todoKeyword` |
| `inactive` | `timestamp` |
| `checked`, `partial` | `checkbox` |
| `bold`, `italic`, `underline`, `strikethrough` | `text` and `headline` |
| `today`, `weekend`, `past`, `imminent`, `distant`, `range`, `current` | the agenda types, as listed above |

## Writing an editor integration

An integration should stay free of Org logic. Its whole job:

1. Start `org-lsp` for `org` files, and on demand for the agenda.
2. Apply the manifest: link the tokens to the editor's highlight names and
   bind the keys of its preset to the commands.
3. Run commands with the cursor position as their argument.
4. Show the agenda document and keep it in sync, as described above.
5. Open and close folds; the server only says where they are.

`lua/org-lsp/` does this at run time. `editors/vscode/` cannot, because
VS Code wants commands, keys and token colors declared in `package.json`:
there `scripts/generate.mjs` writes them from `org-lsp manifest`.

Without an integration, an editor still gets what its LSP client supports
on its own: highlighting, folding, the outline, and TODO changes as code
actions. Commands need the cursor position as an argument, which generic
clients do not send, and the agenda needs `window/showDocument`.
