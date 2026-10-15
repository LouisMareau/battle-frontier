// The Build tool, driven with the mouse and keys in a headless Chrome: draws an L-shaped
// footprint, puts a double door and a window on the ground floor, adds a floor with a bed
// and a pillar, confirms, decorates a wall, walks in through the door (and into walls),
// turns and moves the building, undoes, and downloads the map. Screenshots go to
// screenshots/build-*.png.
//     node tools/buildcheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9483, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(200);
};
let failed = 0;
const check = (what, ok, detail = '') => {
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
};
const expect = async (what, key, want) => {
  const got = await data(key);
  check(what, String(got) === String(want), `${key} = ${got}${String(got) === String(want) ? '' : ` (wanted ${want})`}`);
};
// where a world point is on the screen
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(100);
  return (await data('point')).split(',').map(Number);
};
// a widget the editor marked
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
};
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `build-${name}.png`));
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  await cmd('view:22.5,18.5,15,0,58');
  await browser.click(700, 889); // the status bar: the canvas gets the keyboard
  const buildings0 = Number(await data('buildings'));

  // 1. the footprint: an L
  await browser.key('5');
  await expect('the Build tool', 'tool', 'build');
  await browser.drag([await at(21.5, 0, 17.5), await at(22.5, 0, 17.5), await at(23.5, 0, 17.5)]);
  await browser.drag([await at(21.5, 0, 18.5), await at(22.5, 0, 18.5), await at(23.5, 0, 18.5)]);
  const foot = await at(21.5, 0, 19.5);
  await browser.click(...foot);
  await expect('drawing the footprint', 'build', 'footprint');
  await shot('1-footprint');
  await press('uiNext');
  await expect('designing it', 'build', 'design');

  // 2. a double door on the south wall, a window on the west
  await browser.drag([await at(22.5, 0, 19.0), await at(23.0, 0, 19.0), await at(23.5, 0, 19.0)]);
  await press('uiPut');
  await expect('a double door', 'draft', 'floors:1;floor:1;openings:1;inside:0;pillars:0');
  await press('uiWindows');
  await browser.click(...(await at(21.0, 0, 19.5)));
  await press('uiPut');
  await expect('a window', 'draft', 'floors:1;floor:1;openings:2;inside:0;pillars:0');
  await shot('2-ground-floor');

  // 3. a second floor: a bed inside, a pillar on a corner
  await press('uiAddFloor');
  await press('uiInside');
  await browser.click(...(await at(22.5, 2.0, 17.6)));
  await press('uiPillars');
  await browser.click(...(await at(21.0, 2.0, 17.0)));
  await expect('the second floor', 'draft', 'floors:2;floor:2;inside:1;pillars:1'.replace('floor:2;', 'floor:2;openings:0;'));
  await shot('3-second-floor');

  // 4. confirmed: on the map
  await press('uiConfirm');
  await expect('confirmed', 'build', '');
  await expect('one more building', 'buildings', buildings0 + 1);
  await cmd('view:22.5,18.5,15,-35,40');
  await shot('4-confirmed');
  await cmd('view:22.5,18.5,15,0,58');

  // 5. selected, and a decoration on a wall
  await browser.key('1');
  await browser.click(...(await at(22.5, 1.0, 18.0)));
  await expect('selected', 'selection', `building:${buildings0}`);
  await press('uiDecorate');
  await browser.click(...(await at(21.5, 1.4, 20.0)));
  await expect('a decoration', 'anchors', 1);
  await press('uiDecorate');

  // 6. on foot: in through the door, not through walls
  await press('uiWalk');
  const start = await data('walker');
  check('walking', start !== '', `from ${start}`);
  const step = async (key, want, what) => {
    await browser.key(key);
    await sleep(320);
    await expect(what, 'walker', want);
  };
  await step('ArrowDown', '22,19', 'out through the door');
  await step('ArrowLeft', '22,19', 'the wall stops it');
  await step('ArrowUp', '22,18', 'back in');
  await step('ArrowUp', '22,17', 'inside');
  await shot('5-inside');
  await step('ArrowUp', '22,17', 'the north wall stops it');
  await browser.key('Escape');

  // 7. turned, moved, and the move undone
  const cells = await data('bcells');
  await browser.key('e');
  check('turned', (await data('bcells')) !== cells, `${cells} -> ${await data('bcells')}`);
  const before = await data('bat');
  await browser.drag([await at(22.5, 1.0, 18.0), await at(23.0, 0, 18.0), await at(24.0, 0, 18.0), await at(24.5, 0, 18.0)]);
  const moved = await data('bat');
  check('moved', moved !== before, `${before} -> ${moved}`);
  await shot('6-moved');
  await browser.key('z', { ctrl: true });
  await expect('the move undone', 'bat', before);

  // 8. the download has it
  await press('uiWalk'); // walking again then stopping: no effect on the map
  await browser.key('Escape');
  const [dx, dy] = [525, 17];
  await browser.click(dx, dy);
  await sleep(1500);
  const saved = JSON.parse(readFileSync(join(downloads, 'test_meadow.json'), 'utf8'));
  const b = saved.buildings?.[buildings0];
  check('downloaded', !!b && b.floors.length === 2 && b.floors[0].openings.length === 2 && b.pillars.length === 1 && b.anchors.length === 1,
    b ? `${b.id}: ${b.floors.length} floors, ${b.floors[0].openings.length} openings, ${b.pillars.length} pillar, ${b.anchors.length} decoration` : 'no building');
  console.log('fps', await data('fps'), 'worst frame', await data('maxgap'), 'ms');
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
