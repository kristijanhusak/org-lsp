# Notes of the rest fixer (worktree agent-a92ff95f01afdaa8f, branch worktree-agent-a92ff95f01afdaa8f, based on claude/elegant-meitner-ggxbid 91900ef)

- vscode-1: FIXED d333924. Resolution before a clock-in holds its change (`resolved_changes`), the clock-in sends it with its own in one edit (headline offset shifted). Test agenda_lsp `clocks_in_after_resolving_in_one_edit`. (The agenda_lsp.rs HOME change of fix-1 landed in this commit too.)
- fix-1: FIXED e898b46 (+ agenda_lsp part in d333924). `private_state()` (lsp, agenda_lsp, e2e_scenarios, highlight tests) sets HOME to a per-process temp dir and removes XDG_CONFIG_HOME.
