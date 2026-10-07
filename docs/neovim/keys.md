# Keys in Neovim

The plugin does not keep a list of keys of its own. When the server starts,
it announces its commands with their default keys (`org-lsp manifest`
prints the list), and the plugin binds the keys of the `vim` preset, which
are nvim-orgmode's defaults. Keys for Org files, the agenda, the date
prompt and capture buffers are local to those buffers; the keys listed
under *Anywhere* are global.

`:nmap <buffer>` in an Org buffer lists what is bound there, each key
described by the title of its command.

The other options are in [configuration.md](configuration.md).

## The keys

<!-- The tables below are written by scripts/generate-docs.mjs from the
     server's manifest. Edit the server's table in
     server/org-lsp/src/commands.rs, not the tables. -->

<!-- BEGIN generated: keys -->

### Anywhere

| Keys | Command | What it does |
| --- | --- | --- |
| `<Leader>oa` | `org.agenda.dispatch` | Agenda commands |
| `<Leader>oxj` | `org.clock.goto` | Go to the clocked entry |
| `<Leader>oc` | `org.capture` | Capture |

### In Org files

| Keys | Command | What it does |
| --- | --- | --- |
| `<Leader>oxi` | `org.clock.in` | Clock in |
| `<Leader>oxx` | `org.clock.in.last` | Clock in the last clocked task |
| `<Leader>oxR` | `org.clock.resolve` | Resolve a dangling clock |
| `<Leader>oxo` | `org.clock.out` | Clock out |
| `<Leader>oxq` | `org.clock.cancel` | Cancel the clock |
| `<Leader>oxd` | `org.clock.display` | Show the time clocked under each headline |
| `<Leader>oxr` | `org.clock.report` | Insert or update a clock table |
| `<Leader>oxu` | `org.dblock.update` | Update the dynamic block |
| `cit` | `org.todo` | Change the TODO state |
| `ciT` | `org.todo.previous` | Cycle TODO state backward |
| `ciR` | `org.priority.up` | Raise the priority |
| `cir` | `org.priority.down` | Lower the priority |
| `<Leader>o,` | `org.priority` | Set the priority |
| `<Leader>oxp` | `org.property.set` | Set a property of the entry |
| `<Leader>oxe` | `org.effort.set` | Set the effort of the entry |
| `<Leader>oxE` | `org.effort.increase` | Set the effort to the next allowed value |
| `<Leader>oic` | `org.cite.insert` | Insert a citation, or edit the one at the cursor |
| `<Leader>oiC` | `org.cite.insert.prefix` | Delete the citation or reference at the cursor, or insert one with a style |
| `<Leader>ot` | `org.tags.set` | Set the tags of the headline |
| `<C-Space>` | `org.checkbox.toggle` | Toggle the checkbox |
| `<CR>`¹ (insert mode) | `org.return` | New line, as Org's RET makes it |
| `<S-CR>`¹ (insert mode) | `org.return.item` | New line; on a list item, the next item |
| `<Leader><CR>` | `org.meta.return` | New heading, item or table row |
| `<Leader>oiT` | `org.insert.todo.heading` | New TODO heading, or item with a checkbox |
| `<Leader>oih` | `org.insert.heading.after` | New heading after the subtree |
| `<Leader>oit` | `org.insert.todo.heading.after` | New TODO heading after the subtree |
| `<<`¹ | `org.meta.left` | Promote the heading; outdent the item; move the table column left |
| `>>`¹ | `org.meta.right` | Demote the heading; indent the item, the drawer or the block; move the table column right |
| `<s` | `org.shiftmeta.left` | Promote the subtree; outdent the item with its children; delete the table column |
| `>s` | `org.shiftmeta.right` | Demote the subtree; indent the item with its children; insert a table column |
| `<Leader>oK` | `org.meta.up` | Move the subtree, the item or the table row up |
| `<Leader>oJ` | `org.meta.down` | Move the subtree, the item or the table row down |
| `<Leader>oTd` | `org.shiftmeta.up` | Delete the table row |
| `<Leader>oTO` | `org.shiftmeta.down` | Insert a table row above |
| `<Leader>oTo` | `org.table.insert.row.below` | Insert a table row below |
| `<Leader>o-` | `org.table.hline` | Insert a rule below the table row |
| `<Leader>o<CR>` | `org.table.hline.move` | Insert a rule below the table row and go below it |
| `<Leader>o<Space>` | `org.table.blank.field` | Blank the table field |
| `<Leader>o^` | `org.sort` | Sort the entries, the items or the table rows |
| `<Leader>o/` | `org.sparse.tree` | Show a sparse tree |
| `<Leader>osd` | `org.cut.subtree` | Cut the subtree |
| `<Leader>osy` | `org.copy.subtree` | Copy the subtree |
| `<Leader>osp` | `org.paste.subtree` | Paste the subtree |
| `<Leader>or` | `org.refile` | Refile the entry |
| `<Leader>ona` | `org.add.note` | Add a note to the entry |
| `<Leader>obt` | `org.babel.tangle` | Tangle the file's code blocks |
| `<Leader>o*` | `org.table.recalculate` | Recalculate the table |
| `<Leader>oo` | `org.open` | Open the link or thing at the cursor |
| `<Leader>o$` | `org.archive.subtree` | Archive the subtree |
| `<Leader>oxA` | `org.archive.sibling` | Archive the subtree under its Archive sibling |
| `<Leader>oA` | `org.archive.tag` | Toggle the ARCHIVE tag |
| `<C-a>`¹ | `org.timestamp.up` | Increase the part of the timestamp at the cursor |
| `<C-x>`¹ | `org.timestamp.down` | Decrease the part of the timestamp at the cursor |
| `<S-Up>`¹ | `org.date.later` | Move the date one day later |
| `<S-Down>`¹ | `org.date.earlier` | Move the date one day earlier |
| `<S-Left>`¹ | `org.shift.left` | Move the date a day earlier, or cycle the TODO state or the list's bullets backward |
| `<S-Right>`¹ | `org.shift.right` | Move the date a day later, or cycle the TODO state or the list's bullets forward |
| `<Leader>ois` | `org.schedule` | Schedule the entry |
| `<Leader>oid` | `org.deadline` | Set the deadline of the entry |
| `<Leader>oi.` | `org.timestamp.insert` | Insert or change a timestamp |
| `<Leader>oi!` | `org.timestamp.insert.inactive` | Insert or change an inactive timestamp |
| `cid` | `org.timestamp.change` | Change the timestamp at the cursor |

