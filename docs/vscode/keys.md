# Keys in VS Code

The setting `org-lsp.keys` picks which keys the extension binds:

- **`emacs`** (the default): Emacs's keys, in Org files (`Ctrl+C Ctrl+T`,
  `Tab`, `Shift+arrows`, …), in capture buffers, and Org's single letters
  in the agenda and the date prompt, which are read-only. In Org files
  `Ctrl+C` then waits for a second key instead of copying; `Ctrl+Insert`
  and the context menu still copy.
- **`vim`**: the Neovim plugin's keys, for VSCodeVim. In Org files, VS Code
  binds only the keys that take nothing from Vim (`Tab`, `Shift+Tab`,
  `Ctrl+Space`, `Ctrl+A`, `Ctrl+X`, `Shift+Up`/`Down`), in Normal mode;
  the keys with the leader go in VSCodeVim's own settings, below. Keys
  such as `cit` would take `c` from Vim, so they are not bound.
- **`custom`**: no keys at all. Bind the commands yourself, as shown under
  *Changing them* below.

`Tab` and `Shift+Tab` cycle the visibility of a subtree and of the whole
file, as Org's `TAB` and `S-TAB` do: folded, its children, all of it;
the top level, every headline, everything. Off a headline, block or
drawer, `Tab` is VS Code's own. The commands are also in the Command
Palette under **Org**, and the ones that change the text at the cursor
under **Quick Fix…** (`Ctrl+.`).

The bindings are generated from the server's own description of its
commands (`npm run generate` in `editors/vscode`). **Preferences: Open
Keyboard Shortcuts** lists them; search for `org.`.

The other options are in [configuration.md](configuration.md).

## The keys

<!-- The tables below are written by scripts/generate-docs.mjs from the
     server's manifest and editors/vscode/package.json. Edit the server's
     table in server/org-lsp/src/commands.rs, not the tables. -->

<!-- BEGIN generated: keys -->

### Emacs (`"org-lsp.keys": "emacs"`, the default)

