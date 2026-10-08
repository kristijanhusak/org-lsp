// What org-lsp does of Emacs Org, area by area, and how far each editor
// goes with it: the single source of the feature tables in README.md and
// the editors' READMEs, of docs/parity.md, and of the editors' manual test
// lists (editors/<editor>/TESTING.md). scripts/generate-docs.mjs writes
// them.
//
// An area has:
//
//   emacs    what Org does there, each [text, done] (or [text, done, of]
//            for a count), the parity being the share done;
//   editors  per editor, 'yes' (all the server does works), 'part' (some
//            does not, `why` says what and why), or 'no' (with `why`);
//            left out, an editor has the area as 'yes';
//   checks   what to try by hand: `text`, and the command it runs (`cmd`,
//            its key looked up per editor, `in` the scope of the key when
//            not Org files), or `keys` per editor; `only` or `not` limit a
//            check to some editors.

export const EDITORS = [
  {
    id: 'neovim',
    name: 'Neovim',
    dir: 'editors/neovim',
    setup: [
      'Installed with lazy.nvim (`build = \':OrgLspDownload\'`), no server around: opening an Org file downloads the server and starts it',
      '`:checkhealth org-lsp` reports the server, where it came from and the agenda files',
      '`:OrgLspDownload` downloads the server again and restarts it in the open Org files',
      '`g?` in an Org file and in the agenda lists the keys; `<Leader>oa` works in any buffer',
      'Options from `~/.config/org-lsp/config.toml` apply, and `setup()`\'s `settings` go on top',
    ],
  },
  {
    id: 'vim',
    name: 'Vim',
    dir: 'editors/vim',
    setup: [
      'Installed with one client (yegappan/lsp, vim-lsp, coc.nvim or ALE): the server starts on an Org file. Go through the list once with each client you use',
      '`g?` in an Org file and in the agenda lists the keys; `<Leader>oa` works in any buffer',
      'Options from `~/.config/org-lsp/config.toml` apply, and `g:org_lsp`\'s `settings` go on top',
    ],
  },
  {
    id: 'vscode',
    name: 'VS Code',
    dir: 'editors/vscode',
    setup: [
      'Installed with `scripts/install-vscode.sh` (or a `.vsix` from the release): the server starts on an Org file',
      'The `Org:` commands are in the Command Palette',
      '`"org-lsp.keys": "vim"` (with VSCodeVim) switches to the Vim keys',
      '`org-lsp.` settings and `~/.config/org-lsp/config.toml` reach the server',
    ],
  },
  {
    id: 'sublime',
    name: 'Sublime Text',
    dir: 'editors/sublime',
    setup: [
      'Installed with `scripts/install-sublime.sh`: LSP comes with it, and the server is found or downloaded',
      'LSP\'s `semantic_highlighting` is on',
      'The `Org:` commands are in the Command Palette; *Org: Download the latest server* replaces the downloaded server',
    ],
  },
  {
    id: 'zed',
    name: 'Zed',
    dir: 'editors/zed',
    setup: [
      'Installed as a dev extension: `org-lsp` is found on PATH or downloaded',
      '`"semantic_tokens": "combined"` and inlay hints on for Org in the settings',
      'Code actions (`ctrl-.`) list the Org commands where they apply',
      'The keymap and tasks from the README run the agenda\'s commands by key',
    ],
  },
  {
    id: 'helix',
    name: 'Helix',
    dir: 'editors/helix',
    setup: [
      '`editors/helix/languages.toml` added to `~/.config/helix/languages.toml`, `org-lsp` on PATH: `:lsp-restart` starts it',
      'Code actions (`Space a`) list the Org commands where they apply',
      'Inlay hints show with `display-inlay-hints = true`',
    ],
  },
];

const ZED_TEXT = 'Zed cannot ask for text on a server\'s behalf';
const HELIX_TEXT = 'Helix cannot ask for text on a server\'s behalf';
const NO_KEY_ZED = 'Zed cannot bind a key to a server\'s command (the commands are code actions)';
const NO_KEY_HELIX = 'Helix cannot bind a key to a server\'s command (the commands are code actions, `Space a`)';

