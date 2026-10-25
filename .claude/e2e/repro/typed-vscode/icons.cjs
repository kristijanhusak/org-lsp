// Which icons a quick pick shows: an SVG file in the global storage, in
// /tmp, a data: URI, light/dark pairs, a theme icon with a color.
module.exports = [
  {
    name: 'icons',
    file: 'i.org',
    content: '* A\n',
    steps: [
      {
        run: async (ctx) => {
          const { vscode, fs, path } = ctx;
          const svg = (c) => `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="5" fill="${c}"/></svg>`;
          const tmp = fs.mkdtempSync('/tmp/icons-');
          fs.writeFileSync(path.join(tmp, 'a.svg'), svg('orange'));
          const storage = path.join(process.env.USER_DATA, 'User', 'globalStorage', 'kristijanhusak.org-lsp', 'icons');
          const data = vscode.Uri.parse(`data:image/svg+xml;base64,${Buffer.from(svg('#00ff00')).toString('base64')}`);
          const items = [
            { label: 'tmp file', iconPath: vscode.Uri.file(path.join(tmp, 'a.svg')) },
            { label: 'storage file', iconPath: vscode.Uri.file(path.join(storage, 'dot-orange.svg')) },
            { label: 'data uri', iconPath: data },
            { label: 'light/dark tmp', iconPath: { light: vscode.Uri.file(path.join(tmp, 'a.svg')), dark: vscode.Uri.file(path.join(tmp, 'a.svg')) } },
            { label: 'theme icon red', iconPath: new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor('charts.red')) },
          ];
          fs.mkdirSync(storage, { recursive: true });
          fs.writeFileSync(path.join(storage, 'dot-orange.svg'), svg('orange'));
          const pick = vscode.window.createQuickPick();
          pick.items = items;
          pick.show();
          await ctx.sleep(1500);
          ctx.shot('icons');
          pick.hide();
          return [data.toString(), data.toString(true)];
        },
      },
    ],
  },
];
