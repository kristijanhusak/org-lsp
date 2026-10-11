// Draw pages of terminal frames as PNG images: `node render.cjs a.html …`
// writes a.png next to each. Playwright may be a global package
// (`NODE_PATH`).
const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ deviceScaleFactor: 2 });
  for (const file of process.argv.slice(2)) {
    await page.goto('file://' + file);
    await page.locator('#window').screenshot({ path: file.replace(/\.html$/, '.png'), omitBackground: true });
  }
  await browser.close();
})();
