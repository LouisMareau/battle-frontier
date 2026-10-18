// The wind and the water in the built site, on a made-up map served in the bundle (the sea on
// a sandy coast to the west, a lake with a beach, tall grass, trees): inland water lands its
// small waves (their foam) on the shore the wind blows to, not the other; the sea's waves are
// rougher when the wind blows to the land than out to sea, and roll in either way; the tall
// grass and the trees sway in the wind and stand still when it's calm; the Post-processing
// window's Wind section sets it (the way, the strength, the gusts), back to the defaults too.
// Screenshots to screenshots/water-*.png. Exits 1 when something fails.
//     node tools/watercheck.mjs
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

// the map: the sea (oceanic) west of a wavy coast of sand, a lake (inland) with sand on its
// east side, tall grass, trees
const W = 40;
const H = 30;
const terrain = [];
for (let z = 0; z < H; z++) {
  let row = '';
  for (let x = 0; x < W; x++) {
    const coast = 9 + Math.round(2.2 * Math.sin(z * 0.45) + 1.2 * Math.sin(z * 1.3 + 1));
    const lake = Math.hypot((x - 26) / 1.2, z - 11);
    let c = 'g';
    if (x < coast) c = 'o';
    else if (x < coast + 3) c = 's';
    else if (lake < 5) c = 'w';
    else if (lake < 7 && x > 25) c = 's';
    else if (x >= 30 && z >= 20 && z < 28) c = 't';
    row += c;
  }
  terrain.push(row);
}
const props = [[18, 4, 'round_tree'], [20, 6, 'pine_tree'], [16, 7, 'round_tree'], [21, 20, 'bush'], [23, 21, 'flowers']].map(([x, z, prop]) => ({ prop, at: [x + 0.5, z + 0.5], level: 0 }));
const map = { id: 'water_test', name: 'Water test', width: W, height: H, legend: { g: 'grass', s: 'sand', w: 'water', o: 'ocean', t: 'tall_grass' }, terrain, props };
let wind = { direction: 90, strength: 0.7, gusts: 0.3 };
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/water_test.json'] = map;
  const world = bundle.files['settings/world.json'];
  world.wind = wind;
  // no clouds' shadows, no colour steps: they'd darken or band what's measured
  world.toon = Object.assign(world.toon ?? {}, { cloud_shadow: 0, color_steps: 0 });
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9503, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(250);
};
const at = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await at(name);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(300);
};
// where a point of the world is on the screen
const screen = async (x, z) => {
  await cmd(`point:${x},0,${z}`);
  await sleep(150);
  return (await data('point')).split(',').map(Number);
};
// the map open in the editor, the wind as given, the grid off
const open = async (w) => {
  wind = w;
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:water_test');
  await browser.waitFor('document.body.dataset.map === "water_test" && document.body.dataset.loading === "0"', { timeout: 30000, what: 'the test map' });
  await sleep(1500);
  await browser.click(700, 450);
  await browser.key('g');
};
// how much of a box round a point of the screen is foam (near white), and how much changed
// between two pictures
const foam = (img, [cx, cy], r) => {
  let n = 0;
  let all = 0;
  for (let y = Math.round(cy - r); y < cy + r; y++) {
    for (let x = Math.round(cx - r); x < cx + r; x++) {
      const i = (y * img.width + x) * 4;
      const [R, G, B] = [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]];
      all++;
      // (the sand's B and the water's R stay under it)
      if (Math.min(R, G, B) > 185) n++;
    }
  }
  return n / all;
};
const changed = (a, b, [cx, cy], r) => {
  let n = 0;
  let all = 0;
  for (let y = Math.round(cy - r); y < cy + r; y++) {
    for (let x = Math.round(cx - r); x < cx + r; x++) {
      const i = (y * a.width + x) * 4;
      all++;
      if (Math.abs(a.rgba[i] - b.rgba[i]) + Math.abs(a.rgba[i + 1] - b.rgba[i + 1]) + Math.abs(a.rgba[i + 2] - b.rgba[i + 2]) > 30) n++;
    }
  }
  return n / all;
};
// foam round a point, summed over a few moments (the waves come and go)
const foamOver = async (point, r, name) => {
  let sum = 0;
  for (let k = 0; k < 4; k++) {
    const img = await browser.pixels(k === 0 && name ? join(shots, `water-${name}.png`) : undefined);
    sum += foam(img, point, r);
    await sleep(700);
  }
  return sum / 4;
};

