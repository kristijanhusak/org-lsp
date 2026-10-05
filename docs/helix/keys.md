# Keys in Helix

Helix has no plugins, so org-lsp sets no keys there. Helix also cannot run
a server's command from a key and tell it where the cursor is. What Helix
does have is the code action menu, and the server offers its editing
commands there.

Press `Space a` (`code_action`) in an Org file to see the changes that
apply at the cursor:

| Action | What it does |
| --- | --- |
| `org.todo.next` | Cycle the TODO state forward |
| `org.todo.previous` | Cycle the TODO state backward |
| `org.priority.up` | Raise the priority |
| `org.priority.down` | Lower the priority |
| `org.timestamp.up` | Increase the part of the timestamp at the cursor |
| `org.timestamp.down` | Decrease the part of the timestamp at the cursor |
| `org.date.later` | Move the date one day later |
| `org.date.earlier` | Move the date one day earlier |
| `org.clock.in` | Clock in on the entry (a clock that runs elsewhere stops) |
| `org.clock.out` | Clock out, while a clock runs |
| `org.clock.cancel` | Cancel the clock, while one runs |

The menu shows only those that would change something. Helix has no
status line for the clock; `org-lsp agenda --log` in a terminal shows what
was clocked. Helix cannot ask
for text on a server's behalf, so a TODO state that is set up to take a
note (`WAIT(w@)`) gets its note line without text, and scheduling,
deadlines, refile and capture are not available in Helix.

## Changing the key

The menu is Helix's own `code_action` command, so its key is set in
Helix's `config.toml` like any other:

```toml
[keys.normal]
"C-." = "code_action"

[keys.normal.space]
o = "code_action"
```

Helix cannot bind a key to one particular action.

The other options are in [configuration.md](configuration.md).
