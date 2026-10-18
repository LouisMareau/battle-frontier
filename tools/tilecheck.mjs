// A building's cells and walls one by one, in the Build tool's Structure mode, driven in a
// headless Chrome: a house of 3 by 4 cells with a door; on its ground floor two cells take
// stone slabs (a drag) and one is laid back as the floor's own; a wall segment takes bricks;
// stairs up to a second floor, where a hole is refused under a table, made beside the
// stairs' top (the stairs' own opening stays as it is), and a table over it is refused; a
// wall segment bricks on every floor at once. Confirmed, then walked: in through the door,
// up the stairs, and the hole stops the walker. The map's download has it all.
// Screenshots go to screenshots/tiles-*.png.
//     node tools/tilecheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-tiles');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9498, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
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
const press = async (name) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(150);
};
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `tiles-${name}.png`));
};

// the house: map cells x 21 to 23, z 16 to 19 (its corner at 21, 16); the ground floor at
// height 0, the second floor at 2 (the level's height)
const UP = 2.0;

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  await cmd('view:22.5,18,15,0,58');
  await browser.click(700, 889); // the status bar: the canvas gets the keyboard
  const buildings0 = Number(await data('buildings'));

  // 1. the footprint, a door on the south wall
  await browser.key('5');
  for (const z of [16.5, 17.5, 18.5, 19.5]) {
    await browser.drag([await at(21.5, 0, z), await at(22.5, 0, z), await at(23.5, 0, z)]);
  }
  await press('uiNext');
  await expect('a house of 3 by 4 cells, designed', 'build', 'design');
  await browser.click(...(await at(22.5, 0, 20.0)));
  await press('uiPut');
  check('a door on the south wall', (await data('draft')).includes('openings:1'), await data('draft'));

  // 2. Flooring: two cells of stone slabs (a drag), one laid back as the floor's own; no
  // holes offered on the ground floor
  await press('uiTiles');
  check('  Flooring: no hole on the ground floor', (await data('uiTileHole')) === '', await data('uiTileHole'));
  await press('uiTileBrush_stone_floor');
  await expect('  the stone brush', 'tilebrush', 'stone_floor');
  await browser.drag([await at(21.5, 0, 16.5), await at(22.0, 0, 16.5), await at(22.5, 0, 16.5)]);
  await expect('two cells of stone slabs', 'buildtiles', 2);
  await press('uiTileOwn');
  await browser.click(...(await at(22.5, 0, 16.5)));
  await expect('one back as the floor', 'buildtiles', 1);
  check('  the cell pointed at', (await data('hovercell')) === '1,0', await data('hovercell'));

  // 3. Walls: one segment of the north wall in bricks
  await press('uiWallStyles');
  await press('uiWallBrush_brick_wall');
  await browser.click(...(await at(21.5, 0, 16.05)));
  await expect('a segment of bricks', 'buildwalls', 1);
  await shot('1-ground-floor');

  // 4. a second floor; stairs up to it from the ground floor's east side (their top under
  // its cell 23, 16)
  await press('uiAddFloor');
  await press('uiFloor1');
  await press('uiInside');
  await press('uiPick_stairs');
  await browser.click(...(await at(23.5, 0, 17.6)));
  check('stairs on the ground floor', (await data('draft')).includes('floor:1') && (await data('draft')).includes('inside:1'), await data('draft'));

  // 5. the second floor: a table, then a hole refused under it, made beside the stairs' top
  await press('uiFloor2');
  await press('uiPick_table');
  await browser.click(...(await at(21.5, UP, 16.5)));
  check('a table on the second floor', (await data('draft')).includes('floor:2') && (await data('draft')).includes('inside:1'), await data('draft'));
  await press('uiStructure');
  await press('uiTiles');
  await press('uiTileHole');
  await expect('  the hole brush (above the ground floor)', 'tilebrush', '#hole');
  await browser.click(...(await at(21.5, UP, 16.5)));
  await expect('no hole under the table', 'buildholes', 0);
  check('  and why', (await data('status')).includes('stands there'), await data('status'));
  await browser.click(...(await at(22.5, UP, 16.5)));
  await expect('a hole beside the stairs\' top', 'buildholes', 1);
  await browser.click(...(await at(23.5, UP, 17.5)));
  await expect('  the stairs\' own opening left as it is', 'buildholes', 1);
  await press('uiInside');
  await press('uiPick_table');
  await browser.click(...(await at(22.5, UP, 16.5)));
  check('a table over the hole: refused', (await data('draft')).includes('inside:1') && (await data('status')).includes('a hole down to the floor below'), `${await data('draft')} · ${await data('status')}`);

  // 6. a wall segment bricks on every floor at once
  await press('uiStructure');
  await press('uiWallStyles');
  await press('uiWallEveryFloor');
  await browser.click(...(await at(24.0, UP, 18.5)));
  await expect('the east wall in bricks, up here', 'buildwalls', 1);
  await press('uiFloor1');
  await expect('  and on the ground floor', 'buildwalls', 2);
  await press('uiFloor2');
  await cmd('view:22.5,17.5,11,-35,48');
  await shot('2-hole-and-railing');
  await cmd('view:22.5,18,15,0,58');

  // 7. confirmed; walked: in through the door, up the stairs, the hole stops the walker
  await press('uiConfirm');
  await expect('confirmed', 'build', '');
  await expect('one more building', 'buildings', buildings0 + 1);
  await cmd('view:22.5,18,15,-35,40');
  await shot('3-confirmed');
  // from the north-east: the bricks on the north wall's first segment, on the east wall's
  // third up both floors
  await cmd('view:22.5,18,14,145,30');
  await shot('3b-bricks');
  await cmd('view:22.5,18,15,0,58');
  await browser.key('1');
  await press('uiWalk');
  await cmd('walkto:22,20');
  const step = async (key, want, what, wait = 330) => {
    await browser.key(key);
    await sleep(wait);
    await expect(what, 'walker', want);
  };
  await step('ArrowUp', '22,19', 'in through the door');
  await step('ArrowRight', '23,19', 'to the foot of the stairs');
  await step('ArrowUp', '23,16', 'up the stairs', 1600);
  check('  on the second floor', (await data('floor')).endsWith(':1'), await data('floor'));
  await step('ArrowLeft', '23,16', 'the hole stops it');
  check('  still on the second floor', (await data('floor')).endsWith(':1'), await data('floor'));
  await cmd('view:23,17,9,-40,45');
  await shot('4-upstairs');
  await browser.key('Escape');

  // 8. the download has it: the cells' flooring, the walls' styles, the hole
  await press('uiSave');
  await sleep(1500);
  const saved = JSON.parse(readFileSync(join(downloads, 'test_meadow.json'), 'utf8'));
  const b = saved.buildings?.[buildings0];
  const [f1, f2] = [b?.floors?.[0] ?? {}, b?.floors?.[1] ?? {}];
  check('downloaded', !!b && JSON.stringify(f1.tiles) === '[{"cell":[0,0],"tile":"stone_floor"}]' && f1.walls?.length === 2 && f2.walls?.length === 1
    && JSON.stringify(f2.holes) === '[[1,0]]' && f1.holes === undefined && f2.walls[0].side === 'e',
    b ? JSON.stringify({ tiles: f1.tiles, walls1: f1.walls, walls2: f2.walls, holes: f2.holes }) : 'no building');
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
