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
| `<Leader>oxo` | `org.clock.out` | Clock out |
| `<Leader>oxq` | `org.clock.cancel` | Cancel the clock |
| `cit` | `org.todo` | Change the TODO state |
| `ciT` | `org.todo.previous` | Cycle TODO state backward |
| `ciR` | `org.priority.up` | Raise the priority |
| `cir` | `org.priority.down` | Lower the priority |
| `<Leader>o,` | `org.priority` | Set the priority |
| `<Leader>or` | `org.refile` | Refile the entry |
| `<C-a>`¹ | `org.timestamp.up` | Increase the part of the timestamp at the cursor |
| `<C-x>`¹ | `org.timestamp.down` | Decrease the part of the timestamp at the cursor |
| `<S-Up>`¹ | `org.date.later` | Move the date one day later |
| `<S-Down>`¹ | `org.date.earlier` | Move the date one day earlier |
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
| `J` | `org.agenda.goto.date` | Agenda: go to a date |
| `vd` | `org.agenda.day` | Agenda: day view |
| `vw` | `org.agenda.week` | Agenda: week view |
| `vt` | `org.agenda.fortnight` | Agenda: fortnight view |
| `vm` | `org.agenda.month` | Agenda: month view |
| `vy` | `org.agenda.year` | Agenda: year view |
| `r` | `org.agenda.redo` | Agenda: rebuild |
| `<CR>`, `<Tab>` | `org.agenda.goto` | Agenda: go to the entry |
| `vl` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `R` | `org.agenda.clockreport` | Agenda: the clock report |
| `I` | `org.clock.in` | Clock in |
| `O` | `org.clock.out` | Clock out |
| `X` | `org.clock.cancel` | Cancel the clock |
| `t` | `org.todo` | Change the TODO state |
| `+` | `org.priority.up` | Raise the priority |
| `-` | `org.priority.down` | Lower the priority |
| `<Leader>o,` | `org.priority` | Set the priority |
| `<Leader>or` | `org.refile` | Refile the entry |
| `<S-Right>` | `org.date.later` | Move the date one day later |
| `<S-Left>` | `org.date.earlier` | Move the date one day earlier |
| `<Leader>ois` | `org.schedule` | Schedule the entry |
| `<Leader>oid` | `org.deadline` | Set the deadline of the entry |

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
| `org.agenda.save` | Save all Org files |
| `org.todo.next` | Cycle TODO state forward |
| `org.shift.up` | Increase the timestamp part or raise the priority |
| `org.shift.down` | Decrease the timestamp part or lower the priority |
| `org.shift.left` | Move the date a day earlier or cycle the TODO state backward |
| `org.shift.right` | Move the date a day later or cycle the TODO state forward |

¹ Where the command does not apply, the key does what it does in any
buffer: `<C-a>` off a timestamp still increments the number under the
cursor.

<!-- END generated -->

The plugin adds three keys of its own, which only an editor can carry out:

| Keys | Name in `mappings` | Scope | What it does |
| --- | --- | --- | --- |
| `<Tab>` | `cycle` | `org` | Cycle the visibility of the subtree at the cursor |
| `<S-Tab>` | `global_cycle` | `org` | Cycle the visibility of the whole file |
| `q` | `quit` | `agenda` | Close the agenda |

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
- **Questions** are asked in the editor's own way:
  - a date (`<Leader>ois`, `J` in the agenda) opens Emacs's calendar at
    the bottom, with the keys under *In the date prompt*; `i` there asks
    for the date as text with `vim.ui.input`, read as Org reads it
    (`fri 10:00`, `+2d`, `2/15`); `t` there picks a time: a `Time:` line
    shows it, `h` and `l` choose the hour or the minutes, `k` and `j` move
    it up or down (the minutes by `timestampRoundingMinutes`), and `t`
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
`quit`).

### From the command line

`:Org` runs a command by its name without the `org.` prefix, with
completion: `:Org todo.next`, `:Org agenda.todos`. Words separated by
spaces are joined with dots, so `:Org agenda todos` works as well, and a
group on its own runs its dispatcher: `:Org agenda`.

The editing commands are also code actions, so `vim.lsp.buf.code_action()`
(`gra`) lists those that apply at the cursor.
