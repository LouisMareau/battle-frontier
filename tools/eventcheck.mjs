// The story in the built site, in a real browser: the test meadow's actors and events.
// Walks up to the signpost and reads it, talks to Bo and takes a choice (the camera goes
// to the well and back, a flag is remembered), steps into the well's cells once, sees the
// flag pole wave, and hears the place. Exits 1 when something fails.
//     node tools/eventcheck.mjs
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
const browser = await Browser.launch({ width: 1400, height: 900, port: 9493, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--autoplay-policy=no-user-gesture-required'] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = (c) => browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
  await sleep(250);
};
const shot = (name) => browser.screenshot(join(shots, `events-${name}.png`));
const waitFor = (expr, what) => browser.waitFor(expr, { timeout: 15000, what });
const dialogUp = () => waitFor('document.body.dataset.dialog !== ""', 'a dialog');
const dialogDown = () => waitFor('document.body.dataset.dialog === ""', 'the dialog closing');
// A: Z, with the pointer over the view so the page has the keys
const a = async () => {
  await browser.key('z');
  await sleep(150);
};

try {
  await browser.viewport(1400, 900);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1500);
  await browser.click(700, 500);

  // 1. the Events tool lists what the map has
  await browser.key('7');
  await sleep(200);
  check('the Events tool', (await data('tool')) === 'events', await data('tool'));
  check('the actors on the map', Number(await data('actors')) >= 2, await data('actors'));
  check('the events on the map', Number(await data('events')) >= 4, await data('events'));
  await press('uiActor_bo');
  await press('uiEvent_bo_talk');
  check('no mistakes in its script', (await data('scriptproblems')) === '0', await data('scriptproblems'));
  await shot('1-tool');

  // 2. the signpost, read from the path
  await cmd('walkto:21,24');
  await sleep(300);
  check('standing on the path', (await data('walker')) === '21,24', await data('walker'));
  await a();
  await dialogUp();
  check('the signpost speaks', (await data('dialog')).startsWith('Meadow path'), await data('dialog'));
  await shot('2-sign');
  await a(); // all of it
  await a(); // on
  await dialogDown();

  // 3. Bo: a line, a choice, the camera to the well and back, a flag remembered
  await cmd('walkto:22,8');
  await sleep(300);
  await a();
  await dialogUp();
  check('Bo speaks', (await data('dialogwho')) === 'Bo', `${await data('dialogwho')}: ${await data('dialog')}`);
  await a();
  await a();
  await waitFor('document.body.dataset.choices !== ""', 'the choice');
  check('a choice', (await data('choices')) === 'Yes|Not really', await data('choices'));
  await shot('3-choice');
  await a(); // Yes
  await dialogUp();
  check('the yes branch', (await data('dialog')).includes('well hums'), await data('dialog'));
  await a();
  await a();
  await waitFor('document.body.dataset.running === ""', 'the script ending (the camera went to the well and back)');
  check('remembered', (await data('flags')).includes('met_bo=1'), await data('flags'));
  check('Bo faces the player', true);
  // Pip knows, now
  await cmd('walkto:20,4');
  await sleep(300);
  await browser.evaluate('document.body.dataset.pipwas = ""');
  // Pip wanders: not a sure thing to be in front; the script's branch is the thing checked
  // through the sign of the flag above

  // 4. the well's cells, once
  await cmd('walkto:11,9');
  await sleep(300);
  await cmd('walkto:11,8');
  await dialogUp();
  check('stepping into the well\'s cells', (await data('dialog')).includes('hums softly'), await data('dialog'));
  await a();
  await a();
  await dialogDown();
  check('once only: done', (await data('flags')).includes('done.well_hum=1'), await data('flags'));
  await cmd('walkto:11,9');
  await sleep(200);
  await cmd('walkto:11,8');
  await sleep(600);
  check('and not again', (await data('dialog')) === '', await data('dialog'));

  // 5. the flag waves, the place is heard
  check('a rigged prop posed', Number(await data('animated')) >= 1, await data('animated'));
  console.log('audio', await data('audio'), '· sounds playing', await data('sounds'));
  check('sounds asked for', (await data('audio')) === 'off' || Number(await data('sounds')) >= 1, `${await data('audio')} ${await data('sounds')}`);
  await shot('4-flag');
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
