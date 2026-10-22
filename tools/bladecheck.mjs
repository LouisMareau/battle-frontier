// A meadow's blades and flowers made on the GPU as they're drawn, in the built site's map
// editor: a made-up map (64 x 64 of tall ground growing a meadow, a plateau a level up in its
// middle, a strip of short grass along its west edge), its meadow a grass, daisies and poppies
// (in clusters). Looked at from four distances: blades drawn, culled on the GPU; up close the
// finest templates, far off the simplest; what they cost in vertices within what the picture
// can show; flowers among them; the frame rate; no error. Then the wind: up close, the blades
// sway (the picture changes from one moment to the next), still when the wind is calm, and
// still in the wind when the plants are rigid (their sway 0). With --webgl, the page without
// WebGPU (WebGL 2: no compute shaders, the CPU culling patch by patch). Screenshots to
// screenshots/blades-*.png (blades-webgl-*.png). Exits 1 when something fails.
//     node tools/bladecheck.mjs [--webgl]
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

// the map: 64 x 64, the meadow but a strip of short grass along x 0 to 2, a plateau a level up
// from 24 to 40 each way
const S = 64;
const field = (id) => {
  const terrain = [];
  const levels = [];
  for (let z = 0; z < S; z++) {
    let t = '';
    let l = '';
    for (let x = 0; x < S; x++) {
      t += x < 3 ? 'g' : 'm';
      l += x >= 24 && x < 40 && z >= 24 && z < 40 ? '1' : '0';
    }
    terrain.push(t);
    levels.push(l);
  }
  return { id, name: id, width: S, height: S, legend: { g: 'grass', m: 'meadow_check' }, terrain, levels };
};
const plants = [
  { name: 'Meadow grass', shape: 'blade', weight: 1, height: [0.3, 0.55], width: 0.035, lean: 0.25, bend: 0.4, sway: 1 },
  { name: 'Daisy', shape: 'flower', weight: 0.1, height: [0.18, 0.32], width: 0.012, lean: 0.1, bend: 0.15, sway: 0.45, petals: 8, petal: '#f6f3ea', heart: '#eab92a', bloom: 0.09 },
  { name: 'Poppy', shape: 'flower', weight: 0.08, height: [0.3, 0.5], width: 0.012, lean: 0.1, bend: 0.15, sway: 0.45, petals: 4, petal: '#d0301e', heart: '#2a1a12', bloom: 0.12, clusters: 0.6 },
];
// the wind as the page is opened (a breeze from the west), and whether the plants give to it
let wind = { direction: 90, strength: 0.7, gusts: 0.3 };
let giving = true;
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/meadow_check.json'] = field('meadow_check');
  const tall = bundle.files['terrains/tall_grass.json'];
  const grown = plants.map((p) => ({ ...p, sway: giving ? p.sway : 0 }));
  bundle.files['terrains/meadow_check.json'] = { ...tall, id: 'meadow_check', name: 'Meadow check', growth: { meadow: { plants: grown, density: 48, patchy: 0.3, clump: 0.6, clumping: 0.5 } } };
  // the colours as they are: no cloud shadows drifting over, no colour steps
  const world = bundle.files['settings/world.json'];
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
  world.wind = wind;
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9521, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const num = async (key) => Number(await data(key));
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(250);
};
const press = async (name, wait = 400) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(wait);
};
const shot = (name) => browser.screenshot(join(shots, `blades-${webgl ? 'webgl-' : ''}${name}.png`));
// a look from `view` (x, z, distance, yaw, pitch): once the squares are in and a second's
// stats are, the blades drawn, their vertices, the frame rate, how many with each template
const look = async (view, name) => {
  await cmd(`view:${view}`);
  await sleep(3500);
  const out = { blades: await num('blades'), vertices: await num('bladeverts'), fps: await num('fps'), templates: (await data('bladetemplates')).split(',').map(Number) };
  await shot(name);
  console.log(`  ${name} (${view}): ${out.blades} blades, ${out.vertices} vertices, ${out.fps} fps; by template ${out.templates.join(' ')}`);
  return out;
};

