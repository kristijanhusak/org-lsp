// Bundle the extension into dist/extension.js.
import { build } from 'esbuild';

await build({
  entryPoints: ['src/extension.ts'],
  outfile: 'dist/extension.js',
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['vscode'],
  sourcemap: false,
  minify: process.argv.includes('--minify'),
  logLevel: 'info',
});
