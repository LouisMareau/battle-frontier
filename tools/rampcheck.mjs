// Ramps in the built site's map editor, on a made-up map served in the bundle (a plateau a
// level up north of the lowland, a bay of the lowland east of it, a tower two levels up
// beyond, a pond): a drag along the plateau's top edge makes a ramp down to the lowland, one
// cell deep; one along the tower's edge (2 down to 0), two rows deep, or in the plateau's
// middle (nothing a level down) is refused, saying why; on foot, up the ramp onto the plateau
// and back down, along it but never off its sides nor up the cliff beside it, the walker's
// height rising on the way; the ramp picked by a click, made smooth, taken away and back
// (undo); painted to another level, gone; and saved in the map's record. Screenshots to
// screenshots/ramp-*.png. Exits 1 when something fails.
//     node tools/rampcheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-ramps');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

// the map: 24 x 20 of grass; north of z 8 the plateau (level 1) west of x 14, a bay of the
// lowland (0) from x 14 to 17, the tower (2) from x 18; the lowland south of it, a pond in its
// south-west
const W = 24;
const H = 20;
const terrain = [];
const levels = [];
for (let z = 0; z < H; z++) {
  let t = '';
  let l = '';
  for (let x = 0; x < W; x++) {
    t += x >= 2 && x < 5 && z >= 15 && z < 18 ? 'w' : 'g';
    l += z >= 8 ? '0' : x < 14 ? '1' : x < 18 ? '0' : '2';
  }
  terrain.push(t);
  levels.push(l);
}
const map = { id: 'ramp_test', name: 'Ramp test', width: W, height: H, legend: { g: 'grass', w: 'water' }, terrain, levels };
let levelHeight = 2;
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/ramp_test.json'] = map;
  const world = bundle.files['settings/world.json'];
  levelHeight = (world.character_height ?? 1.5) + (world.level_padding ?? 0.5);
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9512, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
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
// a drag over cells (their centres, on their level), letting go on the last
const drag = async (cells) => {
  const points = [];
  for (const [x, z, level] of cells) points.push(await screen(x + 0.5, level * levelHeight, z + 0.5));
  await browser.drag(points);
  await sleep(500);
};
const shot = (name) => browser.screenshot(join(shots, `ramp-${name}.png`));

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:ramp_test');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "ramp_test"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  // the asset browser closed (it comes open, along the bottom): more of the view
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await cmd('view:10,10,30,0,55');
  await browser.click(700, 889); // the status bar: the canvas gets the keys
  await browser.key('2');
  await press('uiGroundRamps');
  check('the Ground tool on ramps', (await data('tool')) === 'ground' && (await data('groundmode')) === 'ramps', `${await data('tool')} ${await data('groundmode')}`);

  // 1. along the plateau's top edge: a ramp down to the lowland, one cell deep
  await drag([[5, 7, 1], [6, 7, 1], [7, 7, 1], [8, 7, 1]]);
  check('a drag along the top of the step makes a ramp', (await data('ramps')) === '1', `${await data('ramps')} ramps, ${await data('status')}`);
  check('  four long, one deep, rising north from its foot on the lowland', (await data('ramp')).startsWith('5,7;4;n;'), await data('ramp'));
  await cmd('view:7,8.5,14,330,30');
  await sleep(1200);
  await shot('1-made');

  // 2. refused: the tower's edge (2 down to 0), two rows deep, the plateau's middle
  await cmd('view:15,8,30,0,55');
  await drag([[19, 7, 2], [20, 7, 2]]);
  check('none from 2 down to 0', (await data('ramps')) === '1' && (await data('status')).includes('one level only'), await data('status'));
  await cmd('view:10,8,30,0,55');
  await drag([[10, 6, 1], [11, 7, 1]]);
  check('none two rows deep', (await data('ramps')) === '1' && (await data('status')).includes('one cell deep'), await data('status'));
  await drag([[9, 3, 1], [10, 3, 1]]);
  check('none where nothing is a level down', (await data('ramps')) === '1' && (await data('status')).includes("don't step down"), await data('status'));

  // 3. on foot: up the ramp onto the plateau, back down, along it; never off its sides, nor
  // up the cliff beside it
  await cmd('view:10,10,30,0,55');
  await press('uiWalk');
  await cmd('walkto:6,10');
  await sleep(400);
  const step = async (key, want, what) => {
    await browser.key(key);
    await sleep(330);
    const got = await data('walker');
    check(what, got === want, `walker ${got} (wanted ${want})`);
    return Number(await data('walkery'));
  };
  const heights = [Number(await data('walkery'))];
  for (const [z, what] of [[9, 'towards it'], [8, 'to its foot'], [7, 'onto the ramp'], [6, 'up onto the plateau']]) {
    heights.push(await step('ArrowUp', `6,${z}`, what));
  }
  check('  the walker rising all the way, a level in all', heights.every((h, i) => i === 0 || h >= heights[i - 1] - 0.05) && Math.abs(heights.at(-1) - levelHeight) < 0.05 && Math.abs(heights[0]) < 0.05, heights.map((h) => h.toFixed(2)).join(' '));
  check('  on it, halfway', heights[3] > 0.3 * levelHeight && heights[3] < 0.7 * levelHeight, `${heights[3]}`);
  await shot('2-up');
  await step('ArrowDown', '6,7', 'back down onto it');
  await step('ArrowLeft', '5,7', '  along it');
  await step('ArrowLeft', '5,7', '  not off its side');
  await step('ArrowDown', '5,8', '  down off its foot');
  await step('ArrowLeft', '4,8', '  beside it');
  await step('ArrowUp', '4,8', '  not up the cliff beside it');
  await press('uiWalk');

  // 4. picked by a click; made smooth; taken away, and back
  await cmd('view:10,10,30,0,55');
  const [rx, ry] = await screen(6.5, levelHeight / 2, 7.5);
  await browser.click(rx, ry);
  await sleep(400);
  check('a click picks it', (await data('ramp')).startsWith('5,7;4;n;'), await data('ramp'));
  // (its slider pulled all the way left)
  const [sx, sy] = await at('uiRampRough');
  await browser.drag([[sx - 60, sy], [sx - 160, sy], [sx - 320, sy]]);
  await sleep(400);
  check('  made smooth', (await data('ramp')).endsWith(';0.00'), await data('ramp'));
  await press('uiRampRemove');
  check('  taken away', (await data('ramps')) === '0', await data('ramps'));
  await browser.click(700, 889);
  await browser.key('z', { ctrl: true });
  await sleep(500);
  check('  back with an undo', (await data('ramps')) === '1', await data('ramps'));

  // 5. saved with the map
  await press('uiSave');
  await sleep(1500);
  const saved = JSON.parse(readFileSync(join(downloads, 'ramp_test.json'), 'utf8'));
  const r = saved.ramps?.[0];
  check('saved in the record', saved.ramps?.length === 1 && r.at.join(',') === '5,7' && r.size.join(',') === '4,1' && r.up === 'n', JSON.stringify(saved.ramps));

  // 6. a cell of it painted to another level (the lowland's): the ramp gone
  await press('uiGroundPaint');
  await cmd('view:10,10,30,0,55');
  const [px, py] = await screen(6.5, 0, 7.5);
  await browser.click(px, py);
  await sleep(500);
  check('a cell of it painted a level down: the ramp gone', (await data('ramps')) === '0', `${await data('ramps')} ramps, ${await data('status')}`);
  if (browser.errors.length) check('no page errors', false, browser.errors.join(' | '));
} catch (e) {
  console.log(`FAIL ${e.message}`);
  failed++;
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
