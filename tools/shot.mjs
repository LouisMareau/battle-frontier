// One screenshot of the built site in a state given as commands (see the README):
//     node tools/shot.mjs editor [wait=2500] [out=screenshots/editor.png]
//     node tools/shot.mjs "auto:1;start:ENCOUNTER" [wait=8000]
import { mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const [cmd = 'editor', ...rest] = process.argv.slice(2);
const opt = Object.fromEntries(rest.filter((a) => a.includes('=')).map((a) => a.split('=')));
const out = resolve(opt.out ?? join(root, 'screenshots', `${cmd.replace(/[^a-z0-9]+/gi, '-')}.png`));
mkdirSync(dirname(out), { recursive: true });

const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9479, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
try {
  await browser.viewport(1400, 900);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(cmd)}, true)`);
  await browser.wait(Number(opt.wait ?? 2500));
  await browser.screenshot(out);
  console.log('wrote', out, '· state', await browser.evaluate('document.body.dataset.state'));
  if (opt.hook) console.log(opt.hook + ':', await browser.evaluate(`document.body.dataset.${opt.hook} ?? ''`));
  if (browser.errors.length) console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
} finally {
  await browser.close();
  server.close();
}
