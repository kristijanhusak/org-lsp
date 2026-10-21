#!/usr/bin/env python3
"""A sample Org workspace for the LSP tests, written afresh into a
directory: `make(dir)` returns (dir, options)."""
import os
import shutil

GTD = """\
#+TITLE: Getting things done
#+AUTHOR: Tester
#+TODO: TODO NEXT WAIT(w@) | DONE(d!) CANCELLED(c@)
#+TAGS: work home @phone urgent
#+PROPERTY: Effort_ALL 0:10 0:30 1:00 2:00
#+COLUMNS: %40ITEM %TODO %Effort{:} %CLOCKSUM
#+BIBLIOGRAPHY: refs.bib
#+STARTUP: content

* Tasks
** TODO [#A] Write the report                                        :work:
   SCHEDULED: <2026-10-21 Wed 10:00> DEADLINE: <2026-10-23 Fri>
   :PROPERTIES:
   :Effort:   1:00
   :ID:       0a1b2c3d-0000-4000-8000-000000000001
   :END:
   :LOGBOOK:
   CLOCK: [2026-10-20 Tue 09:00]--[2026-10-20 Tue 10:30] =>  1:30
   :END:
   Draft the report for [[file:notes.org::*Meeting notes][the meeting]].
** NEXT Call the plumber                                       :home:@phone:
   DEADLINE: <2026-10-19 Mon>
** WAIT Reply from Alice
   - State "WAIT"       from "TODO"       [2026-10-18 Sun 14:00] \\\\
     Waiting for her answer.
** TODO Water the plants
   SCHEDULED: <2026-10-20 Tue .+2d>
   :PROPERTIES:
   :STYLE:    habit
   :LAST_REPEAT: [2026-10-18 Sun 08:00]
   :END:
   :LOGBOOK:
   - State "DONE"       from "TODO"       [2026-10-18 Sun 08:00]
   - State "DONE"       from "TODO"       [2026-10-16 Fri 08:00]
   - State "DONE"       from "TODO"       [2026-10-14 Wed 08:00]
   :END:
** TODO Weekly review
   SCHEDULED: <2026-10-23 Fri +1w>
** DONE Pay the rent
   CLOSED: [2026-10-01 Thu 09:12]
   SCHEDULED: <2026-10-01 Thu>
** TODO Project Alpha [1/3]                                        :work:
   :PROPERTIES:
   :ORDERED:  t
   :END:
*** DONE Plan it
*** TODO Build it
*** TODO Ship it
** TODO Groceries [33%]
   - [X] milk
   - [ ] bread
   - [ ] 卵 (eggs) 🥚
** Meeting with Bob
   <2026-10-22 Thu 14:00-15:00>
** Conference
   <2026-10-26 Mon>--<2026-10-28 Wed>
** CANCELLED Old idea
   CLOSED: [2026-09-01 Tue 10:00]
*************** TODO An inline task
*************** END
* Notes
** Ideas about 日本語 and émojis 🎉                                  :urgent:
   Some text with *bold*, /italic/, =verbatim=, ~code~, +strike+ and _under_.
   A footnote[fn:1] and an inline one[fn:: inline note].
   A citation [cite:@knuth1984] and another [cite/t:@lamport1994; @knuth1984].
   An unknown key [cite:@knuht1984].
   Links: [[https://orgmode.org][Org]], [[id:0a1b2c3d-0000-4000-8000-000000000002][Alpha node]],
   [[#custom][custom id]], [[*Tasks][Tasks heading]], [[target]], <<target>>,
   [[file:tables.org][tables]], [[roam:Beta]], [[elisp:(+ 1 2)]], https://example.com
** Custom
   :PROPERTIES:
   :CUSTOM_ID: custom
   :END:
   A paragraph.
   1. first
   2. second
      - nested a
      - nested b
   3. third
** Code
   #+NAME: answer
   #+begin_src python :results output
   print(40 + 2)
   #+end_src

   #+begin_src emacs-lisp
   (+ 1 2)
   #+end_src

   #+begin_src sh :tangle out/hello.sh
   echo hello
   #+end_src

   #+begin_quote
   A quote.
   #+end_quote
* Archive                                                          :ARCHIVE:
** DONE Archived thing
   CLOSED: [2026-08-01 Sat 10:00]

[fn:1] The footnote's text.
"""

NOTES = """\
#+TITLE: Notes
* Inbox
** TODO Read the manual
* Meeting notes
  Bob said hello.
** Follow up                                                          :work:
* Projects
** Alpha
** Beta
* Clock report
#+BEGIN: clocktable :scope agenda :maxlevel 2
#+END:
"""

