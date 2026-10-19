; The agenda (Agenda.org) in Helix: the language `org-agenda` that
; editors/helix/languages.toml gives the file the server writes it to,
; parsed with the Org grammar. To the grammar the agenda is paragraphs of
; words, so a line is told by its words, as the server's semantic tokens
; color it in the other editors (`agenda*` in
; server/org-lsp/src/legend.rs), with the default leaders of
; `org-agenda-scheduled-leaders` and `org-agenda-deadline-leaders` and
; the default prefix (the category, then the time). What the text does
; not show is not colored: which day is today, how near a deadline is
; past its leader, the line the clock runs on.
;
; Helix gives a query a few hundred matches at a time, and drops the
; oldest when there are more: so a line's patterns start at its first
; words (no `(expr)*` before them), and a pattern of one word names what
; its word holds (`(expr "[")`).

; The title of a view and the other lines of its structure
; (`org-agenda-structure`): those that start with neither a category
; (`work:`), a day, a time nor a mark.
((paragraph
   . (expr) @_first @markup.heading.agenda
   . (expr)* @markup.heading.agenda
   . "\n")
 (#not-match? @_first "(:|^(Mon|Tues|Wednes|Thurs|Fri|Satur|Sun)day$|^[0-9]|^>)"))
(("\n"
   . (expr) @_first @markup.heading.agenda
   . (expr)* @markup.heading.agenda
   . "\n")
 (#not-match? @_first "(:|^(Mon|Tues|Wednes|Thurs|Fri|Satur|Sun)day$|^[0-9]|^>)"))

; The days (`org-agenda-date`), the weekend's (`org-agenda-date-weekend`).
(("\n"
   . (expr) @_day @markup.heading.date
   . (expr)+ @markup.heading.date
   . "\n")
 (#match? @_day "^(Mon|Tues|Wednes|Thurs|Fri)day$"))
(("\n"
   . (expr) @_day @markup.italic.weekend
   . (expr)+ @markup.italic.weekend
   . "\n")
 (#match? @_day "^(Satur|Sun)day$"))

; The time grid (`org-time-grid`) and the current time
; (`org-agenda-current-time`).
(("\n"
   . (expr ":") @_time @comment.timegrid
   . (expr "-") @_rule @comment.timegrid
   . "\n")
 (#match? @_time "^[0-9]{1,2}:[0-9]{2}\\.+$")
 (#match? @_rule "^-+$"))
(("\n"
   . (expr ":") @_time @markup.bold.now
   . (expr) @_now @markup.bold.now
   . (expr)* @markup.bold.now
   . "\n")
 (#match? @_time "^[0-9]{1,2}:[0-9]{2}\\.+$")
 (#eq? @_now "now"))

; Scheduled entries (`org-scheduled-today`), and those scheduled
; before (`org-scheduled-previously`).
(("\n"
   . (expr ":") @diff.plus.scheduled
   . (expr ":") @_leader @diff.plus.scheduled
   . (expr)* @diff.plus.scheduled
   . "\n")
 (#eq? @_leader "Scheduled:"))
(("\n"
   . (expr ":") @diff.plus.scheduled
   . (expr) @diff.plus.scheduled
   . (expr ":") @_leader @diff.plus.scheduled
   . (expr)* @diff.plus.scheduled
   . "\n")
 (#eq? @_leader "Scheduled:"))
(("\n"
   . (expr ":") @diff.delta.scheduled
   . (expr ".") @_leader @diff.delta.scheduled
   . (expr)* @diff.delta.scheduled
   . "\n")
 (#match? @_leader "^Sched\\."))
(("\n"
   . (expr ":") @diff.delta.scheduled
   . (expr) @diff.delta.scheduled
   . (expr ".") @_leader @diff.delta.scheduled
   . (expr)* @diff.delta.scheduled
   . "\n")
 (#match? @_leader "^Sched\\."))

; Deadlines: coming (`org-upcoming-deadline`), and today or past
; (`org-warning`).
(("\n"
   . (expr ":") @diff.delta.deadline
   . (expr) @_in @diff.delta.deadline
   . (expr) @_days @diff.delta.deadline
   . (expr ":") @_unit @diff.delta.deadline
   . (expr)* @diff.delta.deadline
   . "\n")
 (#eq? @_in "In")
 (#match? @_days "^[0-9]+$")
 (#eq? @_unit "d.:"))
(("\n"
   . (expr ":") @diff.delta.deadline
   . (expr) @diff.delta.deadline
   . (expr) @_in @diff.delta.deadline
   . (expr) @_days @diff.delta.deadline
   . (expr ":") @_unit @diff.delta.deadline
   . (expr)* @diff.delta.deadline
   . "\n")
 (#eq? @_in "In")
 (#match? @_days "^[0-9]+$")
 (#eq? @_unit "d.:"))
(("\n"
   . (expr ":") @diff.minus.deadline
   . (expr ":") @_leader @diff.minus.deadline
   . (expr)* @diff.minus.deadline
   . "\n")
 (#eq? @_leader "Deadline:"))
(("\n"
   . (expr ":") @diff.minus.deadline
   . (expr) @diff.minus.deadline
   . (expr ":") @_leader @diff.minus.deadline
   . (expr)* @diff.minus.deadline
   . "\n")
 (#eq? @_leader "Deadline:"))
(("\n"
   . (expr ":") @diff.minus.deadline
   . (expr) @_days @diff.minus.deadline
   . (expr ".") @_unit @diff.minus.deadline
   . (expr ":") @_ago @diff.minus.deadline
   . (expr)* @diff.minus.deadline
   . "\n")
 (#match? @_days "^[0-9]+$")
 (#eq? @_unit "d.")
 (#eq? @_ago "ago:"))
(("\n"
   . (expr ":") @diff.minus.deadline
   . (expr) @diff.minus.deadline
   . (expr) @_days @diff.minus.deadline
   . (expr ".") @_unit @diff.minus.deadline
   . (expr ":") @_ago @diff.minus.deadline
   . (expr)* @diff.minus.deadline
   . "\n")
 (#match? @_days "^[0-9]+$")
 (#eq? @_unit "d.")
 (#eq? @_ago "ago:"))

; Done entries (`org-agenda-done`), and what log mode shows closed.
(("\n"
   . (expr ":") @comment.done
   . (expr) @_keyword @comment.done
   . (expr)* @comment.done
   . "\n")
 (#any-of? @_keyword "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED" "Closed:"))
(("\n"
   . (expr ":") @comment.done
   . (expr) @comment.done
   . (expr) @_keyword @comment.done
   . (expr)* @comment.done
   . "\n")
 (#any-of? @_keyword "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED" "Closed:"))
(("\n"
   . (expr ":") @comment.done
   . (expr) @comment.done
   . (expr) @comment.done
   . (expr) @_keyword @comment.done
   . (expr)* @comment.done
   . "\n")
 (#any-of? @_keyword "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED" "Closed:"))
(("\n"
   . (expr ":") @comment.done
   . (expr) @comment.done
   . (expr) @comment.done
   . (expr) @comment.done
   . (expr) @_keyword @comment.done
   . (expr)* @comment.done
   . "\n")
 (#any-of? @_keyword "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED" "Closed:"))
(("\n"
   . (expr ":") @comment.done
   . (expr) @comment.done
   . (expr) @comment.done
   . (expr) @comment.done
   . (expr) @comment.done
   . (expr) @_keyword @comment.done
   . (expr)* @comment.done
   . "\n")
 (#any-of? @_keyword "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED" "Closed:"))

; In an entry's line: its keyword (also right after a long category,
; `side-project:TODO`), priority and tags.
((expr) @diff.minus.todo
 (#any-of? @diff.minus.todo
   "TODO" "NEXT" "STARTED" "WAIT" "WAITING" "HOLD" "PROJ" "SOMEDAY" "INPROGRESS"))
((expr) @diff.plus.done
 (#any-of? @diff.plus.done "DONE" "CANCELLED" "CANCELED" "CANCEL" "KILL" "FIXED"))
((expr ":") @diff.minus.todo
 (#match? @diff.minus.todo "^[^\\s:]+:(TODO|NEXT|STARTED|WAIT|WAITING|HOLD|PROJ|SOMEDAY|INPROGRESS)$"))
((expr "#") @constant.numeric.priority
 (#match? @constant.numeric.priority "^\\[#[A-Z0-9]+\\]$"))
((expr ":") @tag
 (#match? @tag "^:([^\\s:]+:|:)+$"))

; The clock report: a table.
(row "|" @punctuation.special.table)
(cell "|" @punctuation.special.table)
(hr) @punctuation.special.table
