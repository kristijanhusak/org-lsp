#!/usr/bin/env bash
# The vim.pack install of docs/neovim.org in a clean XDG. pack.sh <nvim> <mode: cmd|path|download> [update]
NV=$1 MODE=$2 UPDATE=${3:+1}
S=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/neovim/pack-$MODE-$RANDOM
R=/home/user/org-lsp/.claude/e2e/repro/neovim
mkdir -p $S/agenda $S/rel/nightly $S/stage
git clone -q /home/user/org-lsp $S/copy
printf '* TODO A\nSCHEDULED: <2026-10-21 Wed>\n' > $S/agenda/a.org
cp /home/user/org-lsp/target/debug/org-lsp $S/stage/org-lsp
tar -czf $S/rel/nightly/org-lsp-linux-x86_64.tar.gz -C $S/stage org-lsp
P=$(echo "$PATH" | tr ':' '\n' | while read d; do [ -x "$d/org-lsp" ] || echo "$d"; done | paste -sd:)
if [ "$MODE" = path ]; then mkdir -p $S/bin; ln -s /home/user/org-lsp/target/debug/org-lsp $S/bin/org-lsp; P=$S/bin:$P; fi
env -u ORG_LSP_CONFIG PATH=$P XDG_CONFIG_HOME=$S/xdg/config XDG_DATA_HOME=$S/xdg/data XDG_STATE_HOME=$S/xdg/state XDG_CACHE_HOME=$S/xdg/cache \
  MODE=$MODE UPDATE=$UPDATE COPY=$S/copy AGENDA=$S/agenda RELEASES=$S/rel \
  timeout 600 $NV --headless -u $R/pack-init.lua -c "luafile $R/pack-test.lua" 2>&1
echo "exit $?"