#### In Org files

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C Ctrl+X Ctrl+I` | `org.clock.in` | Clock in |
| `Ctrl+C Ctrl+X Ctrl+X` | `org.clock.in.last` | Clock in the last clocked task |
| `Ctrl+C Ctrl+X Ctrl+O` | `org.clock.out` | Clock out |
| `Ctrl+C Ctrl+X Ctrl+Q` | `org.clock.cancel` | Cancel the clock |
| `Ctrl+C Ctrl+X Ctrl+J` | `org.clock.goto` | Go to the clocked entry |
| `Ctrl+C Ctrl+X Ctrl+D` | `org.clock.display` | Show the time clocked under each headline |
| `Ctrl+C Ctrl+X Ctrl+R` | `org.clock.report` | Insert or update a clock table |
| `Ctrl+C Ctrl+X Ctrl+U` | `org.dblock.update` | Update the dynamic block |
| `Ctrl+C Ctrl+X x` | `org.dblock.insert` | Insert a dynamic block |
| `Ctrl+C Ctrl+A` | `org.attach` | Attachments of the entry |
| `Ctrl+C Ctrl+T` | `org.todo` | Change the TODO state |
| `Ctrl+C ,` | `org.priority` | Set the priority |
| `Ctrl+C Ctrl+X p` | `org.property.set` | Set a property of the entry |
| `Ctrl+C Ctrl+X e` | `org.effort.set` | Set the effort of the entry |
| `Ctrl+C Ctrl+X Shift+E` | `org.effort.increase` | Set the effort to the next allowed value |
| `Ctrl+C Ctrl+X o` | `org.ordered.toggle` | Toggle whether subtasks must be done in order (ORDERED) |
| `Ctrl+C Ctrl+X Shift+2` | `org.cite.insert` | Insert a citation, or edit the one at the cursor |
| `Ctrl+C Ctrl+Q` | `org.tags.set` | Set the tags of the headline |
| `Ctrl+C Ctrl+C` | `org.checkbox.toggle` | Toggle the checkbox |
| `Enter`¹ | `org.return` | New line, as Org's RET makes it |
| `Alt+Enter` | `org.meta.return` | New heading, item or table row |
| `Alt+Shift+Enter` | `org.insert.todo.heading` | New TODO heading, or item with a checkbox |
| `Ctrl+Enter` | `org.insert.heading.after` | New heading after the subtree |
| `Ctrl+Shift+Enter` | `org.insert.todo.heading.after` | New TODO heading after the subtree |
| `Alt+Left`¹ | `org.meta.left` | Promote the heading; outdent the item; move the table column left |
| `Alt+Right`¹ | `org.meta.right` | Demote the heading; indent the item, the drawer or the block; move the table column right |
| `Alt+Shift+Left`¹ | `org.shiftmeta.left` | Promote the subtree; outdent the item with its children; delete the table column |
| `Alt+Shift+Right`¹ | `org.shiftmeta.right` | Demote the subtree; indent the item with its children; insert a table column |
| `Alt+Up`¹ | `org.meta.up` | Move the subtree, the item or the table row up |
| `Alt+Down`¹ | `org.meta.down` | Move the subtree, the item or the table row down |
| `Alt+Shift+Up`¹ | `org.shiftmeta.up` | Delete the table row |
| `Alt+Shift+Down`¹ | `org.shiftmeta.down` | Insert a table row above |
| `Ctrl+C -` | `org.table.hline` | Insert a rule below the table row |
| `Ctrl+C Enter` | `org.table.hline.move` | Insert a rule below the table row and go below it |
| `Ctrl+C Shift+6` | `org.sort` | Sort the entries, the items or the table rows |
| `Ctrl+C /` | `org.sparse.tree` | Show a sparse tree |
| `Ctrl+C Ctrl+X Ctrl+W` | `org.cut.subtree` | Cut the subtree |
| `Ctrl+C Ctrl+X Alt+W` | `org.copy.subtree` | Copy the subtree |
| `Ctrl+C Ctrl+X Ctrl+Y` | `org.paste.subtree` | Paste the subtree |
| `Ctrl+C Ctrl+W` | `org.refile` | Refile the entry |
| `Ctrl+C Alt+W` | `org.refile.copy` | Copy the entry to a refile target |
| `Ctrl+C Ctrl+Z` | `org.add.note` | Add a note to the entry |
| `Ctrl+C Ctrl+V t`, `Ctrl+C Ctrl+V Ctrl+T` | `org.babel.tangle` | Tangle the file's code blocks |
| `Ctrl+C Ctrl+V e`, `Ctrl+C Ctrl+V Ctrl+E` | `org.babel.execute` | Evaluate the code block or call at the cursor through Emacs |
| `Ctrl+C Ctrl+V b`, `Ctrl+C Ctrl+V Ctrl+B` | `org.babel.execute.buffer` | Evaluate the file's code blocks through Emacs |
| `Ctrl+C Ctrl+V s`, `Ctrl+C Ctrl+V Ctrl+S` | `org.babel.execute.subtree` | Evaluate the subtree's code blocks through Emacs |
| `Ctrl+C Ctrl+V k` | `org.babel.remove_result` | Remove the results of the code block at the cursor |
| `Ctrl+C Ctrl+N` | `org.heading.next` | Move to the next heading |
| `Ctrl+C Ctrl+P` | `org.heading.previous` | Move to the previous heading |
| `Ctrl+C Ctrl+F` | `org.heading.forward` | Move to the next heading of the same level |
| `Ctrl+C Ctrl+B` | `org.heading.backward` | Move to the previous heading of the same level |
| `Ctrl+C Ctrl+U` | `org.heading.up` | Move to the parent heading |
| `Ctrl+C Ctrl+X Ctrl+N` | `org.link.next` | Move to the next link |
| `Ctrl+C Ctrl+X Ctrl+P` | `org.link.previous` | Move to the previous link |
| `Ctrl+C Ctrl+J` | `org.goto` | Go to a heading of the file |
| `Ctrl+C Ctrl+X f` | `org.footnote.action` | Make a footnote, or go between a footnote and its definition |
| `Ctrl+C ;` | `org.toggle.comment` | Toggle the COMMENT keyword of the heading |
| `Ctrl+C Ctrl+X Ctrl+F` | `org.emphasize` | Insert emphasis markers |
| `Ctrl+C Ctrl+,` | `org.structure.template` | Insert a block (structure template) |
| `Ctrl+C Ctrl+X d` | `org.drawer.insert` | Insert a drawer |
| `Ctrl+C Ctrl+X c` | `org.subtree.clone` | Clone the subtree, shifting its dates |
| `Shift+Enter`¹ | `org.table.copy.down` | Copy the field down to the next row |
| `Ctrl+C +` | `org.table.sum` | Sum the numbers of the table column |
| `Ctrl+C ?` | `org.table.field.info` | Show the references of the table field |
| `Ctrl+C Shift+\` | `org.table.create` | Insert an empty table |
| `Ctrl+C Ctrl+V n`, `Ctrl+C Ctrl+V Ctrl+N` | `org.babel.next` | Move to the next code block |
| `Ctrl+C Ctrl+V p`, `Ctrl+C Ctrl+V Ctrl+P` | `org.babel.previous` | Move to the previous code block |
| `Ctrl+C Ctrl+V g` | `org.babel.goto` | Go to a named code block |
| `Ctrl+C Ctrl+V i` | `org.babel.lob.ingest` | Add the file's named code blocks to the Library of Babel |
| `Ctrl+C Alt+F` | `org.block.next` | Move to the next block |
| `Ctrl+C Alt+B` | `org.block.previous` | Move to the previous block |
| `Ctrl+C Ctrl+X 0` | `org.timer.start` | Start the relative timer |
| `Ctrl+C Ctrl+X .` | `org.timer.insert` | Insert the timer's value |
| `Ctrl+C Ctrl+X -` | `org.timer.item` | Insert a list item with the timer's value |
| `Ctrl+C Ctrl+X ,` | `org.timer.pause` | Pause or continue the timer |
| `Ctrl+C Ctrl+X _` | `org.timer.stop` | Stop the timer |
| `Ctrl+C Ctrl+X ;` | `org.timer.set` | Set a countdown timer |
| `Ctrl+C Ctrl+E` | `org.export.dispatch` | Export the file through Emacs |
| `Ctrl+C Shift+8` | `org.table.recalculate.line` | Recalculate the table's current line |
| `Ctrl+C Ctrl+O` | `org.open` | Open the link or thing at the cursor |
| `Ctrl+C l` | `org.link.store` | Store a link to the place at the cursor |
| `Ctrl+C Ctrl+L` | `org.link.insert` | Insert a link, or edit the one at the cursor |
| `Ctrl+C Alt+L` | `org.link.insert.last` | Insert the last stored link |
| `Ctrl+C Ctrl+Alt+L` | `org.link.insert.all` | Insert all stored links |
| `Ctrl+C Ctrl+X Ctrl+S`, `Ctrl+C Shift+4` | `org.archive.subtree` | Archive the subtree |
| `Ctrl+C Ctrl+X Shift+A` | `org.archive.sibling` | Archive the subtree under its Archive sibling |
| `Ctrl+C Ctrl+X a` | `org.archive.tag` | Toggle the ARCHIVE tag |
| `Ctrl+C Ctrl+X Ctrl+A` | `org.archive.default` | Archive with the default command |
| `Shift+Up`¹ | `org.shift.up` | Increase the timestamp part, raise the priority, or go to the previous item |
| `Shift+Down`¹ | `org.shift.down` | Decrease the timestamp part, lower the priority, or go to the next item |
| `Shift+Left`¹ | `org.shift.left` | Move the date a day earlier, or cycle the TODO state or the list's bullets backward |
| `Shift+Right`¹ | `org.shift.right` | Move the date a day later, or cycle the TODO state or the list's bullets forward |
| `Ctrl+C Ctrl+S` | `org.schedule` | Schedule the entry |
| `Ctrl+C Ctrl+D` | `org.deadline` | Set the deadline of the entry |
| `Ctrl+C .` | `org.timestamp.insert` | Insert or change a timestamp |
| `Ctrl+C !` | `org.timestamp.insert.inactive` | Insert or change an inactive timestamp |
| `Tab`¹ | `org-lsp.cycle` | Cycle the visibility of the subtree, block or drawer at the cursor |
| `Shift+Tab` | `org-lsp.cycleGlobal` | Cycle the visibility of the whole file |
| `Ctrl+C a` | `org.agenda.dispatch` | Agenda commands |
| `Ctrl+C c` | `org.capture` | Capture |

#### In a capture buffer

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C Ctrl+C` | `org.capture.finish` | Capture: finish |
| `Ctrl+C Ctrl+K` | `org.capture.abort` | Capture: abort |
| `Ctrl+C Ctrl+W` | `org.capture.refile` | Capture: refile the entry |