### In a capture buffer

| Keys | Command | What it does |
| --- | --- | --- |
| `<C-c>` | `org.capture.finish` | Capture: finish |
| `<Leader>ok` | `org.capture.abort` | Capture: abort |
| `<Leader>or` | `org.capture.refile` | Capture: refile the entry |

### In the agenda

| Keys | Command | What it does |
| --- | --- | --- |
| `f` | `org.agenda.later` | Agenda: go forward in time |
| `b` | `org.agenda.earlier` | Agenda: go backward in time |
| `.` | `org.agenda.today` | Agenda: go to today |
| `[` | `org.agenda.query.add` | Agenda: search for another word |
| `]` | `org.agenda.query.subtract` | Agenda: search without a word |
| `{` | `org.agenda.query.add.regexp` | Agenda: search for another regexp |
| `}` | `org.agenda.query.subtract.regexp` | Agenda: search without a regexp |
| `J` | `org.agenda.goto.date` | Agenda: go to a date |
| `vd` | `org.agenda.day` | Agenda: day view |
| `vw` | `org.agenda.week` | Agenda: week view |
| `vt` | `org.agenda.fortnight` | Agenda: fortnight view |
| `vm` | `org.agenda.month` | Agenda: month view |
| `vy` | `org.agenda.year` | Agenda: year view |
| `r` | `org.agenda.redo` | Agenda: rebuild |
| `<CR>` | `org.agenda.goto` | Agenda: go to the entry |
| `<Tab>` | `org.agenda.goto.other` | Agenda: go to the entry in another window |
| `s`, `<C-x><C-s>` | `org.agenda.save` | Save all Org files |
| `vl` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `R`, `vR` | `org.agenda.clockreport` | Agenda: the clock report |
| `E`, `vE` | `org.agenda.entry.text` | Agenda: entry text mode, some text of each entry under its line |
| `F` | `org.agenda.follow` | Agenda: follow mode, the cursor's entry in another window |
| `m` | `org.agenda.bulk.mark` | Agenda: mark the entry for a bulk action |
| `u` | `org.agenda.bulk.unmark` | Agenda: unmark the entry |
| `U` | `org.agenda.bulk.unmark.all` | Agenda: unmark all entries |
| `<M-m>` | `org.agenda.bulk.toggle` | Agenda: toggle the mark of the entry |
| `*` | `org.agenda.bulk.mark.all` | Agenda: mark all entries |
| `<M-*>` | `org.agenda.bulk.toggle.all` | Agenda: toggle the marks of all entries |
| `%` | `org.agenda.bulk.mark.regexp` | Agenda: mark the entries matching a regexp |
| `B` | `org.agenda.bulk.action` | Agenda: an action on the marked entries |
| `/` | `org.agenda.filter` | Agenda: filter by tags, categories, effort and regexps |
| `\` | `org.agenda.filter.tag` | Agenda: filter by tag |
| `<` | `org.agenda.filter.category` | Agenda: filter by the line's category |
| `=` | `org.agenda.filter.regexp` | Agenda: filter by a regexp |
| `_` | `org.agenda.filter.effort` | Agenda: filter by effort |
| `^` | `org.agenda.filter.top.headline` | Agenda: filter by the line's top headline |
| `\|` | `org.agenda.filter.remove` | Agenda: remove all filters |
| `va` | `org.agenda.archives` | Agenda: include archived trees |
| `vA` | `org.agenda.archives.files` | Agenda: include archived trees and archive files |
| `I`, `<Leader>oxi` | `org.clock.in` | Clock in |
| `O`, `<Leader>oxo` | `org.clock.out` | Clock out |
| `X`, `<Leader>oxq` | `org.clock.cancel` | Cancel the clock |
| `t` | `org.todo` | Change the TODO state |
| `+`, `<S-Up>` | `org.priority.up` | Raise the priority |
| `-`, `<S-Down>` | `org.priority.down` | Lower the priority |
| `,`, `<Leader>o,` | `org.priority` | Set the priority |
| `<C-c><C-x>p`, `<Leader>oxp` | `org.property.set` | Set a property of the entry |
| `e`, `<Leader>oxe` | `org.effort.set` | Set the effort of the entry |
| `<Leader>ot` | `org.tags.set` | Set the tags of the headline |
| `<C-c><C-w>`, `<Leader>or` | `org.refile` | Refile the entry |
| `z`, `<Leader>ona` | `org.add.note` | Add a note to the entry |
| `<C-c><C-o>`, `<Leader>oo` | `org.open` | Open the link or thing at the cursor |
| `$`, `<C-c><C-x><C-s>`, `<C-c>$`, `<Leader>o$` | `org.archive.subtree` | Archive the subtree |
| `<C-c><C-x>A`, `<Leader>oxA` | `org.archive.sibling` | Archive the subtree under its Archive sibling |
| `<C-c><C-x>a`, `<Leader>oA` | `org.archive.tag` | Toggle the ARCHIVE tag |
| `<C-c><C-x><C-a>` | `org.archive.default` | Archive with the default command |
| `a` | `org.archive.confirm` | Archive with the default command, once confirmed |
| `<S-Right>` | `org.date.later` | Move the date one day later |
| `<S-Left>` | `org.date.earlier` | Move the date one day earlier |
| `<C-c><C-s>`, `<Leader>ois` | `org.schedule` | Schedule the entry |
| `<C-c><C-d>`, `<Leader>oid` | `org.deadline` | Set the deadline of the entry |

### In the date prompt

| Keys | Command | What it does |
| --- | --- | --- |
| `l`, `<S-Right>` | `org.calendar.day.later` | Date prompt: one day later |
| `h`, `<S-Left>` | `org.calendar.day.earlier` | Date prompt: one day earlier |
| `j`, `<S-Down>` | `org.calendar.week.later` | Date prompt: one week later |
| `k`, `<S-Up>` | `org.calendar.week.earlier` | Date prompt: one week earlier |
| `<M-S-Right>` | `org.calendar.month.later` | Date prompt: one month later |
| `<M-S-Left>` | `org.calendar.month.earlier` | Date prompt: one month earlier |
| `<M-S-Down>` | `org.calendar.year.later` | Date prompt: one year later |
| `<M-S-Up>` | `org.calendar.year.earlier` | Date prompt: one year earlier |
| `>` | `org.calendar.scroll.later` | Date prompt: show the next months |
| `<` | `org.calendar.scroll.earlier` | Date prompt: show the previous months |
| `.` | `org.calendar.today` | Date prompt: go to today |
| `<CR>` | `org.calendar.select` | Date prompt: take the day at the cursor |
| `i` | `org.calendar.type` | Date prompt: type the date |
| `t` | `org.calendar.time` | Date prompt: pick the time, or go back to the day |
| `q`, `<Esc>` | `org.calendar.cancel` | Date prompt: cancel |

### Without a default key

| Command | What it does |
| --- | --- |
| `org.agenda.list` | Agenda for the current week or day |
| `org.agenda.todos` | List of all TODO entries |
| `org.todo.next` | Cycle TODO state forward |
| `org.property.delete` | Delete a property of the entry |
| `org.property.delete.globally` | Delete a property from every entry of the file |
| `org.ordered.toggle` | Toggle whether subtasks must be done in order (ORDERED) |
| `org.table.next.field` | Go to the next table field, making a row after the last |
| `org.table.previous.field` | Go to the previous table field |
| `org.table.hline.above` | Insert a rule above the table row |
| `org.table.recalculate.line` | Recalculate the table's current line |
| `org.table.iterate` | Recalculate the table until it does not change |
| `org.archive.done` | Archive the subtrees without open TODO items |
| `org.archive.tag.done` | Tag the subtrees without open TODO items as archived |
| `org.date.later.hours` | Agenda: move the time one hour later |
| `org.date.earlier.hours` | Agenda: move the time one hour earlier |
| `org.date.later.minutes` | Agenda: move the time later by the rounding minutes |
| `org.date.earlier.minutes` | Agenda: move the time earlier by the rounding minutes |
| `org.shift.up` | Increase the timestamp part, raise the priority, or go to the previous item |
| `org.shift.down` | Decrease the timestamp part, lower the priority, or go to the next item |

¹ Where the command does not apply, the key does what it does in any
buffer: `<C-a>` off a timestamp still increments the number under the
cursor, and `<CR>` in a source block's code is Vim's own.

<!-- END generated -->

The plugin adds keys of its own, which only an editor can carry out:

| Keys | Name in `mappings` | Scope | What it does |
| --- | --- | --- | --- |
| `<Tab>` | `cycle` | `org` | Cycle the visibility of the subtree at the cursor |
| `<S-Tab>` | `global_cycle` | `org` | Cycle the visibility of the whole file |
| `q` | `quit` | `agenda` | Close the agenda |
| `K` | `hover` | `agenda` | Show the entry of the line in a float, highlighted |
| `g?` | `help` | `org`, `agenda` | Show the keys of the buffer in a window (`q` closes it) |

A capture buffer is an Org file, so it has the keys of Org files as well
as its own; there `<Leader>or` refiles the captured entry rather than the
subtree at the cursor.

## How they behave

- **Counts** work as in Vim: `3<C-a>` adds three to the part of the
  timestamp under the cursor, `2<S-Right>` moves an agenda entry two days.
- **Repeat**: every key in Org files changes the text, and `.` repeats the
  last of them, with a new count if you type one (`.`, `3.`). A command
  that asks for something asks again.
- **Keys with a meaning of their own** (marked ¹) give it back where the
  command does not apply, so `<C-a>`, `<C-x>`, `<S-Up>` and `<S-Down>`
  still work on ordinary numbers and lines.
- **RET and a completion menu**: while one is open (Vim's, nvim-cmp's or
  blink.cmp's), `<CR>` is the menu's; `<S-CR>` closes it and runs its
  command.
- **`<S-CR>`** is RET that continues lists: on the line of a list item's
  bullet it makes the next item, numbered and with a checkbox as M-RET
  makes it, and elsewhere it is Org's RET. `<CR>` does that too with
  `return_uses_meta_return`.
- **`<Leader><CR>`** and its neighbours put the new heading, item or table
  row below the cursor's line, wherever on it the cursor is: never above
  it, and nothing is split. Typing goes on in insert mode.
- **`K`** in the agenda shows the line's entry in a float at the cursor:
  its headline and its text, each drawer folded to its first line
  (`:PROPERTIES:...`), highlighted as Org files are, with room around
  it (50 columns at least). A second `K` goes
  into the float, `q` there closes it, and moving the cursor closes it
  too.
- **`cid`** leaves the cursor where it was in the timestamp it changes,
  and a date asked for in the calendar goes back to the window it was
  asked from.
- **Questions** are asked in the editor's own way:
  - a date (`<Leader>ois`, `J` in the agenda) opens Emacs's calendar at
    the bottom, with the keys under *In the date prompt*; `i` there asks
    for the date as text with `vim.ui.input`, read as Org reads it
    (`fri 10:00`, `+2d`, `2/15`); `t` there picks a time: a `Time:` line
    shows it, `h` and `l` choose the hour or the minutes, `k` and `j` move
    it up or down (the minutes by `org_time_stamp_rounding_minutes`), and `t`
    again goes back to the days, keeping the time;
  - one key (a TODO keyword with a key, as `TODO(t)`, a capture template,
    or a view in the agenda menu) is read as soon as it is typed, with the
    choices listed in a window at the bottom; `<Esc>` gives up;
  - a choice from a list (refile targets) goes through `vim.ui.select`,
    so a picker plugin that replaces it is used;
  - text (a capture prompt) goes through `vim.ui.input`;
  - a state note (a keyword with `@`, as `WAIT(w@)`) is written in a note
    buffer that opens like a capture buffer, as Emacs's `*Org Note*`:
    `<C-c>` stores it, `<Leader>ok` changes the state without it.
- **Messages**: what Emacs says when a command is done (`Priority of
  current item set to B`, `Scheduled to <2026-05-22 Fri>`, `Refile to
  "Projects" in file …: done`) goes through `vim.notify`, and so do
  commands' errors (`No timestamp at the cursor`).
- **Capture buffers** open in insert mode, where the template's `%?` was.

## Changing them

Keys are set with `mappings`, in `setup()` or in `vim.g.org_lsp` before
the plugin loads. It has one table per scope: `global`, `org`, `agenda`,
`calendar` and `capture`. In a scope, a command (the second column above,
or one of the plugin's own names) maps to:

- a key, which takes the place of the command's default keys;
- a list of keys, to have more than one;
- `false`, to leave the command without a key.

Commands that are not listed keep their default keys.

```lua
require('org-lsp').setup({
  mappings = {
    -- Org's own key for the agenda, and a key for the TODO list.
    global = { ['org.agenda.dispatch'] = '<Leader>a', ['org.agenda.todos'] = '<Leader>oT' },
    org = {
      ['org.todo'] = { 'cit', '<Leader>ot' },   -- two keys
      ['org.todo.next'] = '<C-n>',              -- a command without a default key
      ['org.todo.previous'] = false,            -- no key
      ['org.shift.right'] = '<S-Right>',        -- Org's S-<right>: date or TODO state
      ['org.shift.left'] = '<S-Left>',
      global_cycle = '<Leader><Tab>',           -- the plugin's own command
    },
    agenda = { ['org.agenda.goto.date'] = 'gd', quit = { 'q', '<Esc>' } },
  },
})
```

To set no keys in one scope, give `false` for the scope; the plugin's own
keys in it go too. To set no keys at all, give `false` for `mappings`:

```lua
vim.g.org_lsp = { mappings = { org = false } }   -- nothing in Org files
vim.g.org_lsp = { mappings = false }             -- nothing anywhere
```

Calling `setup()` again changes the keys of buffers opened after it, and
the global ones at once.

### Your own keys

Any command can be run from a key of your own, with
`require('org-lsp').run`, which tells the server where the cursor is:

```lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = 'org',
  callback = function(args)
    vim.keymap.set('n', '<M-Right>', function()
      require('org-lsp').run('org.shift.right', { count = vim.v.count })
    end, { buffer = args.buf, desc = 'Org: date later or next TODO state' })
  end,
})
```

Keys set this way are not repeated by `.`; keys set through `mappings`
are. `run` also takes the plugin's own names (`cycle`, `global_cycle`,
`quit`, `hover`).

### From the command line

`:Org` runs a command by its name without the `org.` prefix, with
completion: `:Org todo.next`, `:Org agenda.todos`. Words separated by
spaces are joined with dots, so `:Org agenda todos` works as well, and a
group on its own runs its dispatcher: `:Org agenda`.

The editing commands are also code actions, so `vim.lsp.buf.code_action()`
(`gra`) lists those that apply at the cursor.
