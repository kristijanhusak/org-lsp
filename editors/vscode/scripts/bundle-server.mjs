// Copy a built server binary into server/, so that `vsce package` ships it.
//
//   npm run bundle-server                # ../../target/release/org-lsp[.exe]
//   npm run bundle-server -- path/to/org-lsp
import { chmodSync, copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

const exe = process.platform === 'win32' ? 'org-lsp.exe' : 'org-lsp';
const source = resolve(process.argv[2] ?? join('..', '..', 'target', 'release', exe));
if (!existsSync(source)) {
  console.error(`No server binary at ${source}. Build it with: cargo build --release -p org-lsp`);
  process.exit(1);
}
mkdirSync('server', { recursive: true });
const target = join('server', basename(source));
copyFileSync(source, target);
chmodSync(target, 0o755);
console.log(`bundled ${source} -> ${target}`);