#### In the agenda

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C a` | `org.agenda.dispatch` | Agenda commands |
| `Ctrl+C c` | `org.capture` | Capture |
| `f` | `org.agenda.later` | Agenda: go forward in time |
| `b` | `org.agenda.earlier` | Agenda: go backward in time |
| `.` | `org.agenda.today` | Agenda: go to today |
| `[` | `org.agenda.query.add` | Agenda: search for another word |
| `]` | `org.agenda.query.subtract` | Agenda: search without a word |
| `Shift+[` | `org.agenda.query.add.regexp` | Agenda: search for another regexp |
| `Shift+]` | `org.agenda.query.subtract.regexp` | Agenda: search without a regexp |
| `Shift+J` | `org.agenda.goto.date` | Agenda: go to a date |
| `v d` | `org.agenda.day` | Agenda: day view |
| `v w` | `org.agenda.week` | Agenda: week view |
| `v t` | `org.agenda.fortnight` | Agenda: fortnight view |
| `v m` | `org.agenda.month` | Agenda: month view |
| `v y` | `org.agenda.year` | Agenda: year view |
| `r` | `org.agenda.redo` | Agenda: rebuild |
| `Enter` | `org.agenda.goto` | Agenda: go to the entry |
| `Tab` | `org.agenda.goto.other` | Agenda: go to the entry in another window |
| `s`, `Ctrl+X Ctrl+S` | `org.agenda.save` | Save all Org files |
| `v l` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `Shift+K` | `org.agenda.habits` | Agenda: show or hide habits |
| `Ctrl+X Ctrl+W` | `org.agenda.write` | Agenda: write it to a file |
| `Shift+R`, `v Shift+R` | `org.agenda.clockreport` | Agenda: the clock report |
| `Shift+E`, `v Shift+E` | `org.agenda.entry.text` | Agenda: entry text mode, some text of each entry under its line |
| `Shift+F` | `org.agenda.follow` | Agenda: follow mode, the cursor's entry in another window |
| `m` | `org.agenda.bulk.mark` | Agenda: mark the entry for a bulk action |
| `u` | `org.agenda.bulk.unmark` | Agenda: unmark the entry |
| `Shift+U` | `org.agenda.bulk.unmark.all` | Agenda: unmark all entries |
| `Alt+M` | `org.agenda.bulk.toggle` | Agenda: toggle the mark of the entry |
| `Shift+8` | `org.agenda.bulk.mark.all` | Agenda: mark all entries |
| `Alt+Shift+8` | `org.agenda.bulk.toggle.all` | Agenda: toggle the marks of all entries |
| `Shift+5` | `org.agenda.bulk.mark.regexp` | Agenda: mark the entries matching a regexp |
| `Shift+B` | `org.agenda.bulk.action` | Agenda: an action on the marked entries |
| `/` | `org.agenda.filter` | Agenda: filter by tags, categories, effort and regexps |
| `\` | `org.agenda.filter.tag` | Agenda: filter by tag |
| `<` | `org.agenda.filter.category` | Agenda: filter by the line's category |
| `=` | `org.agenda.filter.regexp` | Agenda: filter by a regexp |
| `_` | `org.agenda.filter.effort` | Agenda: filter by effort |
| `Shift+6` | `org.agenda.filter.top.headline` | Agenda: filter by the line's top headline |
| `Shift+\` | `org.agenda.filter.remove` | Agenda: remove all filters |
| `v a` | `org.agenda.archives` | Agenda: include archived trees |
| `v Shift+A` | `org.agenda.archives.files` | Agenda: include archived trees and archive files |
| `Shift+I` | `org.clock.in` | Clock in |
| `Shift+O` | `org.clock.out` | Clock out |
| `Shift+X` | `org.clock.cancel` | Cancel the clock |
| `t` | `org.todo` | Change the TODO state |
| `+`, `Shift+Up` | `org.priority.up` | Raise the priority |
| `-`, `Shift+Down` | `org.priority.down` | Lower the priority |
| `,` | `org.priority` | Set the priority |
| `Ctrl+C Ctrl+X p` | `org.property.set` | Set a property of the entry |
| `e` | `org.effort.set` | Set the effort of the entry |
| `Ctrl+C Ctrl+W` | `org.refile` | Refile the entry |
| `z` | `org.add.note` | Add a note to the entry |
| `Ctrl+C Ctrl+O` | `org.open` | Open the link or thing at the cursor |
| `Ctrl+C l` | `org.link.store` | Store a link to the place at the cursor |
| `Shift+4`, `Ctrl+C Ctrl+X Ctrl+S`, `Ctrl+C Shift+4` | `org.archive.subtree` | Archive the subtree |
| `Ctrl+C Ctrl+X Shift+A` | `org.archive.sibling` | Archive the subtree under its Archive sibling |
| `Ctrl+C Ctrl+X a` | `org.archive.tag` | Toggle the ARCHIVE tag |
| `Ctrl+C Ctrl+X Ctrl+A` | `org.archive.default` | Archive with the default command |
| `a` | `org.archive.confirm` | Archive with the default command, once confirmed |
| `Shift+Right` | `org.date.later` | Move the date one day later |
| `Shift+Left` | `org.date.earlier` | Move the date one day earlier |
| `Ctrl+C Ctrl+S` | `org.schedule` | Schedule the entry |
| `Ctrl+C Ctrl+D` | `org.deadline` | Set the deadline of the entry |
| `g ?` | `org-lsp.help` | Show the keys |
| `q` | `org-lsp.agenda.close` | Agenda: close |

#### In the date prompt

| Keys | Command | What it does |
| --- | --- | --- |
| `Shift+Right` | `org.calendar.day.later` | Date prompt: one day later |
| `Shift+Left` | `org.calendar.day.earlier` | Date prompt: one day earlier |
| `Shift+Down` | `org.calendar.week.later` | Date prompt: one week later |
| `Shift+Up` | `org.calendar.week.earlier` | Date prompt: one week earlier |
| `Alt+Shift+Right` | `org.calendar.month.later` | Date prompt: one month later |
| `Alt+Shift+Left` | `org.calendar.month.earlier` | Date prompt: one month earlier |
| `Alt+Shift+Down` | `org.calendar.year.later` | Date prompt: one year later |
| `Alt+Shift+Up` | `org.calendar.year.earlier` | Date prompt: one year earlier |
| `>` | `org.calendar.scroll.later` | Date prompt: show the next months |
| `<` | `org.calendar.scroll.earlier` | Date prompt: show the previous months |
| `.` | `org.calendar.today` | Date prompt: go to today |
| `Enter` | `org.calendar.select` | Date prompt: take the day at the cursor |
| `i` | `org.calendar.type` | Date prompt: type the date |
| `t` | `org.calendar.time` | Date prompt: pick the time, or go back to the day |
| `q`, `Ctrl+G` | `org.calendar.cancel` | Date prompt: cancel |

### Vim (`"org-lsp.keys": "vim"`)

#### In Org files

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+Space` | `org.checkbox.toggle` | Toggle the checkbox |
| `Enter`¹ | `org.return` | New line, as Org's RET makes it |
| `Shift+Enter`¹ | `org.return.item` | New line; on a list item, the next item |
| `Ctrl+A`¹ | `org.timestamp.up` | Increase the part of the timestamp at the cursor |
| `Ctrl+X`¹ | `org.timestamp.down` | Decrease the part of the timestamp at the cursor |
| `Shift+Up`¹ | `org.date.later` | Move the date one day later |
| `Shift+Down`¹ | `org.date.earlier` | Move the date one day earlier |
| `Shift+Left`¹ | `org.shift.left` | Move the date a day earlier, or cycle the TODO state or the list's bullets backward |
| `Shift+Right`¹ | `org.shift.right` | Move the date a day later, or cycle the TODO state or the list's bullets forward |
| `Tab` | `org-lsp.cycle` | Cycle the visibility of the subtree, block or drawer at the cursor |
| `Shift+Tab` | `org-lsp.cycleGlobal` | Cycle the visibility of the whole file |

