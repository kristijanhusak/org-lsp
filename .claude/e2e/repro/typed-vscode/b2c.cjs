// The icons of a TODO menu: with a face of the keyword's, and without.
module.exports = [
  {
    name: 'todo-menu-face',
    settings: { org_todo_keywords: ['TODO(t)', 'WAIT(w@)', '|', 'DONE(d)'], org_todo_keyword_faces: { WAIT: ':foreground orange :weight bold' } },
    file: 'tp.org',
    content: '* TODO Task\n',
    steps: [
      {
        at: [0, 2],
        run: async (ctx) => {
          await ctx.xdotool('key', 'ctrl+c', 'ctrl+t');
          await ctx.sleep(1500);
          ctx.shot('todo-menu-face');
          const dir = ctx.path.join(process.env.USER_DATA, 'User', 'globalStorage');
          const found = [];
          const walk = (d) => { for (const n of ctx.fs.readdirSync(d, { withFileTypes: true })) { const p = ctx.path.join(d, n.name); if (n.isDirectory()) walk(p); else found.push(p); } };
          try { walk(dir); } catch (e) { found.push(String(e)); }
          return found;
        },
        answers: ['t'],
      },
    ],
  },
];