export const AREAS = [
  {
    name: 'Highlighting',
    emacs: [
      ['Headlines by level, TODO keywords, priorities and tags', true],
      ['Emphasis, verbatim and code', true],
      ['Links shown as their description (`org-link-descriptive`)', true],
      ['Emphasis markers hidden (`org-hide-emphasis-markers`)', true],
      ['Timestamps, planning and clock lines', true],
      ['Blocks, keywords, drawers, properties and comments', true],
      ['Code in its language (`org-src-fontify-natively`)', true],
      ['Tables', true],
      ['Footnotes, targets, macros, entities and LaTeX fragments', true],
      ['Archived subtrees shaded (`org-archived`)', true],
      ['Radio targets turning their words into links', false],
      ['Entities and sub- and superscripts shown as characters (`org-pretty-entities`)', false],
    ],
    editors: {
      vim: ['part', 'Links and emphasis markers are not concealed: Vim cannot hide text that has text properties.'],
      sublime: ['part', 'Links and emphasis markers are not concealed: Sublime Text cannot hide text in a view. The colors need LSP\'s `semantic_highlighting` on.'],
      zed: ['part', 'Links and emphasis markers are not concealed: Zed lets no server hide text. The colors need `"semantic_tokens": "combined"`; code blocks take their language\'s colors, inline code does not.'],
      helix: ['no', 'Helix does not use semantic tokens: it colors Org with its own tree-sitter grammar.'],
    },
    checks: [
      { text: 'Headlines are colored by level; TODO and DONE keywords, priorities (`[#A]`) and tags have colors of their own' },
      { text: '`*bold*`, `/italic/`, `_underline_`, `+strike+`, `=verbatim=` and `~code~` are colored' },
      { text: 'A link shows its description alone; its brackets and target show when the cursor is on its line', not: ['vim', 'sublime', 'zed'] },
      { text: 'With `org_hide_emphasis_markers = true`, the markers of `*bold*` are hidden', not: ['vim', 'sublime', 'zed'] },
      { text: 'Timestamps, `SCHEDULED:`/`DEADLINE:`, `CLOCK:` lines, drawers, `#+` keywords and `#` comments are colored' },
      { text: 'A `#+begin_src python` block shows its code in Python\'s colors' },
      { text: 'Inline code (`src_python{x = 1}`) is in Python\'s colors', not: ['zed'] },
      { text: 'Table rules and bars, footnotes (`[fn:1]`), `<<targets>>`, macros (`{{{x}}}`) and entities (`\\alpha`) are colored' },
      { text: 'A subtree tagged `:ARCHIVE:` is shaded' },
      { text: 'Another color scheme keeps the colors (they follow its groups)', only: ['neovim', 'vim'] },
    ],
  },
  {
    name: 'Folding and visibility',
    emacs: [
      ['Cycling a subtree (`TAB`): folded, children, subtree', true],
      ['Cycling the file (`S-TAB`): overview, contents, everything', true],
      ['Startup visibility `overview` and `showeverything`', true],
      ['Other startup visibilities (`content`, `show2levels`…) and the `VISIBILITY` property', false],
      ['Blank lines after a subtree kept outside its fold (`org-cycle-separator-lines`)', true],
      ['Archived subtrees left folded (`org-cycle-hide-archived-subtrees`)', true],
      ['Sparse trees (`C-c /`)', true],
      ['Narrowing to a subtree or block (`C-x n s`, `C-x n b`)', false],
      ['Showing the context of the cursor (`org-reveal`, `C-c C-r`)', false],
    ],
    editors: {
      zed: ['part', 'Zed folds with its own folding: Org\'s cycling and sparse trees need folds an extension can set, which Zed does not offer.'],
      helix: ['no', 'Helix has no folding.'],
    },
    checks: [
      { text: 'On a headline, cycling goes folded → children → subtree', keys: { neovim: '`<Tab>`', vim: '`<Tab>`', vscode: '`Tab`', sublime: '`Tab`' } },
      { text: 'Cycling the whole file goes overview → contents → everything', keys: { neovim: '`<S-Tab>`', vim: '`<S-Tab>`', vscode: '`Shift+Tab`', sublime: '`Shift+Tab`' } },
      { text: 'A file with `#+STARTUP: overview` opens folded', only: ['neovim', 'vim', 'vscode', 'sublime'] },
      { text: 'Two blank lines after a subtree stay visible below it when it is folded', only: ['neovim', 'vim', 'vscode', 'sublime'] },
      { text: 'Opening the parent of an `:ARCHIVE:` subtree leaves that subtree folded', only: ['neovim', 'vim', 'vscode', 'sublime'] },
      { text: 'A sparse tree of the TODO entries folds everything else and highlights them', cmd: 'org.sparse.tree', only: ['neovim', 'vim', 'vscode', 'sublime'] },
      { text: 'Zed\'s own folds open and close on headlines, drawers and blocks', only: ['zed'] },
    ],
  },
  {
    name: 'Headings and structure',
    emacs: [
      ['New headings (`M-RET`, `M-S-RET`, `C-RET`, `C-S-RET`)', true],
      ['Promoting and demoting a heading or a subtree', true],
      ['Moving a subtree up and down', true],
      ['Cutting, copying and pasting subtrees, their level fitted', true],
      ['Sorting entries (`C-c ^`)', true],
      ['The outline of the file (document symbols)', true],
      ['Moving between headings (`C-c C-n`, `C-c C-p`, `C-c C-f`, `C-c C-b`, `C-c C-u`)', false],
      ['Jumping to a heading (`org-goto`, `C-c C-j`)', false],
      ['Turning lines into headings and back (`C-c *`)', false],
      ['Toggling `COMMENT` (`C-c ;`)', false],
      ['Cloning a subtree with its dates shifted (`C-c C-x c`)', false],
    ],
    editors: {
      helix: ['part', 'New headings (`M-RET` and its relatives) are left out: they carry the cursor for the editor to move after the edit, which Helix\'s code actions cannot do.'],
    },
    checks: [
      { text: 'On a headline, a new heading of the same level goes below', cmd: 'org.meta.return', not: ['helix'] },
      { text: 'A new TODO heading', cmd: 'org.insert.todo.heading', not: ['helix'] },
      { text: 'A new heading after the subtree, and a TODO one', cmd: ['org.insert.heading.after', 'org.insert.todo.heading.after'], not: ['helix'] },
      { text: 'Promoting and demoting a heading; its tags stay aligned', cmd: ['org.meta.left', 'org.meta.right'] },
      { text: 'Promoting and demoting a subtree', cmd: ['org.shiftmeta.left', 'org.shiftmeta.right'] },
      { text: 'Moving a subtree up and down past its siblings', cmd: ['org.meta.up', 'org.meta.down'] },
      { text: 'Cutting a subtree and pasting it under another heading: its level fits there', cmd: ['org.cut.subtree', 'org.paste.subtree'] },
      { text: 'Copying a subtree and pasting it', cmd: ['org.copy.subtree', 'org.paste.subtree'] },
      { text: 'Sorting the children of a heading alphabetically, then by TODO state', cmd: 'org.sort' },
      { text: 'The outline lists the headings nested', keys: { neovim: '`:lua vim.lsp.buf.document_symbol()`', vim: 'the client\'s document symbols', vscode: 'the Outline view', sublime: '*Goto Symbol*', zed: 'the outline panel', helix: '`Space s`' } },
    ],
  },
  {
    name: 'Lists and checkboxes',
    emacs: [
      ['New items, numbered and with checkboxes (`M-RET`, `M-S-RET`)', true],
      ['`RET` in an item (`org-return`)', true],
      ['Indenting, outdenting and moving items, with their children', true],
      ['Cycling bullets (`S-<left>`, `S-<right>`)', true],
      ['Repairing and renumbering a list (`C-c C-c`)', true],
      ['Checkboxes, with their parents and `[1/3]` or `[33%]` cookies', true],
      ['Sorting items', true],
      ['Moving to the next and previous item', true],
      ['Turning lines into items and back (`C-c -`)', false],
      ['Radio lists', false],
      ['Circular motion between items (`org-list-use-circular-motion`)', false],
    ],
    editors: {
      zed: ['part', `\`RET\` stays Zed's own and bullets are not cycled: ${NO_KEY_ZED}, and those two are not among them.`],
      helix: ['part', `\`RET\` stays Helix's own, new items and cycled bullets are left out: ${NO_KEY_HELIX}, and those are not among them.`],
    },
    checks: [
      { text: 'On an item, the next item goes below; in a numbered list the numbers go on, after a checkbox item the new one has `[ ]`', cmd: 'org.meta.return', not: ['helix'] },
      { text: '`RET` in the middle of an item\'s line keeps the rest of the line in the item', cmd: 'org.return', not: ['zed', 'helix'] },
      { text: 'The next item from the end of an item\'s line', cmd: 'org.return.item', only: ['neovim', 'vim'] },
      { text: 'Indenting and outdenting an item; and with its children', cmd: ['org.meta.right', 'org.meta.left', 'org.shiftmeta.right', 'org.shiftmeta.left'] },
      { text: 'Moving an item up and down', cmd: ['org.meta.up', 'org.meta.down'] },
      { text: 'Cycling the bullet `-` → `+` → `1.` → `1)` on an item', cmd: ['org.shift.right', 'org.shift.left'], not: ['zed', 'helix'] },
      { text: 'Toggling a checkbox: its parent box and the `[1/3]` cookie follow', cmd: 'org.checkbox.toggle' },
      { text: 'On a list numbered 1, 3, 2, the same command renumbers and repairs it', cmd: 'org.checkbox.toggle' },
      { text: 'Sorting a list\'s items alphabetically', cmd: 'org.sort' },
    ],
  },
  {
    name: 'TODO keywords',
    emacs: [
      ['Cycling states, and fast selection by key (`TODO(t)`)', true],
      ['Sequences and types from `#+TODO:` and `org_todo_keywords`', true],
      ['Logging: `CLOSED:`, the time or a note on a state change (`!`, `@`), `LOG_INTO_DRAWER`', true],
      ['Repeating entries moving to their next date', true],
      ['The parent\'s statistics cookies recounted', true],
      ['Blocking a state change by children and `ORDERED` (`org-enforce-todo-dependencies`)', false],
    ],
    editors: {
      zed: ['part', `A note for a state change (\`@\`) cannot be typed: ${ZED_TEXT}. States are picked with buttons.`],
      helix: ['part', `A note for a state change (\`@\`) cannot be typed: ${HELIX_TEXT}.`],
    },
    checks: [
      { text: 'Changing the TODO state of a headline (with `org_todo_keywords` keys, picked by key)', cmd: 'org.todo' },
      { text: 'Cycling to the next and previous state', cmd: ['org.todo.next', 'org.todo.previous'], not: ['helix'] },
      { text: 'Repeating the change with `.`', only: ['neovim', 'vim'] },
      { text: 'Done with `org_log_done = "time"` writes `CLOSED: [...]` below the headline' , cmd: 'org.todo' },
      { text: 'A state with `@` (`WAIT(w@)`) asks for a note, written below the headline', cmd: 'org.todo', not: ['zed', 'helix'] },
      { text: 'Done on a repeating entry (`<2026-01-01 Thu +1w>`) moves its date a week on and keeps it TODO', cmd: 'org.todo' },
      { text: 'The parent\'s `[1/2]` cookie is recounted', cmd: 'org.todo' },
      { text: 'A `#+TODO: A B | C` line in the file is used instead of the settings', cmd: 'org.todo' },
    ],
  },
  {
    name: 'Priorities',
    emacs: [
      ['Raising, lowering, setting and removing', true],
      ['`#+PRIORITIES:` and number priorities', true],
      ['From the agenda', true],
    ],
    checks: [
      { text: 'Raising and lowering the priority of a headline', cmd: ['org.priority.up', 'org.priority.down'] },
      { text: 'Setting it by key; `SPC` removes it', cmd: 'org.priority', not: ['helix'] },
      { text: '`#+PRIORITIES: 1 5 3` makes the priorities numbers', cmd: 'org.priority.up' },
    ],
  },
  {
    name: 'Tags',
    emacs: [
      ['Setting tags: fast selection by key, exclusive groups, typed', true],
      ['Aligning tags (`org_tags_column`)', true],
      ['Inheritance and `#+FILETAGS:`', true],
      ['Completion', true],
      ['Group tags in searches', true],
      ['A sparse tree by tags', true],
    ],
    editors: {
      zed: ['part', `Setting tags is left out: it needs keys pressed in turn, or typed tags, and ${ZED_TEXT}. Completion and renaming work.`],
      helix: ['part', `Setting tags is left out: it needs keys pressed in turn, or typed tags, and ${HELIX_TEXT}. Completion and renaming work.`],
    },
    checks: [
      { text: 'Setting tags from `#+TAGS: work(w) home(h)` by key; each key turns its tag on and off', cmd: 'org.tags.set', not: ['zed', 'helix'] },
      { text: 'An exclusive group (`{ @office(o) @home(m) }`) turns the other tag off', cmd: 'org.tags.set', not: ['zed', 'helix'] },
      { text: 'Typing tags when none have keys; they are sorted and aligned to `org_tags_column`', cmd: 'org.tags.set', not: ['zed', 'helix'] },
      { text: 'Tags are completed after `:` at the end of a headline' },
      { text: 'Renaming a tag renames it in every headline and `#+FILETAGS:`', keys: { neovim: '`:lua vim.lsp.buf.rename()` on the tag', vim: 'the client\'s rename on the tag', vscode: '`F2` on the tag', sublime: '*LSP: Rename* on the tag', zed: '`F2` on the tag', helix: '`Space r` on the tag' } },
    ],
  },
  {
    name: 'Properties and column view',
    emacs: [
      ['Setting and deleting properties, also from the agenda', true],
      ['Allowed values (`NAME_ALL`) cycled with `S-<left>` and `S-<right>`', true],
      ['Effort (`C-c C-x e`) and `ORDERED`', true],
      ['Inheritance, special properties and property searches', true],
      ['Column view (`C-c C-x C-c`)', false],
      ['Column view dynamic blocks and summaries', false],
    ],
    editors: {
      zed: ['part', `Setting a property or the effort is left out: they are typed, and ${ZED_TEXT}.`],
      helix: ['part', `Setting a property or the effort is left out: they are typed, and ${HELIX_TEXT}.`],
    },
    checks: [
      { text: 'Setting a property: the name and the value are completed, `NAME_ALL`\'s values offered', cmd: 'org.property.set', not: ['zed', 'helix'] },
      { text: 'On a property line with `NAME_ALL`, cycling through its values', cmd: ['org.shift.right', 'org.shift.left'] },
      { text: 'Deleting a property from the entry, and from the whole file', cmd: ['org.property.delete', 'org.property.delete.globally'] },
      { text: '`C-c C-c` in a property drawer asks what to do with the property', cmd: 'org.checkbox.toggle', not: ['zed', 'helix'] },
      { text: 'Setting the effort', cmd: 'org.effort.set', not: ['zed', 'helix'] },
      { text: 'Toggling `ORDERED`', cmd: 'org.ordered.toggle' },
      { text: 'Property names are completed in a drawer' },
    ],
  },
  {
    name: 'Dates and times',
    emacs: [
      ['Inserting active and inactive timestamps (`C-c .`, `C-c !`)', true],
      ['Scheduling and deadlines (`C-c C-s`, `C-c C-d`)', true],
      ['Org\'s date prompt: `+2d`, `fri`, `2/15`, `10:00-11:30`…', true],
      ['The calendar with the prompt', true],
      ['Changing the part at the cursor (`S-<up>`, `S-<down>`), with ranges, repeaters and warnings', true],
      ['Moving a date by days (`S-<left>`, `S-<right>`)', true],
      ['Removing a date or setting a delay (the prefix arguments)', false],
    ],
    editors: {
      vim: ['part', 'Dates are typed, without the calendar: the Vim plugin asks with an input line.'],
      sublime: ['part', 'Dates are typed, without the calendar: the package asks with an input panel.'],
      zed: ['part', `Scheduling, deadlines and new timestamps are left out: they ask for a date, and ${ZED_TEXT}. Changing a timestamp works.`],
      helix: ['part', `Scheduling, deadlines and new timestamps are left out: they ask for a date, and ${HELIX_TEXT}. Changing a timestamp works.`],
    },
    checks: [
      { text: 'Scheduling with `+2d`: `SCHEDULED: <…>` goes below the headline', cmd: 'org.schedule', not: ['zed', 'helix'] },
      { text: 'A deadline on `fri 10:00`', cmd: 'org.deadline', not: ['zed', 'helix'] },
      { text: 'Inserting an active and an inactive timestamp; a second one right after makes a range', cmd: ['org.timestamp.insert', 'org.timestamp.insert.inactive'], not: ['zed', 'helix'] },
      { text: 'While typing the date, the answer shows what it means (`=> <2026-10-09 Fri>`)', only: ['neovim', 'vscode'] },
      { text: 'In the calendar: moving by day, week and month, scrolling months, today, picking a day', in: 'calendar', cmd: ['org.calendar.day.later', 'org.calendar.week.later', 'org.calendar.month.later', 'org.calendar.scroll.later', 'org.calendar.today', 'org.calendar.select'], only: ['neovim', 'vscode'] },
      { text: 'In the calendar: picking a time with the keys, typing the answer instead, cancelling', in: 'calendar', cmd: ['org.calendar.time', 'org.calendar.type', 'org.calendar.cancel'], only: ['neovim', 'vscode'] },
      { text: 'Switching to a month of six weeks keeps the whole calendar in view, the cursor on the selected day', only: ['neovim', 'vscode'] },
      { text: 'After the date goes in, the cursor is on the same text and the window shows the same lines, with `scrolloff` set too', only: ['neovim'] },
      { text: '`calendar_win_mode` `float` and `cursor` open the calendar in a floating window; `settings = { calendar_months = 1 }` shows one month', only: ['neovim'] },
      { text: 'On a timestamp, changing the day, month, year, hour and minute under the cursor; with a count', cmd: ['org.timestamp.up', 'org.timestamp.down'] },
      { text: 'Moving a timestamp by days', cmd: ['org.date.later', 'org.date.earlier'] },
      { text: 'Changing the timestamp at the cursor with the date prompt', cmd: 'org.timestamp.change', not: ['zed', 'helix'] },
      { text: 'A repeater (`+1w`) and a warning (`-2d`) are kept as the date changes' },
    ],
  },
  {
    name: 'Agenda',
    emacs: [
      ['Day, week, fortnight, month and year views, going to a date', true],
      ['The list of TODO entries', true],
      ['Tag, property and group tag searches', true],
      ['Searches for words, phrases and regexps', true],
      ['Stuck projects', true],
      ['Custom commands and block agendas (`org-agenda-custom-commands`)', true],
      ['Filters by tag, category, effort, regexp and top headline', true],
      ['Bulk marks and actions', true],
      ['Follow mode and entry text', true],
      ['Log mode and the clock report', true],
      ['Archives mode', true],
      ['Changing an entry from the agenda: state, priority, tags, dates, notes, refile, archive, clock', true],
      ['Lines formatted as Org formats them (`org-agenda-prefix-format` and the rest)', true],
      ['The time grid', true],
      ['Habits (`org-habit`)', false],
      ['Diary entries and sexps (`%%(diary-…)`)', false],
      ['Column view in the agenda', false],
      ['Writing the agenda to a file (`C-x C-w`)', false],
      ['Appointment reminders (`org-agenda-to-appt`)', false],
    ],
    editors: {
      vscode: ['part', 'Tags are at column 80, not at the window\'s edge (`org_agenda_tags_column` `"auto"`): VS Code does not tell an extension how many columns an editor shows.'],
      zed: ['part', `The agenda is a file, its commands code actions (or keys through Zed's tasks); searches, going to a date and dates asked for are left out (${ZED_TEXT}), and follow mode (Zed reports no cursor moves to an extension).`],
      helix: ['no', 'Helix cannot open a document on a server\'s request. `org-lsp agenda FILE...` prints the agenda in a terminal.'],
    },
    checks: [
      { text: 'The agenda menu lists the views and the custom commands', cmd: 'org.agenda.dispatch', in: 'global' },
      { text: 'The week view: scheduled entries, deadlines with their warnings, the time grid', cmd: 'org.agenda.list', in: 'global' },
      { text: 'Later, earlier, today; day, week, month views; going to a date', in: 'agenda', cmd: ['org.agenda.later', 'org.agenda.earlier', 'org.agenda.today', 'org.agenda.day', 'org.agenda.week', 'org.agenda.month', 'org.agenda.goto.date'] },
      { text: 'Tags at the right edge of the window, following it as it is resized', only: ['neovim', 'vim', 'sublime'] },
      { text: 'The TODO list, a tag search (`work+urgent`), a word search', cmd: 'org.agenda.dispatch', in: 'global', not: ['zed'] },
      { text: 'Going to the line\'s entry in another window, and where the agenda is', in: 'agenda', cmd: ['org.agenda.goto.other', 'org.agenda.goto'] },
      { text: 'Changing the line\'s TODO state and priority; the line follows', in: 'agenda', cmd: ['org.todo', 'org.priority.up'] },
      { text: 'Scheduling the line\'s entry and setting its deadline', in: 'agenda', cmd: ['org.schedule', 'org.deadline'], not: ['zed'] },
      { text: 'Setting the line\'s tags, adding a note, refiling, archiving', in: 'agenda', cmd: ['org.tags.set', 'org.add.note', 'org.refile', 'org.archive.default'], not: ['zed'] },
      { text: 'Filters by tag, category and effort, and removing them', in: 'agenda', cmd: ['org.agenda.filter.tag', 'org.agenda.filter.category', 'org.agenda.filter.effort', 'org.agenda.filter.remove'], not: ['zed'] },
      { text: 'Marking lines and a bulk action on them', in: 'agenda', cmd: ['org.agenda.bulk.mark', 'org.agenda.bulk.action'] },
      { text: 'Follow mode shows the cursor\'s entry as the cursor moves', in: 'agenda', cmd: 'org.agenda.follow', not: ['zed'] },
      { text: 'Entry text, log mode, the clock report, archives mode', in: 'agenda', cmd: ['org.agenda.entry.text', 'org.agenda.log', 'org.agenda.clockreport', 'org.agenda.archives'] },
      { text: 'Hovering a line shows its entry', in: 'agenda', keys: { neovim: '`K`', vim: '`K`', vscode: 'the mouse, `Ctrl+K Ctrl+I`', sublime: 'the mouse', zed: 'the mouse' } },
      { text: 'Rebuilding the agenda, saving the changed files, quitting', in: 'agenda', cmd: ['org.agenda.redo', 'org.agenda.save'] },
      { text: 'An entry changed in its file shows in the agenda when it is rebuilt' },
      { text: 'The agenda window opens as `win_split_mode` says (`split`, `vsplit`, `float`, `current`)', only: ['neovim'] },
      { text: 'The agenda opens as `org-lsp.winSplitMode` says', only: ['vscode'] },
    ],
  },
  {
    name: 'Clocking',
    emacs: [
      ['Clocking in and out, cancelling, going to the clocked entry', true],
      ['The clock history and clocking in the last entry', true],
      ['The clock in the mode line', true],
      ['The time clocked under each headline (`C-c C-x C-d`)', true],
      ['Clock tables, with every parameter', true],
      ['Resolving dangling clocks and idle time', true],
      ['Clocking from the agenda, the clocked entry highlighted', true],
      ['A notice when the clock passes the effort', false],
    ],
    editors: {
      zed: ['part', 'No clock in the status line: Zed has no place there for an extension\'s text.'],
      helix: ['part', 'No clock in the status line: Helix has no place there for a server\'s text.'],
    },
    checks: [
      { text: 'Clocking in: `:LOGBOOK:` gets a `CLOCK:` line and the clock shows (`[0:01] (Task)`)', cmd: 'org.clock.in' },
      { text: 'Clocking out writes the end and the duration', cmd: 'org.clock.out' },
      { text: 'Cancelling the clock takes its line out', cmd: 'org.clock.cancel' },
      { text: 'Going to the clocked entry from another file', cmd: 'org.clock.goto', in: 'global' },
      { text: 'Clocking in the last entry again', cmd: 'org.clock.in.last' },
      { text: 'The time clocked under each headline shows next to it', cmd: 'org.clock.display' },
      { text: 'A clock table: inserted, then updated; shifting its `:block` on the `#+BEGIN:` line', cmd: ['org.clock.report', 'org.dblock.update'] },
      { text: 'A clock left running when the editor closed is offered for resolving on the next start', cmd: 'org.clock.resolve' },
      { text: 'In the agenda, clocking in and out; the clocked line is highlighted from edge to edge', in: 'agenda', cmd: ['org.clock.in', 'org.clock.out'] },
      { text: 'The clock in the statusline (`clock_status()`)', only: ['neovim'] },
      { text: 'The clock in the statusline (`org_lsp#clock_status()`)', only: ['vim'] },
      { text: 'The clock in the status bar', only: ['vscode', 'sublime'] },
    ],
  },
  {
    name: 'Capture',
    emacs: [
      ['Templates of every type: entries, items, checkboxes, table lines, plain text', true],
      ['Targets: files, headlines, outline paths, regexps, date trees, IDs, the clock', true],
      ['Prompts and `%`-escapes (`%^{…}`, `%t`, `%a`, `%i`, `%?`…)', true],
      ['The capture buffer: finishing, aborting, refiling', true],
      ['Clock options (`:clock-in`, `:clock-resume`…)', true],
      ['`function` targets', false],
      ['`%(…)` (Emacs Lisp)', false],
      ['`%[file]`', false],
      ['`%c`, `%x`, `%:keyword`', false],
      ['Template contexts', false],
    ],
    editors: {
      zed: ['part', `Templates with prompts are left out, and there is no capture buffer to type in: ${ZED_TEXT}.`],
      helix: ['no', `Helix cannot open the capture buffer, and ${HELIX_TEXT}.`],
    },
    checks: [
      { text: 'The menu lists the templates by key', cmd: 'org.capture', in: 'global' },
      { text: 'An entry template opens the capture buffer with the cursor at `%?`', cmd: 'org.capture', in: 'global', not: ['zed'] },
      { text: 'A `%^{Name}` prompt asks for text', cmd: 'org.capture', in: 'global', not: ['zed'] },
      { text: 'Finishing puts the entry under its target', in: 'capture', cmd: 'org.capture.finish', not: ['zed'] },
      { text: 'Aborting leaves the target unchanged', in: 'capture', cmd: 'org.capture.abort', not: ['zed'] },
      { text: 'Refiling from the capture buffer', in: 'capture', cmd: 'org.capture.refile', not: ['zed'] },
      { text: 'A date tree target files under year, month and day', cmd: 'org.capture', in: 'global' },
      { text: '`%a` links back to where the capture started', cmd: 'org.capture', in: 'global' },
      { text: 'A template without prompts goes in at once', cmd: 'org.capture', in: 'global', only: ['zed'] },
    ],
  },
  {
    name: 'Refile and archive',
    emacs: [
      ['Refiling to a heading, between files too (`org-refile-targets`)', true],
      ['Archiving a subtree to its archive file', true],
      ['Archiving to the archive sibling', true],
      ['Toggling the `ARCHIVE` tag', true],
      ['Archive locations, `ARCHIVE` properties, date trees, `ARCHIVE_…` context', true],
      ['Archiving all done subtrees', true],
      ['Logging refiles (`org-log-refile`)', false],
      ['Creating parent nodes as the target', false],
      ['Refiling a region', false],
      ['The prefix arguments (going to the target, copying)', false],
      ['`org-refile-target-verify-function`', false],
    ],
    editors: {
      helix: ['part', 'Refiling is left out: it asks for a target from a list, which Helix cannot show for a server. Archiving works as code actions.'],
    },
    checks: [
      { text: 'Refiling a subtree under a heading of the same file, and of another file', cmd: 'org.refile', not: ['helix'] },
      { text: 'Archiving a subtree: it goes to `FILE_archive` with its `ARCHIVE_…` properties', cmd: 'org.archive.subtree' },
      { text: 'Archiving to the archive sibling', cmd: 'org.archive.sibling' },
      { text: 'Toggling the `ARCHIVE` tag', cmd: 'org.archive.tag' },
      { text: 'Archiving every done subtree after confirming each', cmd: 'org.archive.done' },
      { text: 'In the agenda: archiving the line\'s entry; archives mode shows archived entries', in: 'agenda', cmd: ['org.archive.default', 'org.agenda.archives'], not: ['helix'] },
    ],
  },
  {
    name: 'Links',
    emacs: [
      ['Following links: headlines, targets, custom IDs, files with searches, IDs, URLs, timestamps, tags', true],
      ['Storing and inserting links, with descriptions, editing the link at the cursor', true],
      ['IDs created (`org-id-get-create`)', true],
      ['Completion of file names, headlines and custom IDs in links', true],
      ['Backlinks, hovers and clickable links (beyond Emacs: references, document links)', true],
      ['Link abbreviations (`#+LINK:`, `org-link-abbrev-alist`)', false],
      ['A region as the search of a stored link', false],
      ['Completing `id:` and `help:` links', false],
      ['Moving to the next and previous link (`C-c C-x C-n`, `C-c C-x C-p`)', false],
    ],
    editors: {
      zed: ['part', `Storing and inserting links are left out: they ask for text, and ${ZED_TEXT}. Following works through go to definition, for links inside Org files and to files.`],
      helix: ['part', `Storing and inserting links are left out: they ask for text, and ${HELIX_TEXT}. Following works through go to definition.`],
    },
    checks: [
      { text: 'Following a link to a headline, a custom ID and a `<<target>>`', cmd: 'org.open' },
      { text: 'Following a `file:` link with a search (`file:notes.org::*Heading`) and an `id:` link', cmd: 'org.open' },
      { text: 'Following a URL opens the browser; a timestamp opens its day\'s agenda', cmd: 'org.open', not: ['zed', 'helix'] },
      { text: 'Go to definition on a link does the same', keys: { neovim: '`gd` or `vim.lsp.buf.definition()`', vim: 'the client\'s go to definition', vscode: '`F12`', sublime: '*LSP: Goto Definition*', zed: '`F12`', helix: '`gd`' } },
      { text: 'Storing a link to a headline and inserting it elsewhere with a description', cmd: ['org.link.store', 'org.link.insert'], not: ['zed', 'helix'] },
      { text: 'Inserting a link on an existing link edits it', cmd: 'org.link.insert', not: ['zed', 'helix'] },
      { text: 'Creating an ID for the entry', cmd: 'org.id.create' },
      { text: 'In `[[`, file names, headlines and custom IDs are completed' },
      { text: 'Hovering a link shows where it leads' },
      { text: 'References on a headline list the links to it, in every file' },
      { text: 'Renaming a headline updates the links that search for it' },
      { text: 'Workspace symbols find headlines of every file' },
    ],
  },
  {
    name: 'Footnotes',
    emacs: [
      ['Following a reference to its definition and back', true],
      ['Highlighting and checking (`org-lint`)', true],
      ['Creating a footnote (`C-c C-x f`)', false],
      ['Renumbering, sorting and normalizing footnotes', false],
    ],
    checks: [
      { text: 'Following `[fn:1]` goes to its definition, and from the definition back to the reference', cmd: 'org.open' },
      { text: 'A reference without a definition is reported' },
    ],
  },
  {
    name: 'Tables',
    emacs: [
      ['Aligning (`C-c C-c`, formatting)', true],
      ['Moving between fields (`TAB`, `S-TAB`), a row made past the last', true],
      ['Moving, inserting and deleting rows and columns', true],
      ['Rules (`C-c -`, `C-c RET`)', true],
      ['Blanking a field', true],
      ['Sorting rows', true],
      ['The `#+TBLFM:` line following moved columns and rows', true],
      ['Rectangles: cutting, copying and pasting', false],
      ['Copying down (`S-RET`)', false],
      ['Editing a field in its own buffer (`C-c \\``)', false],
      ['Shrunk columns (`<5>`)', false],
      ['Making a table from a region (`C-c |`)', false],
      ['Summing (`C-c +`) and field information (`C-c ?`)', false],
      ['Moving the field with the shifted cursor keys', false],
      ['Importing and exporting tables', false],
      ['Plotting (`org-plot`)', false],
    ],
    editors: {
      zed: ['part', `Moving between fields with \`TAB\` is left out: ${NO_KEY_ZED}, and it needs one. The other edits are code actions in a table.`],
      helix: ['part', `Moving between fields with \`TAB\` is left out: ${NO_KEY_HELIX}, and it needs one. The other edits are code actions in a table.`],
    },
    checks: [
      { text: 'Formatting the file aligns the tables', keys: { neovim: '`vim.lsp.buf.format()`', vim: 'the client\'s format command', vscode: 'Format Document', sublime: '*LSP: Format File*', zed: 'editor: format', helix: '`:format`' } },
      { text: 'Moving to the next and previous field; past the last field a row is made', cmd: ['org.table.next.field', 'org.table.previous.field'], not: ['zed', 'helix'] },
      { text: 'Moving a column left and right, a row up and down', cmd: ['org.meta.left', 'org.meta.right', 'org.meta.up', 'org.meta.down'] },
      { text: 'Inserting and deleting a column and a row', cmd: ['org.shiftmeta.right', 'org.shiftmeta.left', 'org.shiftmeta.down', 'org.shiftmeta.up'] },
      { text: 'A rule below the row; a rule and a move to the row below', cmd: ['org.table.hline', 'org.table.hline.move'] },
      { text: 'Blanking the field', cmd: 'org.table.blank.field' },
      { text: 'Sorting the rows by the cursor\'s column', cmd: 'org.sort' },
      { text: 'The `#+TBLFM:` line follows a moved column' },
    ],
  },
  {
    name: 'Table formulas',
    emacs: [
      ['Column, field and range formulas', true],
      ['References by row, column, hline and name', true],
      ['Parameters and constants', true],
      ['Calc\'s arithmetic, vectors, dates and durations', true],
      ['Modes and formats (`;%.2f`, `;N`, `;T`)', true],
      ['Recalculating the line, the table, until stable', true],
      ['Formulas in Emacs Lisp', false],
      ['Remote references', false],
      ['Units', false],
      ['Calc\'s fractions and symbolic mode', false],
      ['The formula editor (`C-c \'`)', false],
      ['The formula debugger', false],
    ],
    checks: [
      { text: '`#+TBLFM: $3=$1*$2` fills the third column', cmd: 'org.table.recalculate' },
      { text: 'Recalculating the cursor\'s line only', cmd: 'org.table.recalculate.line' },
      { text: '`C-c C-c` on the `#+TBLFM:` line applies it', cmd: 'org.checkbox.toggle' },
      { text: 'A range sum `@>$2=vsum(@2..@-1)` and a format `;%.2f`' },
      { text: 'Date arithmetic (`$2=$1+7`) on timestamps' },
      { text: 'Iterating until the table is stable', cmd: 'org.table.iterate' },
    ],
  },
  {
    name: 'Code blocks (Babel)',
    emacs: [
      ['Tangling, with Noweb references, comments and header arguments from properties', true],
      ['Evaluating a block, a `#+CALL:` line and inline code (through Emacs)', true],
      ['Evaluating the file\'s and the subtree\'s blocks', true],
      ['Removing results', true],
      ['Sessions', true],
      ['Asking before evaluating (`org-confirm-babel-evaluate`)', true],
      ['Editing a block in its own buffer (`C-c \'`)', false],
      ['Tangling\'s Emacs Lisp (evaluated header arguments, `<<block()>>`)', false],
      ['Detangling', false],
      ['Moving between blocks (`C-c C-v n`, `C-c C-v p`, `C-c C-v g`)', false],
      ['The Library of Babel (`C-c C-v i`)', false],
    ],
    editors: {
      zed: ['part', 'Only tangling and the block at the cursor are offered (code actions in a block): the file\'s and the subtree\'s evaluation and removing results need keys, which Zed cannot give a server.'],
      helix: ['part', 'Only tangling and the block at the cursor are offered (code actions in a block): the file\'s and the subtree\'s evaluation and removing results need keys, which Helix cannot give a server.'],
    },
    checks: [
      { text: 'Tangling writes each block to its `:tangle` file, `:mkdirp` and `:shebang` applied', cmd: 'org.babel.tangle' },
      { text: 'Evaluating an `emacs-lisp` block asks first (yes or no), then puts `#+RESULTS:` below it (needs Emacs)', cmd: 'org.babel.execute' },
      { text: '`C-c C-c` in the block does the same', cmd: 'org.checkbox.toggle', not: ['zed', 'helix'] },
      { text: '`src_emacs-lisp{(* 6 7)}` gets `{{{results(=42=)}}}`; a `#+CALL:` line its results', cmd: 'org.babel.execute' },
      { text: 'Evaluating the file\'s and the subtree\'s blocks', cmd: ['org.babel.execute.buffer', 'org.babel.execute.subtree'], not: ['zed', 'helix'] },
      { text: 'Removing a block\'s results', cmd: 'org.babel.remove_result', not: ['zed', 'helix'] },
      { text: 'Typing in the file while a slow block runs: the results are dropped with a warning' },
    ],
  },
  {
    name: 'Export and publishing',
    emacs: [
      ['The export menu, with the backends Emacs has (through Emacs)', true],
      ['Body only and the subtree at the cursor', true],
      ['To a buffer, to a file, and opened', true],
      ['Publishing a file, a project, all projects', true],
      ['Forcing publishing', true],
      ['Exporting in the background (`C-a`)', false],
      ['Only the visible part (`C-v`)', false],
      ['The export stack (`&`)', false],
      ['Inserting a template (`#`)', false],
    ],
    editors: {
      zed: ['no', 'Not offered as a code action yet; its menu would come as buttons.'],
      helix: ['no', 'Not offered as a code action yet.'],
    },
    checks: [
      { text: 'The menu shows Emacs\'s backends, `C-b`, `C-s` and `C-f` toggle the options (needs Emacs)', cmd: 'org.export.dispatch' },
      { text: 'To an HTML file: the file is written next to the Org file and announced', cmd: 'org.export.dispatch' },
      { text: 'To an HTML file and opened: the browser opens it', cmd: 'org.export.dispatch' },
      { text: 'The subtree at the cursor as UTF-8 text in a buffer: a file of it is shown', cmd: 'org.export.dispatch' },
      { text: 'Unsaved changes are in the export', cmd: 'org.export.dispatch' },
      { text: '`emacs_config` adds Markdown (`org-export-backends`): it is in the menu after a restart' },
    ],
  },
  {
    name: 'Citations',
    emacs: [
      ['Bibliographies from `#+BIBLIOGRAPHY:` and `org_cite_global_bibliography` (BibTeX, CSL-JSON)', true],
      ['Keys completed', true],
      ['A key\'s entry on hover, following a key', true],
      ['Unknown keys reported', true],
      ['Inserting, editing and deleting citations (`C-c C-x @`)', true],
    ],
    editors: {
      zed: ['part', 'Inserting citations is not offered as a code action yet. Completion, hover and following work.'],
      helix: ['part', 'Inserting citations is not offered as a code action yet. Completion, hover and following work.'],
    },
    checks: [
      { text: 'Keys are completed after `[cite:@`' },
      { text: 'Hovering a key shows its entry; following it opens the bibliography there' },
      { text: 'An unknown key is reported, with close keys to put instead' },
      { text: 'Inserting a citation, picking keys; on a citation, adding a key; with the prefix, deleting one', cmd: ['org.cite.insert', 'org.cite.insert.prefix'], not: ['zed', 'helix'] },
    ],
  },
  {
    name: 'Diagnostics (org-lint)',
    emacs: [['org-lint\'s checkers', 59, 61]],
    checks: [
      { text: 'A heading in a paragraph (`text ** heading`), a link to nowhere and a block left open are reported' },
      { text: 'An unknown header argument (`:foo 1`) on a source block is reported, as information' },
      { text: '`org_lint_ignore = ["link-to-local-file"]` silences that checker' },
    ],
  },
  {
    name: 'Indentation and formatting',
    emacs: [
      ['Virtual indentation (`org-indent-mode`, `#+STARTUP: indent`)', true],
      ['Hidden leading stars (`org-hide-leading-stars`)', true],
      ['Hard indentation (`org-adapt-indentation`)', true],
      ['Indenting lines and regions (`org-indent-line`, `org-indent-region`)', true],
      ['Aligning tables and tags', true],
      ['Lines a long line wraps into indented (`wrap-prefix`)', false],
    ],
    editors: {
      vim: ['part', 'New lines are not indented by the server (no `indentexpr`), and wrapped lines are not.'],
      sublime: ['part', 'Virtual indentation shows as phantoms, the leading stars still showing; new lines are not indented as they are typed.'],
      zed: ['part', 'Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed.'],
      helix: ['part', 'Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed.'],
    },
    checks: [
      { text: 'A file with `#+STARTUP: indent` shows its text indented by level, the file unchanged' },
      { text: 'With `#+STARTUP: hidestars` only the last star shows', not: ['sublime', 'zed', 'helix'] },
      { text: 'With `org_adapt_indentation = true`, a new line below a headline is indented', not: ['vim', 'sublime', 'zed', 'helix'] },
      { text: 'Re-indenting a selection indents it as Org does', keys: { neovim: '`=`', vscode: 'Format Selection', sublime: '*LSP: Format Selection*', zed: 'format selections', helix: '`=`' }, not: ['vim'] },
      { text: 'Formatting the file aligns tables and puts tags in their column' },
    ],
  },
  {
    name: 'Notes and logging',
    emacs: [
      ['Notes (`org-add-note`), in the log or the `LOGBOOK` drawer', true],
      ['Notes and times on state changes', true],
      ['`LOG_INTO_DRAWER` and the `LOGGING` property', true],
      ['Logging rescheduled dates and deadlines (`org-log-reschedule`, `org-log-redeadline`)', false],
    ],
    editors: {
      zed: ['no', `A note is typed text: ${ZED_TEXT}.`],
      helix: ['no', `A note is typed text: ${HELIX_TEXT}.`],
    },
    checks: [
      { text: 'Adding a note: a note buffer opens, and finishing writes it in the entry\'s log', cmd: 'org.add.note' },
      { text: 'With `LOG_INTO_DRAWER`, the note goes in `:LOGBOOK:`', cmd: 'org.add.note' },
      { text: 'Adding a note from the agenda', in: 'agenda', cmd: 'org.add.note' },
    ],
  },
  {
    name: 'Completion and editing helpers',
    emacs: [
      ['Completion: TODO keywords, tags, `#+` keywords and `#+STARTUP:` values, blocks, drawers, properties, entities, link targets', true],
      ['Structure templates (`C-c C-,`)', false],
      ['Emphasis (`C-c C-x C-f`)', false],
      ['Inserting a drawer (`C-c C-x d`)', false],
    ],
    editors: {
      neovim: ['yes', 'Completion needs an LSP completion plugin (or `vim.lsp.completion`).'],
    },
    checks: [
      { text: 'At the start of a headline, TODO keywords are completed' },
      { text: '`#+` keywords, and the values after `#+STARTUP:`' },
      { text: 'Block names after `#+begin_`, drawer names after `:`' },
      { text: 'Entities after `\\` (`\\alp` → `\\alpha`)' },
    ],
  },
  { name: 'Timers', emacs: [['Relative timers and countdowns (`org-timer`)', false], ['Timer items in lists', false]] },
  { name: 'Attachments', emacs: [['Attaching files to an entry (`C-c C-a`)', false], ['`attachment:` links', false]] },
  { name: 'Encryption', emacs: [['Encrypting and decrypting entries (`org-crypt`)', false]] },
  { name: 'Inline images and LaTeX previews', emacs: [['Inline images (`C-c C-x C-v`)', false], ['LaTeX previews (`C-c C-x C-l`)', false]] },
];

/** The share of an area's Emacs features done, as a whole percentage. */
export function parity(area) {
  let done = 0;
  let total = 0;
  for (const [, state, of] of area.emacs) {
    if (typeof state === 'number') {
      done += state;
      total += of;
    } else {
      done += state ? 1 : 0;
      total += 1;
    }
  }
  return { done, total, percent: Math.round((100 * done) / total) };
}

/** The parity of all of Org: the mean of the areas', each area counting
 * the same however many features it lists. */
export function overall() {
  return Math.round(AREAS.reduce((sum, area) => sum + parity(area).percent, 0) / AREAS.length);
}

/** How the editor `id` does with `area`: [status, why]. */
export function support(area, id) {
  if (parity(area).done === 0) return ['none', ''];
  return area.editors?.[id] ?? ['yes', ''];
}