// the page opened on the test map, the wind as `wind` says and the plants giving to it or not
const open = async (w, gives = true) => {
  wind = w;
  giving = gives;
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:meadow_check');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "meadow_check"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await cmd('view:32,32,24,0,50');
  await browser.waitFor('Number(document.body.dataset.blades ?? 0) > 0', { timeout: 60000, what: 'blades drawn' });
};
// how many pixels of the view (its middle, clear of the panels) change from one moment to the
// next, up close among the blades: what the wind moves
const moving = async (name) => {
  await cmd('view:20,52,6,0,35');
  await sleep(3000);
  const [l, t, r, b] = (await data('viewrect')).split(',').map(Number);
  const a = await browser.pixels(join(shots, `blades-${webgl ? 'webgl-' : ''}${name}-a.png`));
  await sleep(700);
  const c = await browser.pixels(join(shots, `blades-${webgl ? 'webgl-' : ''}${name}-b.png`));
  let n = 0;
  for (let y = Math.round(t + 40); y < b - 40; y += 2) {
    for (let x = Math.round(l + 40); x < r - 40; x += 2) {
      const i = (y * a.width + x) * 4;
      if (Math.max(Math.abs(a.rgba[i] - c.rgba[i]), Math.abs(a.rgba[i + 1] - c.rgba[i + 1]), Math.abs(a.rgba[i + 2] - c.rgba[i + 2])) > 28) n++;
    }
  }
  return n;
};

try {
  await browser.viewport(1400, 900);
  if (webgl) await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: "Object.defineProperty(Navigator.prototype, 'gpu', { get: () => undefined, configurable: true });" });
  await open(wind);
  if (webgl) check('culled on the CPU (WebGL 2)', (await data('bladesgpu')) === '0', await data('bladesgpu'));
  else check('culled on the GPU (WebGPU)', (await data('bladesgpu')) === '1', await data('bladesgpu'));
  const mid = await look('32,32,24,0,50', 'mid');
  const near = await look('20,52,6,0,35', 'near');
  const far = await look('32,32,70,0,55', 'far');
  const farthest = await look('32,32,160,0,60', 'farthest');
  const flowers = (at) => at.templates[3] + at.templates[4] + at.templates[5];
  check('blades drawn, flowers among them', mid.blades > 2000 && flowers(mid) > 0 && flowers(mid) < mid.blades / 3, `${mid.blades} blades, ${flowers(mid)} flowers`);
  check('  up close the finest', near.templates[0] > near.templates[2] && near.templates[0] > 0, near.templates.join(' '));
  // (on WebGL 2 a patch takes the finest template its nearest blade needs: from far off the
  // middle one, the simplest only from the farthest)
  const simplest = webgl ? far.templates[0] === 0 && farthest.templates[2] > farthest.templates[1] : far.templates[2] > far.templates[0];
  check('  far off the simplest, never the finest', simplest && farthest.templates[0] === 0, `${far.templates.join(' ')} / ${farthest.templates.join(' ')}`);
  // the share drawn falls with the pixels a cell covers: from far off a cell holds a blade or
  // two of the 48 it grows
  const cells = S * (S - 3);
  check('  from the farthest only a few of every cell\'s', farthest.blades < cells * 48 * 0.1, `${farthest.blades} drawn of ${cells * 48} grown`);
  for (const [name, at] of [['mid', mid], ['near', near], ['far', far], ['farthest', farthest]]) {
    check(`  ${name}: within what the picture shows`, at.vertices > 0 && at.vertices < 1400 * 900 * 4, `${at.vertices} vertices`);
  }
  check('  the frame keeps up', mid.fps >= 20 && far.fps >= 20, `${mid.fps} and ${far.fps} fps`);
  // the wind: the blades sway (the picture changes moment to moment), not when it's calm, nor
  // when the plants are rigid
  const windy = await moving('windy');
  await open({ ...wind, strength: 0 });
  const calm = await moving('calm');
  await open(wind, false);
  const rigid = await moving('rigid');
  console.log(`  moving: ${windy} pixels in the wind, ${calm} calm, ${rigid} rigid in the wind`);
  check('the blades sway in the wind', windy > 1500 && windy > 10 * Math.max(calm, 1), `${windy} pixels change, ${calm} when calm`);
  check('  still when the wind is calm', calm < 150, `${calm} pixels`);
  check('  rigid plants (their sway 0) stand still in the wind', rigid < 150, `${rigid} pixels`);
  check('no error in the page', browser.errors.length === 0, browser.errors.slice(0, 3).join(' | '));
} catch (e) {
  check('ran to its end', false, e.message);
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
