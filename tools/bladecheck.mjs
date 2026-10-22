// A meadow's blades and flowers made on the GPU as they're drawn, in the built site's map
// editor: a made-up map (64 x 64 of tall ground growing a meadow, a plateau a level up in its
// middle, a strip of short grass along its west edge), its meadow a grass, daisies and poppies
// (in clusters). Looked at from four distances: blades drawn, culled on the GPU; up close the
// finest templates, far off the simplest; what they cost in vertices within what the picture
// can show; flowers among them; the frame rate; no error. With --webgl, the page without
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
  { name: 'Daisy', shape: 'flower', weight: 0.1, height: [0.18, 0.32], width: 0.012, lean: 0.1, bend: 0.15, sway: 0.8, petals: 8, petal: '#f6f3ea', heart: '#eab92a', bloom: 0.09 },
  { name: 'Poppy', shape: 'flower', weight: 0.08, height: [0.3, 0.5], width: 0.012, lean: 0.1, bend: 0.15, sway: 0.8, petals: 4, petal: '#d0301e', heart: '#2a1a12', bloom: 0.12, clusters: 0.6 },
];
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/meadow_check.json'] = field('meadow_check');
  const tall = bundle.files['terrains/tall_grass.json'];
  bundle.files['terrains/meadow_check.json'] = { ...tall, id: 'meadow_check', name: 'Meadow check', growth: { meadow: { plants, density: 48, patchy: 0.3, clump: 0.6, clumping: 0.5 } } };
  // the colours as they are: no cloud shadows drifting over, no colour steps
  const world = bundle.files['settings/world.json'];
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
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

try {
  await browser.viewport(1400, 900);
  if (webgl) await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: "Object.defineProperty(Navigator.prototype, 'gpu', { get: () => undefined, configurable: true });" });
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:meadow_check');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "meadow_check"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await cmd('view:32,32,24,0,50');
  await browser.waitFor('Number(document.body.dataset.blades ?? 0) > 0', { timeout: 60000, what: 'blades drawn' });
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
  check('no error in the page', browser.errors.length === 0, browser.errors.slice(0, 3).join(' | '));
} catch (e) {
  check('ran to its end', false, e.message);
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
