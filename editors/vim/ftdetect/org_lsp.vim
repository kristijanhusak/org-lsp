autocmd BufNewFile,BufRead *.org,*.org_archive setfiletype org
" The agenda, as a file the server writes for clients that cannot fetch it.
autocmd BufNewFile,BufRead *.org-agenda setfiletype org-agenda
