// Painting levels in the built site's map editor, on a made-up map served in the bundle (a
// lowland, a hill two levels up in its north), seen from the south: with level 2 picked, the
// Ground brush aims at the cell under the pointer on its own ground (not where the pointer
// meets level 2), and a click raises that cell; a 3 x 3 stroke held still raises its nine
// cells and no more (the cells it's raised don't pull it towards the camera); with level 0
// picked, a cell of the hill's top sinks, the one under the pointer; ground raised to meet the
// hill's top joins it with no seam (the ground shader told the cells' new levels: it shaded the
// old cliff's foot along the join); the map saved, its levels changed there and nowhere else. Screenshots to screenshots/level-*.png. Exits 1 when
// something fails.
//     node tools/levelcheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-levels');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

// the map: 20 x 16 of grass on level 0, the hill (level 2) from x 12 to 16 and z 2 to 5
const W = 20;
const H = 16;
const hill = (x, z) => x >= 12 && x <= 16 && z >= 2 && z <= 5;
const base = [];
for (let z = 0; z < H; z++) {
  let l = '';
  for (let x = 0; x < W; x++) l += hill(x, z) ? '2' : '0';
  base.push(l);
}
const map = { id: 'level_test', name: 'Level test', width: W, height: H, legend: { g: 'grass' }, terrain: base.map(() => 'g'.repeat(W)), levels: base };
let levelHeight = 1;
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/level_test.json'] = map;
  const world = bundle.files['settings/world.json'];
  // a level of ground: half a floor (a character and room above it)
  levelHeight = ((world.character_height ?? 1.5) + (world.level_padding ?? 0.5)) / 2;
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9513, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(250);
};
const at = async (name) => (await data(name)).split(',').map(Number);
const press = async (name, wait = 400) => {
  const [x, y] = await at(name);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(wait);
};
// where a point of the world is on the screen
const screen = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(150);
  return at('point');
};
// the pointer over a cell's centre at a level's height: the cell the editor says is under it
const hover = async (x, z, level) => {
  const p = await screen(x + 0.5, level * levelHeight, z + 0.5);
  await browser.move(...p);
  await sleep(300);
  return { p, under: await data('groundcell') };
};
const shot = (name) => browser.screenshot(join(shots, `level-${name}.png`));

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:level_test');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "level_test"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  // the asset browser closed (it comes open, along the bottom): more of the view
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await browser.click(700, 889); // the status bar: the canvas gets the keys
  await browser.key('2');
  check('the Ground tool, painting', (await data('tool')) === 'ground' && (await data('groundmode')) === 'paint', `${await data('tool')} ${await data('groundmode')}`);
  await browser.key('PageUp');
  await browser.key('PageUp');
  check('level 2 picked', (await data('level')) === '2', await data('level'));
  await cmd('view:9,9,26,0,52');
  await sleep(600);

  // 1. a cell of the lowland: aimed at on its own ground, raised by a click
  const aim = await hover(6, 10, 0);
  check('level 2 picked, the brush aims at the cell under the pointer, on level 0', aim.under === '6,10,0', aim.under);
  await shot('1-aim');
  await browser.click(...aim.p);
  await sleep(500);
  const raised = await hover(6, 10, 2);
  check('  a click raises that cell', raised.under === '6,10,2', raised.under);

  // 2. a 3 x 3 stroke held still: its nine cells and no more
  await cmd('brush:1');
  const held = await hover(3, 9, 0);
  check('a 3 x 3 brush on the lowland, aimed at its own ground', held.under === '3,9,0', held.under);
  await browser.press();
  await sleep(900);
  await browser.release();
  await sleep(500);
  await shot('2-held');

  // 3. level 0 picked: a cell of the hill's top sinks, the one under the pointer
  await browser.key('PageDown');
  await browser.key('PageDown');
  check('level 0 picked', (await data('level')) === '0', await data('level'));
  await cmd('brush:0');
  await cmd('view:12,8,26,0,52');
  await sleep(600);
  const top = await hover(14, 5, 2);
  check('level 0 picked, the brush aims at the hill top under the pointer, on level 2', top.under === '14,5,2', top.under);
  await shot('3-sink-aim');
  await browser.click(...top.p);
  await sleep(500);
  const sunk = await hover(14, 5, 0);
  check('  a click sinks that cell', sunk.under === '14,5,0', sunk.under);
  await shot('4-sunk');

  // 3b. raised to the hill's level against its edge: the ground runs on, no seam where they join
  await browser.key('PageUp');
  await browser.key('PageUp');
  check('level 2 picked again', (await data('level')) === '2', await data('level'));
  await cmd('view:12,8,20,0,52');
  await sleep(600);
  for (const x of [12, 13]) {
    const { p } = await hover(x, 6, 0);
    await browser.click(...p);
    await sleep(300);
  }
  // (the brush's square off the ground being measured)
  await browser.move(1300, 450);
  await sleep(1200);
  const joined = await browser.pixels(join(shots, 'level-5-joined.png'));
  const light = ([x, y]) => {
    const i = (Math.round(y) * joined.width + Math.round(x)) * 4;
    return joined.rgba[i] * 0.3 + joined.rgba[i + 1] * 0.59 + joined.rgba[i + 2] * 0.11;
  };
  const byJoin = [];
  const onHill = [];
  for (const x of [12.3, 12.7, 13.3, 13.7]) {
    byJoin.push(light(await screen(x, 2 * levelHeight, 6.12)));
    onHill.push(light(await screen(x, 2 * levelHeight, 5.5)));
  }
  const mean = (a) => a.reduce((t, v) => t + v, 0) / a.length;
  check('  raised to meet the hill: no seam where they join, the ground as light on both sides', mean(byJoin) > mean(onHill) * 0.93,
    `${mean(byJoin).toFixed(0)} by the join, ${mean(onHill).toFixed(0)} on the hill`);

  // 4. saved: the levels changed there, and nowhere else
  await press('uiSave');
  await sleep(1500);
  const saved = JSON.parse(readFileSync(join(downloads, 'level_test.json'), 'utf8'));
  const want = base.map((row, z) => [...row].map((c, x) => (x === 6 && z === 10) || (x >= 2 && x <= 4 && z >= 8 && z <= 10) || (z === 6 && (x === 12 || x === 13)) ? '2' : x === 14 && z === 5 ? '0' : c).join(''));
  const off = [];
  for (let z = 0; z < H; z++) {
    for (let x = 0; x < W; x++) {
      const got = saved.levels?.[z]?.[x] ?? '0';
      if (got !== want[z][x]) off.push(`${x},${z} is ${got} (wanted ${want[z][x]})`);
    }
  }
  check('saved: raised and sunk where the pointer was, nowhere else', off.length === 0, off.join('; ') || `${saved.levels?.length} rows`);
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
