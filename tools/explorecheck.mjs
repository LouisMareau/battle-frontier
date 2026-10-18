// The overworld in the built site, in a real browser: Explore walks the test meadow from
// the menu, reads the signpost, talks to Bo and takes a choice, leaves by the HUD's Exit (Esc
// doesn't leave), and after the page
// is loaded again stands where it left off with the flags kept; "forget" starts over.
// Exits 1 when something fails.
//     node tools/explorecheck.mjs
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
mkdirSync(shots, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9495, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--autoplay-policy=no-user-gesture-required'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = (c) => browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
const shot = (name) => browser.screenshot(join(shots, `explore-${name}.png`));
const waitFor = (expr, what) => browser.waitFor(expr, { timeout: 15000, what });
const dialogUp = () => waitFor('document.body.dataset.dialog !== ""', 'a dialog');
// the HUD's Exit, clicked: back on the menu
const exit = async () => {
  const [x, y] = (await data('uiHudExit')).split(',').map(Number);
  await browser.click(x, y);
};
const dialogDown = () => waitFor('document.body.dataset.dialog === ""', 'the dialog closing');
const a = async () => {
  await browser.key('z');
  await sleep(150);
};
const menu = async () => {
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
};
const explore = async (map) => {
  await cmd(map ? `explore:${map}` : 'explore');
  await browser.waitFor('document.body.dataset.state === "explore"', { what: 'the overworld' });
  await sleep(1500);
  await browser.click(700, 450);
};

try {
  await browser.viewport(1400, 900);
  await menu();
  await cmd('forget');
  await sleep(100);

  // 1. from the menu into the meadow, standing somewhere free
  await explore('test_meadow');
  check('the overworld on the test meadow', (await data('map')) === 'test_meadow' && (await data('walker')) !== '', `${await data('map')} at ${await data('walker')}`);
  await shot('1-meadow');

  // 2. the story plays here too
  await cmd('walkto:21,24');
  await sleep(300);
  await a();
  await dialogUp();
  check('the signpost speaks', (await data('dialog')).startsWith('Meadow path'), await data('dialog'));
  await a();
  await a();
  await dialogDown();
  await cmd('walkto:22,8');
  await sleep(300);
  await a();
  await dialogUp();
  check('Bo speaks', (await data('dialogwho')) === 'Bo');
  await a();
  await a();
  await waitFor('document.body.dataset.choices !== ""', 'the choice');
  await shot('2-bo');
  await a(); // Yes
  await dialogUp();
  await a();
  await a();
  await waitFor('document.body.dataset.running === ""', 'the script ending');
  check('remembered', (await data('flags')).includes('met_bo=1'), await data('flags'));

  // 2b. into the cottage: the outside darkens as the roof fades away, and back out
  await cmd('walkto:10,13');
  await sleep(300);
  await cmd('walkto:10,10');
  await sleep(180);
  const early = Number(await data('dim'));
  await shot('3-entering');
  await sleep(600);
  check('inside the cottage', (await data('inside')) === 'cottage', await data('inside'));
  check('the outside darkens, over time', early > 0 && early < 0.7 && Number(await data('dim')) >= 0.7, `${early} then ${await data('dim')}`);
  await shot('4-inside');
  await cmd('walkto:10,13');
  await sleep(180);
  const leaving = Number(await data('dim'));
  await sleep(600);
  check('out again, the light back', (await data('inside')) === '' && leaving > 0 && Number(await data('dim')) === 0, `${leaving} then ${await data('dim')}`);
  await cmd('walkto:22,8');
  await sleep(300);

  // 3. Esc doesn't leave; out with Exit, back in: the same place, the same memory
  await browser.key('Escape');
  await new Promise((r) => setTimeout(r, 300));
  check("Esc doesn't leave the world", (await data('state')) === 'explore', await data('state'));
  await browser.key('Escape');
  await exit();
  await browser.waitFor('document.body.dataset.state === "menu"', { what: 'the menu again' });
  check('Exit leaves for the menu', true);
  await menu();
  await explore();
  check('loaded again: the saved map', (await data('map')) === 'test_meadow', await data('map'));
  check('  standing where it left off', (await data('walker')) === '22,8', await data('walker'));
  check('  with the flags kept', (await data('flags')).includes('met_bo=1'), await data('flags'));
  // Bo's talk goes the short way now that they've met
  await a();
  await dialogUp();
  await a();
  await a();
  await waitFor('document.body.dataset.choices !== ""', 'the choice again');
  await browser.key('ArrowDown');
  await a(); // Not really
  await dialogUp();
  check('the other branch', (await data('dialog')).includes('know the way'), await data('dialog'));
  await a();
  await a();
  await waitFor('document.body.dataset.running === ""', 'the script ending');

  // 4. forgetting starts over
  await exit();
  await browser.waitFor('document.body.dataset.state === "menu"', { what: 'the menu' });
  await cmd('forget');
  await sleep(100);
  await explore('test_meadow');
  check('forgotten: no flags', (await data('flags')) === '', await data('flags'));
  check('  and a fresh start', (await data('walker')) !== '22,8', await data('walker'));
  console.log('audio', await data('audio'), '· sounds', await data('sounds'), '· animated', await data('animated'));
  if (browser.errors.length) {
    failed++;
    console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  }
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
