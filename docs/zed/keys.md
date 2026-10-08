# Keys in Zed

Zed cannot run a language server's command from a key, so in Org files
the commands are in the **code actions** (**editor: toggle code actions**,
`ctrl-.` / `cmd-.`), which list what applies where the cursor is:

- In an Org file: change the TODO state (the next one, the previous one,
  or one picked from a list), the priority, the timestamp under the
  cursor; toggle a checkbox; a new heading, item or table row (as M-RET,
  M-S-RET, C-RET and C-S-RET make them), the cursor then in it; clock in
  and out; refile; archive; open the agenda (this week, the TODO list, or
  the agenda menu); capture.
- In a capture buffer: file the entry, give up, or refile it.

Bind `editor::ToggleCodeActions` to a key of your own if the menu is too
far away.

What a command asks for is asked with buttons: a TODO state, a priority,
a refile target, a capture template, yes or no. Zed has no way to ask for
typed text, so the commands that want a date (schedule, deadline) or text
(a template's `%^{…}` prompts, a tags search) say so and do nothing; set
those dates in the file itself, where completion helps.

The settings are in [configuration.md](configuration.md).

## The agenda's keys

The agenda is a read-only file the server writes (`Agenda.org-agenda`, in
a directory of its own under the system's temporary one), opened as Org;
the server writes it again when it changes. Its commands have keys
instead of code actions: the same keys as in every editor, Emacs's agenda
keys, with `J` for a date and the views under `v` (`v d`, `v w`), so that
Vim mode keeps its motions.

An extension cannot bind keys, and a key of Zed's cannot run a language
server's command, but it can run a task. Two files of the extension set
that up, once:

1. [tasks.json](../../editors/zed/tasks.json): a task for each agenda
   command (*Org agenda: …*). Paste its entries into your tasks (**zed:
   open tasks**, `~/.config/zed/tasks.json`).
2. [keymap.json](../../editors/zed/keymap.json): binds the agenda's keys
   to those tasks. Paste its block into your keymap (**zed: open
   keymap**).

Then in the agenda:

<!-- The table below is written by scripts/generate-docs.mjs from
     editors/zed/keymap.json and tasks.json, which editors/zed/generate.mjs
     writes from the server's manifest. Edit the server's table in
     server/org-lsp/src/commands.rs, not the table. -->

<!-- BEGIN generated: keys -->

| Keys | Command or Zed action | What it does |
| --- | --- | --- |
| `q` | `pane::CloseActiveItem` | Close the agenda |
| `enter` | `editor::GoToDefinition` | Go to the entry of the line |
| `tab` | `editor::GoToDefinitionSplit` | Show the entry of the line in a split |
| `f` | `org.agenda.later` | Agenda: go forward in time |
| `b` | `org.agenda.earlier` | Agenda: go backward in time |
| `.` | `org.agenda.today` | Agenda: go to today |
| `[` | `org.agenda.query.add` | Agenda: search for another word |
| `]` | `org.agenda.query.subtract` | Agenda: search without a word |
| `{` | `org.agenda.query.add.regexp` | Agenda: search for another regexp |
| `}` | `org.agenda.query.subtract.regexp` | Agenda: search without a regexp |
| `shift-j` | `org.agenda.goto.date` | Agenda: go to a date |
| `v d` | `org.agenda.day` | Agenda: day view |
| `v w` | `org.agenda.week` | Agenda: week view |
| `v t` | `org.agenda.fortnight` | Agenda: fortnight view |
| `v m` | `org.agenda.month` | Agenda: month view |
| `v y` | `org.agenda.year` | Agenda: year view |
| `r` | `org.agenda.redo` | Agenda: rebuild |
| `s`, `ctrl-x ctrl-s` | `org.agenda.save` | Save all Org files |
| `v l` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `shift-k` | `org.agenda.habits` | Agenda: show or hide habits |
| `ctrl-x ctrl-w` | `org.agenda.write` | Agenda: write it to a file |
| `ctrl-c ctrl-x ctrl-c` | `org.agenda.columns` | Agenda: column view |
| `shift-r`, `v shift-r` | `org.agenda.clockreport` | Agenda: the clock report |
| `shift-e`, `v shift-e` | `org.agenda.entry.text` | Agenda: entry text mode, some text of each entry under its line |
| `shift-f` | `org.agenda.follow` | Agenda: follow mode, the cursor's entry in another window |
| `m` | `org.agenda.bulk.mark` | Agenda: mark the entry for a bulk action |
| `u` | `org.agenda.bulk.unmark` | Agenda: unmark the entry |
| `shift-u` | `org.agenda.bulk.unmark.all` | Agenda: unmark all entries |
| `alt-m` | `org.agenda.bulk.toggle` | Agenda: toggle the mark of the entry |
| `*` | `org.agenda.bulk.mark.all` | Agenda: mark all entries |
| `alt-*` | `org.agenda.bulk.toggle.all` | Agenda: toggle the marks of all entries |
| `%` | `org.agenda.bulk.mark.regexp` | Agenda: mark the entries matching a regexp |
| `shift-b` | `org.agenda.bulk.action` | Agenda: an action on the marked entries |
| `/` | `org.agenda.filter` | Agenda: filter by tags, categories, effort and regexps |
| `\` | `org.agenda.filter.tag` | Agenda: filter by tag |
| `<` | `org.agenda.filter.category` | Agenda: filter by the line's category |
| `=` | `org.agenda.filter.regexp` | Agenda: filter by a regexp |
| `_` | `org.agenda.filter.effort` | Agenda: filter by effort |
| `^` | `org.agenda.filter.top.headline` | Agenda: filter by the line's top headline |
| `\|` | `org.agenda.filter.remove` | Agenda: remove all filters |
| `v a` | `org.agenda.archives` | Agenda: include archived trees |
| `v shift-a` | `org.agenda.archives.files` | Agenda: include archived trees and archive files |
| `shift-i` | `org.clock.in` | Clock in |
| `shift-o` | `org.clock.out` | Clock out |
| `shift-x` | `org.clock.cancel` | Cancel the clock |
| `t` | `org.todo` | Change the TODO state |
| `+`, `shift-up` | `org.priority.up` | Raise the priority |
| `-`, `shift-down` | `org.priority.down` | Lower the priority |
| `,` | `org.priority` | Set the priority |
| `ctrl-c ctrl-x p` | `org.property.set` | Set a property of the entry |
| `e` | `org.effort.set` | Set the effort of the entry |
| `ctrl-c ctrl-w` | `org.refile` | Refile the entry |
| `z` | `org.add.note` | Add a note to the entry |
| `ctrl-c ctrl-o` | `org.open` | Open the link or thing at the cursor |
| `ctrl-c l` | `org.link.store` | Store a link to the place at the cursor |
| `$`, `ctrl-c ctrl-x ctrl-s`, `ctrl-c $` | `org.archive.subtree` | Archive the subtree |
| `ctrl-c ctrl-x shift-a` | `org.archive.sibling` | Archive the subtree under its Archive sibling |
| `ctrl-c ctrl-x a` | `org.archive.tag` | Toggle the ARCHIVE tag |
| `ctrl-c ctrl-x ctrl-a` | `org.archive.default` | Archive with the default command |
| `a` | `org.archive.confirm` | Archive with the default command, once confirmed |
| `shift-right` | `org.date.later` | Move the date one day later |
| `shift-left` | `org.date.earlier` | Move the date one day earlier |
| `ctrl-c ctrl-s` | `org.schedule` | Schedule the entry |
| `ctrl-c ctrl-d` | `org.deadline` | Set the deadline of the entry |

<!-- END generated -->

A task's terminal stays out of sight unless the command fails.

The tasks are yours rather than the extension's because Zed offers an
extension's tasks only in files of the project's folders, and the agenda
is a file of the server's own, elsewhere: a key bound to one would open
the task picker instead. Each task runs
`~/.local/state/org-lsp/agenda-command`, a script the server writes,
which runs the command in the agenda where the cursor is: the server
follows the cursor through the document highlights Zed asks for when it
moves (after `lsp_highlight_debounce`, 75 milliseconds). With agendas
open in several windows, the keys act on the one the cursor was last in.
On Windows, where the script is `agenda-command.cmd` under your user
folder and the shell is PowerShell, make each task's command
`& "$HOME/.local/state/org-lsp/agenda-command.cmd" org.agenda.later`
(with its own command at the end).
