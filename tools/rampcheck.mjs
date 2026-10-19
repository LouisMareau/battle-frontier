// Ramps in the built site's map editor, on a made-up map served in the bundle (a plateau a
// level up north of the lowland, a bay of the lowland east of it, a tower two levels up
// beyond, a pond): a drag along the plateau's top edge makes a ramp down to the lowland, one
// cell deep; one along the tower's edge (2 down to 0), two rows deep, or in the plateau's
// middle (nothing a level down) is refused, saying why; on foot, up the ramp onto the plateau
// and back down, along it but never off its sides nor up the cliff beside it, the walker's
// height rising on the way; the ramp picked by a click, made smooth, taken away and back
// (undo); saved in the map's record. Then what lies along ramps: stone stairs from the
// asset browser, refused by the Place tool off a ramp (saying why), laid on the ramp (made as
// uneven as it gets) from its foot to its top and end to end, every tread stone where the
// ramp's ground would show, the grass before its foot and past its top; a mud slide on a
// ramp rising west, down to the bay, from a click past its foot; both saved in the map's
// record; and the ramp painted to another level, gone, the stairs with it. Screenshots to
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
  // the colours as they are: no cloud shadows drifting over, no colour steps
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
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

  // 6. stone stairs along it. First the ramp as uneven as it gets (its ground furthest from
  // the straight line its stairs are made on): picked, its slider pulled all the way right
  await cmd('period:12:00');
  await cmd('view:10,10,30,0,55');
  const [qx, qy] = await screen(6.5, levelHeight / 2, 7.5);
  await browser.click(qx, qy);
  await sleep(400);
  const [ux, uy] = await at('uiRampRough');
  await browser.drag([[ux, uy], [ux + 160, uy], [ux + 320, uy]]);
  await sleep(400);
  check('the ramp made as uneven as it gets', (await data('ramp')).endsWith(';1.00'), await data('ramp'));
  // the stairs and the slide from the asset browser's Ramps folder
  const pick = async (id) => {
    if (!(await data('layout')).includes('assets=bottom')) await press('uiAssets');
    await press('uiFolder_Ramps');
    await press(`uiAsset_${id}`);
    await press('uiAssets');
    await browser.click(700, 889);
    await browser.key('3');
    await sleep(300);
  };
  await pick('stone_stairs');
  check('the Place tool with the stairs', (await data('tool')) === 'place', await data('tool'));
  // the points their treads are on, at the back of each (where the ramp's ground, were it
  // left as it is, would come up through them), along the ramp but its ends' walls; the grass
  // just before its foot and just past its top
  const treads = [];
  for (let k = 0; k < 8; k++) {
    for (const x of [5.5, 6.3, 7.1, 7.9, 8.5]) treads.push([x, ((k + 1) * levelHeight) / 8 + 0.003, 8 - (k + 0.85) / 8]);
  }
  const outside = [6, 7, 8].flatMap((x) => [[x, 0.003, 8.1], [x, levelHeight + 0.003, 6.9]]);
  const front = 'view:7,7.5,11,0,35';
  const look = async () => {
    await cmd(front);
    await sleep(1200);
    const places = [];
    for (const p of [...treads, ...outside]) places.push(await screen(...p));
    return places;
  };
  const colour = (img, [x, y]) => {
    const i = (Math.round(y) * img.width + Math.round(x)) * 4;
    return [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]];
  };
  const green = ([r, g, b]) => g > r + 10 && g > b + 10;
  const stone = ([r, g, b]) => Math.abs(r - g) < 28 && Math.abs(g - b) < 32 && r + g + b > 120;
  // bare: the ramp's own ground (grass, rock down its middle) where the treads will be
  const places = await look();
  const bare = await browser.pixels(join(shots, 'ramp-3-bare.png'));
  const bareGreen = treads.filter((_, i) => green(colour(bare, places[i]))).length;
  // off a ramp (the lowland, two cells before its foot): refused, saying why
  const props0 = Number(await data('props'));
  await browser.click(...(await screen(7.5, 0, 10.5)));
  await sleep(400);
  check('off a ramp: refused', Number(await data('props')) === props0 && (await data('status')).startsWith('No ramp there to lay Stone stairs along'), `${await data('props')} props, ${await data('tool')}: ${await data('status')}`);
  // on it: laid from its foot to its top, end to end, turned up its way
  await browser.click(...(await screen(7.0, levelHeight / 2, 7.5)));
  await sleep(1500);
  check('on the ramp: laid along it', Number(await data('props')) === props0 + 1 && (await data('along')) === 'stone_stairs@5,7x1', `${await data('props')} props, along ${await data('along')}`);
  check('  and says so', (await data('status')) === 'Stone stairs along the ramp at 5, 7: 4 cells long, its foot to the south', await data('status'));
  // (the pointer off the view and the stairs let go: no marks over them)
  await browser.move(120, 460);
  await browser.key('Escape');
  const laid = await look();
  const img = await browser.pixels(join(shots, 'ramp-4-stairs.png'));
  const onTreads = treads.map((_, i) => colour(img, laid[i]));
  const notStone = onTreads.filter((c) => !stone(c));
  check('  every tread stone, none of the ramp\'s ground through them', notStone.length === 0, `${treads.length - notStone.length} of ${treads.length} stone (bare, ${bareGreen} were grass)${notStone.length ? `; not: ${JSON.stringify(notStone.slice(0, 4))}` : ''}`);
  const around = outside.map((_, i) => colour(img, laid[treads.length + i]));
  check('  flush: grass just before its foot and just past its top', around.every(green), JSON.stringify(around));
  await cmd('view:7,7.5,9,330,22');
  await sleep(1200);
  await shot('5-stairs-turned');
  await cmd('view:7,8,7,60,18');
  await sleep(1200);
  await shot('6-stairs-low');

  // 7. a mud slide on a ramp down the plateau's east side to the bay, rising west: a click
  // on the bay past its foot puts it on
  await browser.click(700, 889);
  await browser.key('2');
  await press('uiGroundRamps');
  await cmd('view:15,5,30,0,55');
  await drag([[13, 3, 1], [13, 4, 1], [13, 5, 1]]);
  check('a ramp down to the bay, rising west', (await data('ramps')) === '2' && (await data('ramp')).startsWith('13,3;3;w;'), `${await data('ramps')} ramps, ${await data('ramp')}`);
  await pick('mud_slide');
  check('the Place tool with the slide', (await data('tool')) === 'place', await data('tool'));
  // (seen from the south-east, over the bay: the tower beyond it hides it from the east)
  await cmd('view:14,4.5,12,30,60');
  await sleep(400);
  await browser.click(...(await screen(14.5, 0, 4.5)));
  await sleep(1500);
  check('the slide on it, from a click past its foot', (await data('along')) === 'stone_stairs@5,7x1;mud_slide@13,3x1', `along ${await data('along')}, ${await data('status')}`);
  check('  and says so', (await data('status')) === 'Mud slide along the ramp at 13, 3: 3 cells long, its foot to the east', await data('status'));
  await browser.move(120, 460);
  await browser.key('Escape');
  await cmd('view:13.5,4.5,11,30,35');
  await sleep(1200);
  await shot('7-slide');

  // 8. both in the map's record: on their ramps' cells, their level, turned up their ways
  rmSync(join(downloads, 'ramp_test.json'), { force: true });
  await press('uiSave');
  await sleep(1500);
  const record = JSON.parse(readFileSync(join(downloads, 'ramp_test.json'), 'utf8'));
  const placed = (id) => record.props?.find((p) => p.prop === id);
  const stairs = placed('stone_stairs');
  const slide = placed('mud_slide');
  check('saved: the stairs', stairs?.at?.join(',') === '7,7.5' && stairs.level === 1 && !stairs.turn, JSON.stringify(stairs));
  check('saved: the slide', slide?.at?.join(',') === '13.5,4.5' && slide.level === 1 && slide.turn === 270, JSON.stringify(slide));

  // 9. a cell of the first ramp painted to another level (the lowland's): the ramp gone, and
  // the stairs with it
  await browser.click(700, 889);
  await browser.key('2');
  await press('uiGroundPaint');
  await cmd('view:10,10,30,0,55');
  const [px, py] = await screen(6.5, 0, 7.5);
  await browser.click(px, py);
  await sleep(500);
  check('a cell of it painted a level down: the ramp gone', (await data('ramps')) === '1', `${await data('ramps')} ramps, ${await data('status')}`);
  check('  with the stairs along it', (await data('along')) === 'mud_slide@13,3x1' && (await data('status')).includes('with what lay along it'), `along ${await data('along')}, ${await data('status')}`);
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
