// The wind and the water in the built site, on a made-up map served in the bundle (the sea on
// a sandy coast to the west, a lake with a beach, tall grass, trees): inland water lands its
// small waves (their foam) on the shore the wind blows to, not the other; the sea's waves are
// rougher when the wind blows to the land than out to sea, and roll in either way; the tall
// grass and the trees sway in the wind and stand still when it's calm; the clouds' shadows drift
// the way the wind blows, morph as they go, and run on across a cliff; the Post-processing
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
// no clouds' shadows unless a part asks for them (they'd darken what's measured); never colour
// steps (they'd band it)
let clouds = { cloud_shadow: 0 };
// for the clouds: a grass map, lowland west, a plateau east (a level up), and the same all
// lowland
const cliffMap = {
  id: 'cloud_test', name: 'Cloud test', width: 40, height: 30, legend: { g: 'grass' },
  terrain: Array.from({ length: 30 }, () => 'g'.repeat(40)), levels: Array.from({ length: 30 }, () => '0'.repeat(20) + '1'.repeat(20)),
};
const flatMap = { ...cliffMap, id: 'cloud_flat', name: 'Cloud flat', levels: Array.from({ length: 30 }, () => '0'.repeat(40)) };
// a level's height, as the world says
let levelHeight = 2;
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/water_test.json'] = map;
  bundle.files['maps/cloud_test.json'] = cliffMap;
  bundle.files['maps/cloud_flat.json'] = flatMap;
  const world = bundle.files['settings/world.json'];
  levelHeight = (world.character_height ?? 1.5) + (world.level_padding ?? 0.5);
  world.wind = wind;
  world.toon = Object.assign(world.toon ?? {}, { color_steps: 0 }, clouds);
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
const screen = async (x, z, y = 0) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(150);
  return (await data('point')).split(',').map(Number);
};
// the map open in the editor, the wind (and the clouds) as given, the grid off
const open = async (w, c = { cloud_shadow: 0 }, id = 'water_test') => {
  wind = w;
  clouds = c;
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd(`map:${id}`);
  await browser.waitFor(`document.body.dataset.map === ${JSON.stringify(id)} && document.body.dataset.loading === "0"`, { timeout: 30000, what: 'the test map' });
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
// a picture's light in blocks of 8 by 8 pixels, over the 3D view
const BLOCK = 8;
const view = { x0: 270, y0: 70, x1: 1150, y1: 850 };
const blocks = (img) => {
  const w = Math.floor((view.x1 - view.x0) / BLOCK);
  const h = Math.floor((view.y1 - view.y0) / BLOCK);
  const out = new Float32Array(w * h);
  for (let by = 0; by < h; by++) {
    for (let bx = 0; bx < w; bx++) {
      let sum = 0;
      for (let y = 0; y < BLOCK; y++) {
        for (let x = 0; x < BLOCK; x++) {
          const i = ((view.y0 + by * BLOCK + y) * img.width + view.x0 + bx * BLOCK + x) * 4;
          sum += img.rgba[i] * 0.3 + img.rgba[i + 1] * 0.59 + img.rgba[i + 2] * 0.11;
        }
      }
      out[by * w + bx] = sum / (BLOCK * BLOCK);
    }
  }
  return { w, h, l: out };
};
// how far (in pixels) the second picture's light is moved from the first's: the shift that
// matches them best
const shift = (a, b, reach = 14) => {
  let best = [0, 0, Infinity];
  for (let dy = -reach; dy <= reach; dy++) {
    for (let dx = -reach; dx <= reach; dx++) {
      let sum = 0;
      let n = 0;
      for (let y = reach; y < a.h - reach; y++) {
        for (let x = reach; x < a.w - reach; x++) {
          sum += Math.abs(a.l[y * a.w + x] - b.l[(y + dy) * a.w + x + dx]);
          n++;
        }
      }
      if (sum / n < best[2]) best = [dx * BLOCK, dy * BLOCK, sum / n];
    }
  }
  return best;
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

  // 4. the clouds' shadows drift the way the wind blows (east, then south), and morph as they
  // go (with no drift at all, they still change); cast from their own height, they run on
  // across a cliff (cast from the ground's, a level's step shifted them, a seam along it)
  const heavy = { cloud_shadow: 0.8, cloud_cover: 0.5, cloud_size: 9.5, cloud_speed: 1.5 };
  const drift = async (direction) => {
    await open({ direction, strength: 0, gusts: 0 }, heavy, 'cloud_test');
    await cmd('view:10,15,24,0,89');
    await sleep(1200);
    const [ox, oy] = await screen(10, 15);
    const [ex, ey] = await screen(11, 15);
    const [sx, sy] = await screen(10, 16);
    const a = blocks(await browser.pixels(join(shots, `water-clouds-${direction}.png`)));
    await sleep(1000);
    const b = blocks(await browser.pixels());
    const [dx, dy] = shift(a, b, 24);
    // the way the wind blows, on the screen
    const way = direction === 90 ? [ex - ox, ey - oy] : [sx - ox, sy - oy];
    const len = Math.hypot(...way) * Math.hypot(dx, dy);
    return [dx, dy, len > 0 ? (dx * way[0] + dy * way[1]) / len : 0];
  };
  const [cx, cy, eastward] = await drift(90);
  check("the clouds' shadows drift the way the wind blows: east", eastward > 0.9 && Math.hypot(cx, cy) > 24, `moved ${cx},${cy} px, ${eastward.toFixed(2)} along the wind`);
  const [qx, qy, southward] = await drift(180);
  check('  and south', southward > 0.9 && Math.hypot(qx, qy) > 24, `moved ${qx},${qy} px, ${southward.toFixed(2)} along the wind`);
  await open({ direction: 90, strength: 0, gusts: 0 }, { ...heavy, cloud_speed: 0 }, 'cloud_test');
  await cmd('view:10,15,24,0,89');
  await sleep(1200);
  const still0 = await browser.pixels();
  await sleep(300);
  const still1 = await browser.pixels();
  await sleep(14000);
  const later = await browser.pixels();
  const centre = await screen(10, 15);
  const moment = changed(still0, still1, centre, 300);
  const morphed = changed(still0, later, centre, 300);
  check('  with no drift, they morph (slowly)', morphed > 0.02 && morphed > moment * 5, `${(morphed * 100).toFixed(1)}% changed in 14 s, ${(moment * 100).toFixed(1)}% in 0.3 s`);
  // a plateau a level up has the shadows the same ground has with no cliff at all (a flat
  // copy of the map, at the same moment: no drift, the morph slow); cast from the ground's
  // own height, a level's step moved them along the sun's way, a seam along it (some three
  // cells at 17:30)
  const still = { ...heavy, cloud_speed: 0 };
  const picture = async (id) => {
    await open({ direction: 90, strength: 0, gusts: 0 }, still, id);
    await cmd('period:17:30');
    await cmd('view:20,15,24,0,89');
    await sleep(1500);
    return browser.pixels(join(shots, `water-${id}.png`));
  };
  const flat = await picture('cloud_flat');
  const stepped = await picture('cloud_test');
  // where the ground (or the plateau's top) is on the screen: seen near enough straight down,
  // a level plane's points are where three of them say
  const plane = async (y) => {
    const [o, ex, ez] = [await screen(20, 15, y), await screen(24, 15, y), await screen(20, 19, y)];
    return (x, z) => [0, 1].map((k) => o[k] + ((ex[k] - o[k]) * (x - 20) + (ez[k] - o[k]) * (z - 15)) / 4);
  };
  const [ground, top] = [await plane(0), await plane(levelHeight)];
  const inside = ([px, py]) => px > view.x0 + 2 && px < view.x1 - 2 && py > view.y0 + 2 && py < view.y1 - 2;
  // the light round a point, 3 by 3 pixels
  const lum = (img, [px, py]) => {
    let sum = 0;
    for (let dy = -1; dy <= 1; dy++) {
      for (let dx = -1; dx <= 1; dx++) {
        const i = ((Math.round(py) + dy) * img.width + Math.round(px) + dx) * 4;
        sum += img.rgba[i] * 0.3 + img.rgba[i + 1] * 0.59 + img.rgba[i + 2] * 0.11;
      }
    }
    return sum / 9;
  };
  let apart = 0;
  const lights = [];
  for (let z = 9.5; z <= 20.5; z += 0.5) {
    for (let x = 21; x <= 25.5; x += 0.5) {
      const [a, b] = [ground(x, z), top(x, z)];
      if (!inside(a) || !inside(b)) continue;
      const l = lum(flat, a);
      apart += Math.abs(lum(stepped, b) - l);
      lights.push(l);
    }
  }
  const n = Math.max(lights.length, 1);
  const mean = lights.reduce((sum, l) => sum + l, 0) / n;
  const spread = lights.reduce((sum, l) => sum + Math.abs(l - mean), 0) / n;
  check('  cast from their own height: a plateau has the shadows its ground would have with no cliff', lights.length > 100 && spread > 6 && apart / n < spread * 0.3, `${(apart / n).toFixed(1)} apart, the shadows' own spread ${spread.toFixed(1)} (${lights.length} points)`);

  // 5. the Post-processing window's Wind: its way, strength and gusts; calm at the strength's
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