#### In a capture buffer

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C` | `org.capture.finish` | Capture: finish |

#### In the agenda

| Keys | Command | What it does |
| --- | --- | --- |
| `f` | `org.agenda.later` | Agenda: go forward in time |
| `b` | `org.agenda.earlier` | Agenda: go backward in time |
| `.` | `org.agenda.today` | Agenda: go to today |
| `[` | `org.agenda.query.add` | Agenda: search for another word |
| `]` | `org.agenda.query.subtract` | Agenda: search without a word |
| `Shift+[` | `org.agenda.query.add.regexp` | Agenda: search for another regexp |
| `Shift+]` | `org.agenda.query.subtract.regexp` | Agenda: search without a regexp |
| `Shift+J` | `org.agenda.goto.date` | Agenda: go to a date |
| `v d` | `org.agenda.day` | Agenda: day view |
| `v w` | `org.agenda.week` | Agenda: week view |
| `v t` | `org.agenda.fortnight` | Agenda: fortnight view |
| `v m` | `org.agenda.month` | Agenda: month view |
| `v y` | `org.agenda.year` | Agenda: year view |
| `r` | `org.agenda.redo` | Agenda: rebuild |
| `Enter` | `org.agenda.goto` | Agenda: go to the entry |
| `Tab` | `org.agenda.goto.other` | Agenda: go to the entry in another window |
| `s`, `Ctrl+X Ctrl+S` | `org.agenda.save` | Save all Org files |
| `v l` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `Shift+K` | `org.agenda.habits` | Agenda: show or hide habits |
| `Ctrl+X Ctrl+W` | `org.agenda.write` | Agenda: write it to a file |
| `Shift+R`, `v Shift+R` | `org.agenda.clockreport` | Agenda: the clock report |
| `Shift+E`, `v Shift+E` | `org.agenda.entry.text` | Agenda: entry text mode, some text of each entry under its line |
| `Shift+F` | `org.agenda.follow` | Agenda: follow mode, the cursor's entry in another window |
| `m` | `org.agenda.bulk.mark` | Agenda: mark the entry for a bulk action |
| `u` | `org.agenda.bulk.unmark` | Agenda: unmark the entry |
| `Shift+U` | `org.agenda.bulk.unmark.all` | Agenda: unmark all entries |
| `Alt+M` | `org.agenda.bulk.toggle` | Agenda: toggle the mark of the entry |
| `Shift+8` | `org.agenda.bulk.mark.all` | Agenda: mark all entries |
| `Alt+Shift+8` | `org.agenda.bulk.toggle.all` | Agenda: toggle the marks of all entries |
| `Shift+5` | `org.agenda.bulk.mark.regexp` | Agenda: mark the entries matching a regexp |
| `Shift+B` | `org.agenda.bulk.action` | Agenda: an action on the marked entries |
| `/` | `org.agenda.filter` | Agenda: filter by tags, categories, effort and regexps |
| `\` | `org.agenda.filter.tag` | Agenda: filter by tag |
| `<` | `org.agenda.filter.category` | Agenda: filter by the line's category |
| `=` | `org.agenda.filter.regexp` | Agenda: filter by a regexp |
| `_` | `org.agenda.filter.effort` | Agenda: filter by effort |
| `Shift+6` | `org.agenda.filter.top.headline` | Agenda: filter by the line's top headline |
| `Shift+\` | `org.agenda.filter.remove` | Agenda: remove all filters |
| `v a` | `org.agenda.archives` | Agenda: include archived trees |
| `v Shift+A` | `org.agenda.archives.files` | Agenda: include archived trees and archive files |
| `Shift+I` | `org.clock.in` | Clock in |
| `Shift+O` | `org.clock.out` | Clock out |
| `Shift+X` | `org.clock.cancel` | Cancel the clock |
| `t` | `org.todo` | Change the TODO state |
| `+`, `Shift+Up` | `org.priority.up` | Raise the priority |
| `-`, `Shift+Down` | `org.priority.down` | Lower the priority |
| `,` | `org.priority` | Set the priority |
| `Ctrl+C Ctrl+X p` | `org.property.set` | Set a property of the entry |
| `e` | `org.effort.set` | Set the effort of the entry |
| `Ctrl+C Ctrl+W` | `org.refile` | Refile the entry |
| `z` | `org.add.note` | Add a note to the entry |
| `Ctrl+C Ctrl+O` | `org.open` | Open the link or thing at the cursor |
| `Ctrl+C l` | `org.link.store` | Store a link to the place at the cursor |
| `Shift+4`, `Ctrl+C Ctrl+X Ctrl+S`, `Ctrl+C Shift+4` | `org.archive.subtree` | Archive the subtree |
| `Ctrl+C Ctrl+X Shift+A` | `org.archive.sibling` | Archive the subtree under its Archive sibling |
| `Ctrl+C Ctrl+X a` | `org.archive.tag` | Toggle the ARCHIVE tag |
| `Ctrl+C Ctrl+X Ctrl+A` | `org.archive.default` | Archive with the default command |
| `a` | `org.archive.confirm` | Archive with the default command, once confirmed |
| `Shift+Right` | `org.date.later` | Move the date one day later |
| `Shift+Left` | `org.date.earlier` | Move the date one day earlier |
| `Ctrl+C Ctrl+S` | `org.schedule` | Schedule the entry |
| `Ctrl+C Ctrl+D` | `org.deadline` | Set the deadline of the entry |
| `g ?` | `org-lsp.help` | Show the keys |
| `q` | `org-lsp.agenda.close` | Agenda: close |

#### In the date prompt

| Keys | Command | What it does |
| --- | --- | --- |
| `l`, `Shift+Right` | `org.calendar.day.later` | Date prompt: one day later |
| `h`, `Shift+Left` | `org.calendar.day.earlier` | Date prompt: one day earlier |
| `j`, `Shift+Down` | `org.calendar.week.later` | Date prompt: one week later |
| `k`, `Shift+Up` | `org.calendar.week.earlier` | Date prompt: one week earlier |
| `Alt+Shift+Right` | `org.calendar.month.later` | Date prompt: one month later |
| `Alt+Shift+Left` | `org.calendar.month.earlier` | Date prompt: one month earlier |
| `Alt+Shift+Down` | `org.calendar.year.later` | Date prompt: one year later |
| `Alt+Shift+Up` | `org.calendar.year.earlier` | Date prompt: one year earlier |
| `>` | `org.calendar.scroll.later` | Date prompt: show the next months |
| `<` | `org.calendar.scroll.earlier` | Date prompt: show the previous months |
| `.` | `org.calendar.today` | Date prompt: go to today |
| `Enter` | `org.calendar.select` | Date prompt: take the day at the cursor |
| `i` | `org.calendar.type` | Date prompt: type the date |
| `t` | `org.calendar.time` | Date prompt: pick the time, or go back to the day |
| `q`, `Escape` | `org.calendar.cancel` | Date prompt: cancel |

¹ Where the command does not apply, the key does what it does without
the extension: `Shift+Up` off a timestamp or headline selects, `Tab` off
a headline, block or drawer indents, `Enter` in a source block's code
is VS Code's own.

#### The leader keys, for VSCodeVim

VS Code cannot bind keys that start with the leader without taking
them from Vim. Add them to VSCodeVim's settings instead (they apply in
every file; outside Org files the commands say they do not apply):

```json
"vim.normalModeKeyBindingsNonRecursive": [
  { "before": ["<leader>", "o", "a"], "commands": ["org.agenda.dispatch"] },
  { "before": ["<leader>", "o", "x", "i"], "commands": ["org.clock.in"] },
  { "before": ["<leader>", "o", "x", "x"], "commands": ["org.clock.in.last"] },
  { "before": ["<leader>", "o", "x", "R"], "commands": ["org.clock.resolve"] },
  { "before": ["<leader>", "o", "x", "o"], "commands": ["org.clock.out"] },
  { "before": ["<leader>", "o", "x", "q"], "commands": ["org.clock.cancel"] },
  { "before": ["<leader>", "o", "x", "j"], "commands": ["org.clock.goto"] },
  { "before": ["<leader>", "o", "x", "d"], "commands": ["org.clock.display"] },
  { "before": ["<leader>", "o", "x", "r"], "commands": ["org.clock.report"] },
  { "before": ["<leader>", "o", "x", "u"], "commands": ["org.dblock.update"] },
  { "before": ["<leader>", "o", "i", "x"], "commands": ["org.dblock.insert"] },
  { "before": ["<leader>", "o", "x", "a"], "commands": ["org.attach"] },
  { "before": ["<leader>", "o", ","], "commands": ["org.priority"] },
  { "before": ["<leader>", "o", "x", "p"], "commands": ["org.property.set"] },
  { "before": ["<leader>", "o", "x", "e"], "commands": ["org.effort.set"] },
  { "before": ["<leader>", "o", "x", "E"], "commands": ["org.effort.increase"] },
  { "before": ["<leader>", "o", "i", "c"], "commands": ["org.cite.insert"] },
  { "before": ["<leader>", "o", "i", "C"], "commands": ["org.cite.insert.prefix"] },
  { "before": ["<leader>", "o", "t"], "commands": ["org.tags.set"] },
  { "before": ["<leader>", "<CR>"], "commands": ["org.meta.return"] },
  { "before": ["<leader>", "o", "i", "T"], "commands": ["org.insert.todo.heading"] },
  { "before": ["<leader>", "o", "i", "h"], "commands": ["org.insert.heading.after"] },
  { "before": ["<leader>", "o", "i", "t"], "commands": ["org.insert.todo.heading.after"] },
  { "before": ["<leader>", "o", "K"], "commands": ["org.meta.up"] },
  { "before": ["<leader>", "o", "J"], "commands": ["org.meta.down"] },
  { "before": ["<leader>", "o", "T", "d"], "commands": ["org.shiftmeta.up"] },
  { "before": ["<leader>", "o", "T", "O"], "commands": ["org.shiftmeta.down"] },
  { "before": ["<leader>", "o", "T", "o"], "commands": ["org.table.insert.row.below"] },
  { "before": ["<leader>", "o", "-"], "commands": ["org.table.hline"] },
  { "before": ["<leader>", "o", "<CR>"], "commands": ["org.table.hline.move"] },
  { "before": ["<leader>", "o", "<Space>"], "commands": ["org.table.blank.field"] },
  { "before": ["<leader>", "o", "^"], "commands": ["org.sort"] },
  { "before": ["<leader>", "o", "/"], "commands": ["org.sparse.tree"] },
  { "before": ["<leader>", "o", "s", "d"], "commands": ["org.cut.subtree"] },
  { "before": ["<leader>", "o", "s", "y"], "commands": ["org.copy.subtree"] },
  { "before": ["<leader>", "o", "s", "p"], "commands": ["org.paste.subtree"] },
  { "before": ["<leader>", "o", "r"], "commands": ["org.refile"] },
  { "before": ["<leader>", "o", "y"], "commands": ["org.refile.copy"] },
  { "before": ["<leader>", "o", "n", "a"], "commands": ["org.add.note"] },
  { "before": ["<leader>", "o", "b", "t"], "commands": ["org.babel.tangle"] },
  { "before": ["<leader>", "o", "b", "e"], "commands": ["org.babel.execute"] },
  { "before": ["<leader>", "o", "b", "b"], "commands": ["org.babel.execute.buffer"] },
  { "before": ["<leader>", "o", "b", "s"], "commands": ["org.babel.execute.subtree"] },
  { "before": ["<leader>", "o", "b", "k"], "commands": ["org.babel.remove_result"] },
  { "before": ["<leader>", "o", "j"], "commands": ["org.goto"] },
  { "before": ["<leader>", "o", "f"], "commands": ["org.footnote.action"] },
  { "before": ["<leader>", "o", "F"], "commands": ["org.footnote.menu"] },
  { "before": ["<leader>", "o", ";"], "commands": ["org.toggle.comment"] },
  { "before": ["<leader>", "o", "i", "e"], "commands": ["org.emphasize"] },
  { "before": ["<leader>", "o", "i", "b"], "commands": ["org.structure.template"] },
  { "before": ["<leader>", "o", "i", "D"], "commands": ["org.drawer.insert"] },
  { "before": ["<leader>", "o", "s", "c"], "commands": ["org.subtree.clone"] },
  { "before": ["<leader>", "o", "D"], "commands": ["org.table.copy.down"] },
  { "before": ["<leader>", "o", "+"], "commands": ["org.table.sum"] },
  { "before": ["<leader>", "o", "?"], "commands": ["org.table.field.info"] },
  { "before": ["<leader>", "o", "i", "<Bar>"], "commands": ["org.table.create"] },
  { "before": ["<leader>", "o", "b", "n"], "commands": ["org.babel.next"] },
  { "before": ["<leader>", "o", "b", "p"], "commands": ["org.babel.previous"] },
  { "before": ["<leader>", "o", "b", "g"], "commands": ["org.babel.goto"] },
  { "before": ["<leader>", "o", "b", "i"], "commands": ["org.babel.lob.ingest"] },
  { "before": ["<leader>", "o", "x", "0"], "commands": ["org.timer.start"] },
  { "before": ["<leader>", "o", "x", "."], "commands": ["org.timer.insert"] },
  { "before": ["<leader>", "o", "x", "-"], "commands": ["org.timer.item"] },
  { "before": ["<leader>", "o", "x", ","], "commands": ["org.timer.pause"] },
  { "before": ["<leader>", "o", "x", "_"], "commands": ["org.timer.stop"] },
  { "before": ["<leader>", "o", "x", ";"], "commands": ["org.timer.set"] },
  { "before": ["<leader>", "o", "e"], "commands": ["org.export.dispatch"] },
  { "before": ["<leader>", "o", "*"], "commands": ["org.table.recalculate"] },
  { "before": ["<leader>", "o", "o"], "commands": ["org.open"] },
  { "before": ["<leader>", "o", "l", "s"], "commands": ["org.link.store"] },
  { "before": ["<leader>", "o", "l", "i"], "commands": ["org.link.insert"] },
  { "before": ["<leader>", "o", "$"], "commands": ["org.archive.subtree"] },
  { "before": ["<leader>", "o", "x", "A"], "commands": ["org.archive.sibling"] },
  { "before": ["<leader>", "o", "A"], "commands": ["org.archive.tag"] },
  { "before": ["<leader>", "o", "c"], "commands": ["org.capture"] },
  { "before": ["<leader>", "o", "i", "s"], "commands": ["org.schedule"] },
  { "before": ["<leader>", "o", "i", "d"], "commands": ["org.deadline"] },
  { "before": ["<leader>", "o", "i", "."], "commands": ["org.timestamp.insert"] },
  { "before": ["<leader>", "o", "i", "!"], "commands": ["org.timestamp.insert.inactive"] }
]
```

### Every command

| Command | In the Command Palette |
| --- | --- |
| `org.agenda.dispatch` | Org: Agenda commands |
| `org.agenda.list` | Org: Agenda for the current week or day |
| `org.agenda.todos` | Org: List of all TODO entries |
| `org.agenda.later` | Org: Agenda: go forward in time |
| `org.agenda.earlier` | Org: Agenda: go backward in time |
| `org.agenda.today` | Org: Agenda: go to today |
| `org.agenda.query.add` | Org: Agenda: search for another word |
| `org.agenda.query.subtract` | Org: Agenda: search without a word |
| `org.agenda.query.add.regexp` | Org: Agenda: search for another regexp |
| `org.agenda.query.subtract.regexp` | Org: Agenda: search without a regexp |
| `org.agenda.goto.date` | Org: Agenda: go to a date |
| `org.agenda.day` | Org: Agenda: day view |
| `org.agenda.week` | Org: Agenda: week view |
| `org.agenda.fortnight` | Org: Agenda: fortnight view |
| `org.agenda.month` | Org: Agenda: month view |
| `org.agenda.year` | Org: Agenda: year view |
| `org.agenda.redo` | Org: Agenda: rebuild |
| `org.agenda.goto` | Org: Agenda: go to the entry |
| `org.agenda.goto.other` | Org: Agenda: go to the entry in another window |
| `org.agenda.save` | Org: Save all Org files |
| `org.agenda.log` | Org: Agenda: log mode, what was closed and clocked |
| `org.agenda.habits` | Org: Agenda: show or hide habits |
| `org.agenda.write` | Org: Agenda: write it to a file |
| `org.agenda.clockreport` | Org: Agenda: the clock report |
| `org.agenda.entry.text` | Org: Agenda: entry text mode, some text of each entry under its line |
| `org.agenda.follow` | Org: Agenda: follow mode, the cursor's entry in another window |
| `org.agenda.bulk.mark` | Org: Agenda: mark the entry for a bulk action |
| `org.agenda.bulk.unmark` | Org: Agenda: unmark the entry |
| `org.agenda.bulk.unmark.all` | Org: Agenda: unmark all entries |
| `org.agenda.bulk.toggle` | Org: Agenda: toggle the mark of the entry |
| `org.agenda.bulk.mark.all` | Org: Agenda: mark all entries |
| `org.agenda.bulk.toggle.all` | Org: Agenda: toggle the marks of all entries |
| `org.agenda.bulk.mark.regexp` | Org: Agenda: mark the entries matching a regexp |
| `org.agenda.bulk.action` | Org: Agenda: an action on the marked entries |
| `org.agenda.filter` | Org: Agenda: filter by tags, categories, effort and regexps |
| `org.agenda.filter.tag` | Org: Agenda: filter by tag |
| `org.agenda.filter.category` | Org: Agenda: filter by the line's category |
| `org.agenda.filter.regexp` | Org: Agenda: filter by a regexp |
| `org.agenda.filter.effort` | Org: Agenda: filter by effort |
| `org.agenda.filter.top.headline` | Org: Agenda: filter by the line's top headline |
| `org.agenda.filter.remove` | Org: Agenda: remove all filters |
| `org.agenda.archives` | Org: Agenda: include archived trees |
| `org.agenda.archives.files` | Org: Agenda: include archived trees and archive files |
| `org.clock.in` | Org: Clock in |
| `org.clock.in.last` | Org: Clock in the last clocked task |
| `org.clock.resolve` | Org: Resolve a dangling clock |
| `org.clock.out` | Org: Clock out |
| `org.clock.cancel` | Org: Cancel the clock |
| `org.clock.goto` | Org: Go to the clocked entry |
| `org.clock.display` | Org: Show the time clocked under each headline |
| `org.clock.report` | Org: Insert or update a clock table |
| `org.dblock.update` | Org: Update the dynamic block |
| `org.dblock.insert` | Org: Insert a dynamic block |
| `org.attach` | Org: Attachments of the entry |
| `org.todo` | Org: Change the TODO state |
| `org.todo.next` | Org: Cycle TODO state forward |
| `org.todo.previous` | Org: Cycle TODO state backward |
| `org.priority.up` | Org: Raise the priority |
| `org.priority.down` | Org: Lower the priority |
| `org.priority` | Org: Set the priority |
| `org.property.set` | Org: Set a property of the entry |
| `org.property.delete` | Org: Delete a property of the entry |
| `org.property.delete.globally` | Org: Delete a property from every entry of the file |
| `org.effort.set` | Org: Set the effort of the entry |
| `org.effort.increase` | Org: Set the effort to the next allowed value |
| `org.ordered.toggle` | Org: Toggle whether subtasks must be done in order (ORDERED) |
| `org.cite.insert` | Org: Insert a citation, or edit the one at the cursor |
| `org.cite.insert.prefix` | Org: Delete the citation or reference at the cursor, or insert one with a style |
| `org.tags.set` | Org: Set the tags of the headline |
| `org.checkbox.toggle` | Org: Toggle the checkbox |
| `org.return` | Org: New line, as Org's RET makes it |
| `org.return.item` | Org: New line; on a list item, the next item |
| `org.meta.return` | Org: New heading, item or table row |
| `org.insert.todo.heading` | Org: New TODO heading, or item with a checkbox |
| `org.insert.heading.after` | Org: New heading after the subtree |
| `org.insert.todo.heading.after` | Org: New TODO heading after the subtree |
| `org.meta.left` | Org: Promote the heading; outdent the item; move the table column left |
| `org.meta.right` | Org: Demote the heading; indent the item, the drawer or the block; move the table column right |
| `org.shiftmeta.left` | Org: Promote the subtree; outdent the item with its children; delete the table column |
| `org.shiftmeta.right` | Org: Demote the subtree; indent the item with its children; insert a table column |
| `org.meta.up` | Org: Move the subtree, the item or the table row up |
| `org.meta.down` | Org: Move the subtree, the item or the table row down |
| `org.shiftmeta.up` | Org: Delete the table row |
| `org.shiftmeta.down` | Org: Insert a table row above |
| `org.table.insert.row.below` | Org: Insert a table row below |
| `org.table.next.field` | Org: Go to the next table field, making a row after the last |
| `org.table.previous.field` | Org: Go to the previous table field |
| `org.table.hline` | Org: Insert a rule below the table row |
| `org.table.hline.above` | Org: Insert a rule above the table row |
| `org.table.hline.move` | Org: Insert a rule below the table row and go below it |
| `org.table.blank.field` | Org: Blank the table field |
| `org.sort` | Org: Sort the entries, the items or the table rows |
| `org.sparse.tree` | Org: Show a sparse tree |
| `org.cut.subtree` | Org: Cut the subtree |
| `org.copy.subtree` | Org: Copy the subtree |
| `org.paste.subtree` | Org: Paste the subtree |
| `org.refile` | Org: Refile the entry |
| `org.refile.copy` | Org: Copy the entry to a refile target |
| `org.refile.goto` | Org: Go to a refile target |
| `org.refile.goto.last` | Org: Go to where the last refile put its entry |
| `org.add.note` | Org: Add a note to the entry |
| `org.babel.tangle` | Org: Tangle the file's code blocks |
| `org.babel.execute` | Org: Evaluate the code block or call at the cursor through Emacs |
| `org.babel.execute.buffer` | Org: Evaluate the file's code blocks through Emacs |
| `org.babel.execute.subtree` | Org: Evaluate the subtree's code blocks through Emacs |
| `org.babel.remove_result` | Org: Remove the results of the code block at the cursor |
| `org.heading.next` | Org: Move to the next heading |
| `org.heading.previous` | Org: Move to the previous heading |
| `org.heading.forward` | Org: Move to the next heading of the same level |
| `org.heading.backward` | Org: Move to the previous heading of the same level |
| `org.heading.up` | Org: Move to the parent heading |
| `org.link.next` | Org: Move to the next link |
| `org.link.previous` | Org: Move to the previous link |
| `org.goto` | Org: Go to a heading of the file |
| `org.footnote.action` | Org: Make a footnote, or go between a footnote and its definition |
| `org.footnote.menu` | Org: Sort, renumber, normalize or delete footnotes |
| `org.toggle.item` | Org: Turn the line into an item, or the item into a line |
| `org.toggle.heading` | Org: Turn the line into a heading, or the heading into a line |
| `org.toggle.comment` | Org: Toggle the COMMENT keyword of the heading |
| `org.emphasize` | Org: Insert emphasis markers |
| `org.structure.template` | Org: Insert a block (structure template) |
| `org.drawer.insert` | Org: Insert a drawer |
| `org.subtree.clone` | Org: Clone the subtree, shifting its dates |
| `org.table.copy.down` | Org: Copy the field down to the next row |
| `org.table.sum` | Org: Sum the numbers of the table column |
| `org.table.field.info` | Org: Show the references of the table field |
| `org.table.create` | Org: Insert an empty table |
| `org.table.import` | Org: Import a file as a table |
| `org.table.export` | Org: Export the table to a file |
| `org.babel.next` | Org: Move to the next code block |
| `org.babel.previous` | Org: Move to the previous code block |
| `org.babel.goto` | Org: Go to a named code block |
| `org.babel.lob.ingest` | Org: Add the file's named code blocks to the Library of Babel |
| `org.block.next` | Org: Move to the next block |
| `org.block.previous` | Org: Move to the previous block |
| `org.timer.start` | Org: Start the relative timer |
| `org.timer.insert` | Org: Insert the timer's value |
| `org.timer.item` | Org: Insert a list item with the timer's value |
| `org.timer.pause` | Org: Pause or continue the timer |
| `org.timer.stop` | Org: Stop the timer |
| `org.timer.set` | Org: Set a countdown timer |
| `org.export.dispatch` | Org: Export the file through Emacs |
| `org.table.recalculate` | Org: Recalculate the table |
| `org.table.recalculate.line` | Org: Recalculate the table's current line |
| `org.table.iterate` | Org: Recalculate the table until it does not change |
| `org.open` | Org: Open the link or thing at the cursor |
| `org.link.store` | Org: Store a link to the place at the cursor |
| `org.link.insert` | Org: Insert a link, or edit the one at the cursor |
| `org.link.insert.last` | Org: Insert the last stored link |
| `org.link.insert.all` | Org: Insert all stored links |
| `org.id.create` | Org: Create an ID for the entry |
| `org.archive.subtree` | Org: Archive the subtree |
| `org.archive.sibling` | Org: Archive the subtree under its Archive sibling |
| `org.archive.tag` | Org: Toggle the ARCHIVE tag |
| `org.archive.default` | Org: Archive with the default command |
| `org.archive.confirm` | Org: Archive with the default command, once confirmed |
| `org.archive.done` | Org: Archive the subtrees without open TODO items |
| `org.archive.tag.done` | Org: Tag the subtrees without open TODO items as archived |
| `org.capture` | Org: Capture |
| `org.capture.finish` | Org: Capture: finish |
| `org.capture.abort` | Org: Capture: abort |
| `org.capture.refile` | Org: Capture: refile the entry |
| `org.timestamp.up` | Org: Increase the part of the timestamp at the cursor |
| `org.timestamp.down` | Org: Decrease the part of the timestamp at the cursor |
| `org.date.later` | Org: Move the date one day later |
| `org.date.earlier` | Org: Move the date one day earlier |
| `org.date.later.hours` | Org: Agenda: move the time one hour later |
| `org.date.earlier.hours` | Org: Agenda: move the time one hour earlier |
| `org.date.later.minutes` | Org: Agenda: move the time later by the rounding minutes |
| `org.date.earlier.minutes` | Org: Agenda: move the time earlier by the rounding minutes |
| `org.shift.up` | Org: Increase the timestamp part, raise the priority, or go to the previous item |
| `org.shift.down` | Org: Decrease the timestamp part, lower the priority, or go to the next item |
| `org.shift.left` | Org: Move the date a day earlier, or cycle the TODO state or the list's bullets backward |
| `org.shift.right` | Org: Move the date a day later, or cycle the TODO state or the list's bullets forward |
| `org.schedule` | Org: Schedule the entry |
| `org.deadline` | Org: Set the deadline of the entry |
| `org.schedule.remove` | Org: Remove the scheduled date of the entry |
| `org.deadline.remove` | Org: Remove the deadline of the entry |
| `org.schedule.delay` | Org: Delay the scheduled date until a day |
| `org.deadline.warning` | Org: Warn of the deadline from a day |
| `org.timestamp.insert` | Org: Insert or change a timestamp |
| `org.timestamp.insert.inactive` | Org: Insert or change an inactive timestamp |
| `org.timestamp.change` | Org: Change the timestamp at the cursor |
| `org.calendar.day.later` | Org: Date prompt: one day later |
| `org.calendar.day.earlier` | Org: Date prompt: one day earlier |
| `org.calendar.week.later` | Org: Date prompt: one week later |
| `org.calendar.week.earlier` | Org: Date prompt: one week earlier |
| `org.calendar.month.later` | Org: Date prompt: one month later |
| `org.calendar.month.earlier` | Org: Date prompt: one month earlier |
| `org.calendar.year.later` | Org: Date prompt: one year later |
| `org.calendar.year.earlier` | Org: Date prompt: one year earlier |
| `org.calendar.scroll.later` | Org: Date prompt: show the next months |
| `org.calendar.scroll.earlier` | Org: Date prompt: show the previous months |
| `org.calendar.today` | Org: Date prompt: go to today |
| `org.calendar.select` | Org: Date prompt: take the day at the cursor |
| `org.calendar.type` | Org: Date prompt: type the date |
| `org.calendar.time` | Org: Date prompt: pick the time, or go back to the day |
| `org.calendar.cancel` | Org: Date prompt: cancel |
| `org-lsp.cycle` | Org: Cycle the visibility of the subtree, block or drawer at the cursor |
| `org-lsp.help` | Org: Show the keys |
| `org-lsp.cycleGlobal` | Org: Cycle the visibility of the whole file |

<!-- END generated -->

## How they behave

VS Code has no count, so each command moves by one step. Questions are
asked in VS Code's own way:

- **A date** (scheduling, a deadline, `j` in the agenda) opens Emacs's
  calendar beside the file, with the keys under *In the date prompt*.
  `i` there asks for the date in an input box, with the timestamp the
  answer makes shown as you type. `t` picks a time: a `Time:` line shows
  it, `Shift+Left` and `Shift+Right` choose the hour or the minutes,
  `Shift+Up` and `Shift+Down` move it up or down (the minutes by
  `org_time_stamp_rounding_minutes` in `org-lsp.settings`), and `t` again goes
  back to the days, keeping the time.
- **One key**, such as a TODO keyword with a key or a capture template, is
  picked from a list that shows each choice's key (`[t] TODO`). Type to
  narrow the list, then press `Enter`.
- **A choice from a list**, such as a refile target, is picked from a
  list.
- **Text**, such as a capture prompt, is typed in an input box.
- **A state note** (a keyword with `@`, as `WAIT(w@)`) is written in a note
  buffer that opens like a capture buffer, as Emacs's `*Org Note*`:
  `Ctrl+C Ctrl+C` stores it, `Ctrl+C Ctrl+K` changes the state without it.

What Emacs says when a command is done (`Priority of current item set to
B`, `Scheduled to <2026-05-22 Fri>`) shows in the status bar for a few
seconds; a command that cannot run says why in a warning.

## Changing them

Open **Preferences: Open Keyboard Shortcuts (JSON)** and add entries to
`keybindings.json`. Each binding needs a `when` clause that says where it
applies:

| Where | `when` |
| --- | --- |
| Org files | `editorTextFocus && editorLangId == 'org'` |
| The agenda | `editorTextFocus && editorLangId == 'org-agenda'` |
| The date prompt | `editorTextFocus && editorLangId == 'org-date'` |
| A capture buffer | `editorTextFocus && editorLangId == 'org' && resourceFilename =~ /^CAPTURE-/` |

To **add** a key, bind it to the command's name, from the second column of
the tables above:

```json
[
  { "key": "ctrl+alt+a", "command": "org.agenda.dispatch" },
  { "key": "ctrl+alt+c", "command": "org.capture" },
  { "key": "alt+t", "command": "org.todo", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+up", "command": "org.shift.up", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+down", "command": "org.shift.down", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+left", "command": "org.shift.left", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+right", "command": "org.shift.right", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+s", "command": "org.schedule", "when": "editorTextFocus && editorLangId == 'org'" }
]
```

The first two have no `when`, so they work anywhere, as Emacs's
`C-c a` and `C-c c` do. With `"org-lsp.keys": "custom"` these are the only
keys there are.

To **remove** a default key, add the same key and command with a `-` in
front of the command. To **change** one, remove it and add the new key:

```json
[
  { "key": "j", "command": "-org.agenda.goto.date" },
  { "key": "shift+j", "command": "org.agenda.goto.date", "when": "editorTextFocus && editorLangId == 'org-agenda'" }
]
```

The same can be done in **Preferences: Open Keyboard Shortcuts**: search
for `org.`, then right-click a binding to change or remove it.