TABLES = """\
#+TITLE: Tables
* Budget
| Item  | Qty | Price | Total |
|-------+-----+-------+-------|
| Apple |   3 |  0.50 |       |
| Pear  |   2 |  0.75 |       |
| Plum  |  10 |  0.20 |       |
|-------+-----+-------+-------|
| Sum   |     |       |       |
#+TBLFM: $4=$2*$3::@>$4=vsum(@I..@II)

* Unaligned
|a|bb|ccc|
|-
|1|22|333|
|日本|x|🎉|

* Sort me
- zebra
- apple
- mango

* Headlines to sort
** Charlie
** alpha
** Bravo
"""

BIB = """\
@book{knuth1984,
  author = {Donald E. Knuth},
  title = {The {\\TeX}book},
  publisher = {Addison-Wesley},
  year = 1984
}

@book{lamport1994,
  author = {Leslie Lamport},
  title = {{\\LaTeX}: A Document Preparation System},
  publisher = {Addison-Wesley},
  year = 1994
}
"""

ARCHIVE = """\
#    -*- mode: org -*-


Archived entries from file gtd.org


* DONE Old archived task
  CLOSED: [2026-07-01 Wed 10:00]
  :PROPERTIES:
  :ARCHIVE_TIME: 2026-07-02 Thu 10:00
  :ARCHIVE_FILE: gtd.org
  :ARCHIVE_OLPATH: Tasks
  :ARCHIVE_CATEGORY: gtd
  :ARCHIVE_TODO: DONE
  :END:
"""

ROAM_ALPHA = """\
:PROPERTIES:
:ID:       0a1b2c3d-0000-4000-8000-000000000002
:ROAM_ALIASES: "A node"
:END:
#+title: Alpha
#+filetags: :project:

Alpha links to [[id:0a1b2c3d-0000-4000-8000-000000000003][Beta]].
* A heading node
  :PROPERTIES:
  :ID:       0a1b2c3d-0000-4000-8000-000000000004
  :END:
"""

ROAM_BETA = """\
:PROPERTIES:
:ID:       0a1b2c3d-0000-4000-8000-000000000003
:ROAM_REFS: https://example.com/beta
:END:
#+title: Beta

Beta, back to [[id:0a1b2c3d-0000-4000-8000-000000000002][Alpha]].
"""

DAILY = """\
:PROPERTIES:
:ID:       0a1b2c3d-0000-4000-8000-000000000005
:END:
#+title: 2026-10-20

* Yesterday's note
"""

JOURNAL = """\
* Tuesday, 10/20/26
** 09:00 Morning entry
   Something happened.
** TODO Carry this over
"""


def make(directory, clean=True):
    if clean and os.path.exists(directory):
        shutil.rmtree(directory)
    os.makedirs(directory, exist_ok=True)
    files = {
        "gtd.org": GTD,
        "notes.org": NOTES,
        "tables.org": TABLES,
        "refs.bib": BIB,
        "gtd.org_archive": ARCHIVE,
        "roam/alpha.org": ROAM_ALPHA,
        "roam/beta.org": ROAM_BETA,
        "roam/daily/2026-10-20.org": DAILY,
        "journal/20261020": JOURNAL,
        ".git/HEAD": "ref: refs/heads/main\n",
    }
    for name, text in files.items():
        p = os.path.join(directory, name)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            f.write(text)
    options = {
        "org_agenda_files": [os.path.join(directory, "gtd.org"), os.path.join(directory, "notes.org")],
        "org_directory": directory,
        "org_default_notes_file": os.path.join(directory, "notes.org"),
        "org_todo_keywords": ["TODO", "NEXT", "WAIT(w@)", "|", "DONE(d!)", "CANCELLED(c@)"],
        "org_log_done": "time",
        "org_log_into_drawer": True,
        "org_modules": ["org-habit"],
        "org_cite_global_bibliography": [os.path.join(directory, "refs.bib")],
        "org_refile_targets": [{"files": "agenda", "maxlevel": 2}],
        "org_capture_templates": [
            {"key": "t", "description": "Task", "target": ["file+headline", "notes.org", "Inbox"],
             "template": "* TODO %?\n  %u"},
            {"key": "n", "description": "Note", "target": ["file", "notes.org"], "template": "* %? :note:\n%U"},
            {"key": "j", "description": "Journal", "target": ["file+olp+datetree", "notes.org"],
             "template": "* %?\nEntered on %U"},
        ],
        "org_roam_enabled": True,
        "org_roam_directory": os.path.join(directory, "roam"),
        "org_journal_enabled": True,
        "org_journal_dir": os.path.join(directory, "journal"),
        "auto_save": False,
    }
    return directory, options


if __name__ == "__main__":
    import sys

    print(make(sys.argv[1]))