try {
  await browser.viewport(1400, 900);

  // 1. inland: the wind blows east; its small waves land on the lake's east shore
  await open({ direction: 90, strength: 0.7, gusts: 0.3 });
  check('the wind as the world says', (await data('wind')) === '90 0.70 0.30', await data('wind'));
  await cmd('view:26,11,30,0,89');
  await sleep(1500);
  const east = await screen(31.6, 11);
  const west = await screen(20.4, 11);
  const inView = ([x, y]) => x > 270 && x < 1150 && y > 60 && y < 860;
  check('  both shores in the view', inView(east) && inView(west), `${east} ${west}`);
  // a cell on the screen
  const r = Math.abs(east[0] - west[0]) / 11.2;
  const downwind = await foamOver(east, r, 'lake');
  const upwind = await foamOver(west, r);
  check('inland: foam where the wind lands its waves (east), little where it leaves (west)', downwind > upwind * 1.5 && downwind > 0.02, `${(downwind * 100).toFixed(1)}% vs ${(upwind * 100).toFixed(1)}%`);
  // the lake ripples
  const a = await browser.pixels();
  await sleep(1000);
  const b = await browser.pixels();
  check('  its water moves', changed(a, b, await screen(26, 11), r * 2) > 0.05);

  // 2. the sea: its waves rougher when the wind blows to the land (east) than out to sea
  await cmd('view:10,15,22,0,89');
  await sleep(1500);
  const coast = await screen(8.5, 15);
  check('  the coast in the view', inView(coast), `${coast}`);
  const onshore = await foamOver(coast, r * 1.5, 'sea-onshore');
  const c1 = await browser.pixels();
  await sleep(1000);
  const c2 = await browser.pixels();
  check('the sea rolls in', changed(c1, c2, coast, r * 1.5) > 0.05);
  await open({ direction: 270, strength: 0.7, gusts: 0.3 });
  await cmd('view:10,15,22,0,89');
  await sleep(1500);
  const offshore = await foamOver(coast, r * 1.5, 'sea-offshore');
  check('  rougher with the wind blowing to the land than out to sea', onshore > offshore * 1.2, `${(onshore * 100).toFixed(1)}% vs ${(offshore * 100).toFixed(1)}%`);
  const d1 = await browser.pixels();
  await sleep(1000);
  const d2 = await browser.pixels();
  check('  and it rolls in still, the wind against it', changed(d1, d2, coast, r * 1.5) > 0.03);

  // 3. the tall grass and the trees sway in the wind, and stand still in the calm
  const sway = async () => {
    await cmd('view:33,24,10,0,40');
    await sleep(1500);
    const g = await screen(33.5, 24);
    const p = await browser.pixels();
    await sleep(900);
    const q = await browser.pixels();
    await cmd('view:18,6,10,0,40');
    await sleep(1500);
    const t = await screen(18.5, 4.5);
    const s = await browser.pixels();
    await sleep(900);
    const u = await browser.pixels();
    return [changed(p, q, g, 120), changed(s, u, t, 120)];
  };
  await open({ direction: 90, strength: 0.7, gusts: 0.3 });
  const [grass, trees] = await sway();
  await open({ direction: 90, strength: 0, gusts: 0 });
  const [calmGrass, calmTrees] = await sway();
  check('the tall grass sways in the wind', grass > 0.06 && grass > calmGrass * 3, `${(grass * 100).toFixed(1)}% vs ${(calmGrass * 100).toFixed(1)}% calm`);
  check('  the trees too', trees > 0.01 && trees > calmTrees * 3, `${(trees * 100).toFixed(1)}% vs ${(calmTrees * 100).toFixed(1)}% calm`);

  // 4. the Post-processing window's Wind: its way, strength and gusts; calm at the strength's
  // left end; back to the defaults
  await open({ direction: 90, strength: 0.7, gusts: 0.3 });
  await press('uiPost');
  for (const m of ['uiPostWindWay', 'uiPostWindStrength', 'uiPostWindGusts']) check(`  ${m} there`, (await at(m))[0] > 0, await data(m));
  const [lx, ly, , lh] = (await data('uiPostWindStrengthRect')).split(',').map(Number);
  await browser.click(lx + 2, ly + lh / 2);
  await sleep(300);
  check('the strength at its left end: calm, not saved yet', (await data('wind')).split(' ')[1] === '0.00' && (await data('postdirty')) === '1', `${await data('wind')} ${await data('postdirty')}`);
  await browser.screenshot(join(shots, 'water-post.png'));
  await press('uiPostDefaults');
  check('  Defaults: the breeze it comes with', (await data('wind')) === '112 0.35 0.50', await data('wind'));
  if (browser.errors.length) check('no errors on the page', false, browser.errors.join(' | ').slice(0, 300));
} catch (e) {
  console.log(`FAIL ${e.message}`);
  failed++;
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
