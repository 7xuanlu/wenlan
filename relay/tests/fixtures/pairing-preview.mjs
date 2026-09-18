// SPDX-License-Identifier: Apache-2.0
// Loopback-only preview with synthetic consent; never a deployable entrypoint.
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const require = createRequire(new URL('../../package.json', import.meta.url));
const { Miniflare } = require('miniflare');
const { build } = require('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('./pairing-preview-worker.ts', import.meta.url))],
  bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  external: ['cloudflare:workers'], loader: { '.png': 'binary' } });
const runtime = new Miniflare({ modules: true, script: bundle.outputFiles[0].text,
  compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0 });
console.log(`PAIRING_PREVIEW ${await runtime.ready}pairing`);
const timer = setTimeout(() => runtime.dispose(), 15 * 60 * 1000);
process.on('SIGINT', async () => { clearTimeout(timer); await runtime.dispose(); });
