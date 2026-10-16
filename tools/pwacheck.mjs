// Is the site installable as an app? Loads it in headless Chrome, waits for its service
// worker, and asks Chrome what it makes of the page: the manifest it found, its icons,
// and the reasons it couldn't be installed (none: it can). The built site by default,
// or another address (the deployed one):
//     node tools/pwacheck.mjs [url=https://battle-frontier.pages.dev/]
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const opt = Object.fromEntries(process.argv.slice(2).filter((a) => a.includes('=')).map((a) => a.split(/=(.*)/s).slice(0, 2)));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

const server = opt.url ? null : await serve(0);
const url = opt.url ?? `http://localhost:${server.address().port}/index.html`;
const browser = await Browser.launch({ width: 1200, height: 800, port: 9499, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
try {
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 120000, what: 'the menu' });
  await browser.waitFor('document.body.dataset.serviceworker === "ready"', { timeout: 30000, what: 'the service worker' });
  check('the service worker runs', true);
  const { url: manifestUrl, errors, data } = await browser.send('Page.getAppManifest');
  const manifest = data ? JSON.parse(data) : {};
  check('a manifest', !!manifestUrl && (errors ?? []).length === 0, `${manifestUrl} ${JSON.stringify(errors ?? [])}`);
  check('  named Battle Frontier, standalone', manifest.name === 'Battle Frontier' && manifest.display === 'standalone', `${manifest.name} · ${manifest.display}`);
  const sizes = (manifest.icons ?? []).map((i) => `${i.sizes}${i.purpose ? ' ' + i.purpose : ''}`);
  check('  with its icons', sizes.includes('192x192') && sizes.includes('512x512') && sizes.includes('512x512 maskable'), sizes.join(', '));
  for (const icon of manifest.icons ?? []) {
    const status = await browser.evaluate(`fetch(${JSON.stringify(new URL(icon.src, manifestUrl).href)}).then((r) => r.status + ' ' + r.headers.get('content-type'))`);
    check(`  ${icon.src} is served`, status.startsWith('200 image/png'), status);
  }
  const installability = await browser.send('Page.getInstallabilityErrors').catch((e) => ({ error: String(e) }));
  const reasons = (installability.installabilityErrors ?? []).map((e) => e.errorId + (e.errorArguments?.length ? ` (${e.errorArguments.map((a) => `${a.name}=${a.value}`).join(', ')})` : ''));
  check('Chrome can install it', !installability.error && reasons.length === 0, installability.error ?? (reasons.join('; ') || 'nothing in the way'));
  if (browser.errors.length) console.log('page errors (not counted):\n' + browser.errors.join('\n'));
} finally {
  await browser.close();
  server?.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
