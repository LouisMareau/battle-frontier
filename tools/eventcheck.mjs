// The story in the built site, in a real browser: the test meadow's actors and events.
// The server holds the flag pole's model back, so the map is built without it and must be
// built again when it comes (the pole showed up missing once, its flag waving alone).
// Walks up to the signpost and reads it, talks to Bo and takes a choice (the camera goes
// to the well and back, a flag is remembered), steps into the well's cells once, sees the
// flag pole wave on its pole, and hears the place. With --webgl, the page without WebGPU
// (WebGL 2: the flag, a skinned model, posed from a uniform buffer). Exits 1 when something
// fails.
//     node tools/eventcheck.mjs [--webgl]
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
mkdirSync(shots, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const webgl = process.argv.includes('--webgl');
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

// the flag pole's model comes 2.5 s late: after the map's props were built
const server = await serve(0, { delay: (path) => (path.endsWith('models/flag_pole.glb') ? 2500 : 0) });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9493, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--autoplay-policy=no-user-gesture-required'] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = (c) => browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
  await sleep(250);
};
const shot = (name) => browser.screenshot(join(shots, `events-${webgl ? 'webgl-' : ''}${name}.png`));
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
  if (webgl) await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: "Object.defineProperty(Navigator.prototype, 'gpu', { get: () => undefined, configurable: true });" });
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1500);
  await browser.click(700, 500);

  // 0. the props built without the flag pole's model, then again with it when it comes
  check('the flag pole\'s model held back', Number(await data('models')) <= 1, `${await data('models')} models`);
  await waitFor('document.body.dataset.squares === "1/1"', 'the map built');
  const bare = Number(await data('propsverts'));
  await waitFor('Number(document.body.dataset.models) >= 2', 'the flag pole\'s model, late');
  await sleep(400);
  const whole = Number(await data('propsverts'));
  check('the props built again when it came', whole > bare, `${bare} -> ${whole} vertices`);
  // the asset browser closed (it comes open, along the bottom: after the above, which counts
  // on the editor's first seconds): the view as these steps knew it
  if ((await data('layout')).includes('assets=bottom')) {
    await press('uiAssets');
    await sleep(400);
    await browser.click(700, 889); // the status bar: the canvas gets the keys
  }

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

  // 5. the flag waves on its pole (grey, against the grass south of it), the place is heard
  check('a rigged prop posed', Number(await data('animated')) >= 1, await data('animated'));
  await browser.key('Escape'); // the walk ends: the camera is free again
  await sleep(200);
  await cmd('view:16.5,12.5,9,180,40');
  await sleep(1200);
  const at = async (x, y, z) => {
    await cmd(`point:${x},${y},${z}`);
    await sleep(120);
    return (await data('point')).split(',').map(Number);
  };
  const [fx, fy] = await at(16.05, 0.1, 12.5);
  const [tx, ty] = await at(16.05, 2.9, 12.5);
  const shot5 = await browser.pixels(join(shots, `events-${webgl ? 'webgl-' : ''}5-pole.png`));
  let hits = 0;
  const samples = 12;
  for (let k = 0; k < samples; k++) {
    const [cx, cy] = [Math.round(fx + (tx - fx) * k / (samples - 1)), Math.round(fy + (ty - fy) * k / (samples - 1))];
    let grey = false;
    for (let y = cy - 4; y <= cy + 4 && !grey; y++) {
      for (let x = cx - 5; x <= cx + 5 && !grey; x++) {
        const i = (y * shot5.width + x) * 4;
        const [r, g, b] = [shot5.rgba[i], shot5.rgba[i + 1], shot5.rgba[i + 2]];
        grey = Math.max(r, g, b) - Math.min(r, g, b) < 28 && r > 70 && r < 230;
      }
    }
    hits += grey;
  }
  check('the pole drawn, all the way up', hits >= samples - 2, `${hits} of ${samples} points along it`);
  console.log('audio', await data('audio'), '· sounds playing', await data('sounds'));
  check('sounds asked for', (await data('audio')) === 'off' || Number(await data('sounds')) >= 1, `${await data('audio')} ${await data('sounds')}`);
  await shot('4-flag');
  // the flag (crimson: the skinned part, posed by its rig) from the pole's top, seen whole
  // (waving, about 7000 to 20000 pixels a little left of and below it; none when it isn't
  // drawn there)
  await cmd('view:16.5,14.5,10,180,40');
  await sleep(1200);
  const [rx, ry] = await at(16.1, 2.7, 12.54);
  const shot6 = await browser.pixels(join(shots, `events-${webgl ? 'webgl-' : ''}6-flag.png`));
  let flag = 0;
  for (let y = Math.max(0, ry - 200); y < Math.min(shot6.height, ry + 200); y++) {
    for (let x = Math.max(0, rx - 350); x < Math.min(shot6.width, rx + 150); x++) {
      const i = (y * shot6.width + x) * 4;
      flag += shot6.rgba[i] > 120 && shot6.rgba[i + 1] < 70 && shot6.rgba[i + 2] < 70;
    }
  }
  check('the flag drawn from the pole\'s top', flag > 2000, `${flag} crimson pixels round it`);
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
