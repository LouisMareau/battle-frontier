// Grass grown through tall ground drawn as copies of its model, in the built site's map editor:
// a made-up map (64 x 64 of tall grass on level 0, a plateau a level up in its middle, a strip
// of short grass along its west edge) whose tall grass grows a made-up model of 2,000 blades a
// cell (16,000 vertices, painted for the wind, each blade a piece of its own: thinned further
// off), tiled. Looked at from three distances: copies drawn (some out of sight), culled on the
// GPU; what they cost in vertices within what the picture can show (DENSITY vertices a pixel:
// far fewer than every copy as made), the finest levels up close and none of them far off; the
// frame rate
// (and the engine's own blades' on the same map, for the record); no error. Screenshots to
// screenshots/grass-*.png. Exits 1 when something fails.
//     node tools/grasscheck.mjs
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

// a made-up grass model: `blades` blades over a cell, each its own piece (a strip of three
// segments, 8 vertices, narrowing to its tip, leaning a little its own way, darker at its foot),
// painted for the wind (swaying from its foot, a phase a blade), as a .glb
const BLADES = 2000;
function grassGlb(blades) {
  const pos = [];
  const nor = [];
  const col = [];
  const wind = [];
  const idx = [];
  let seed = 7;
  const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
  const linear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  for (let b = 0; b < blades; b++) {
    const [x, z] = [rnd() * 0.94 - 0.47, rnd() * 0.94 - 0.47];
    const [tall, wide, way, lean] = [0.45 + 0.35 * rnd(), 0.012 + 0.008 * rnd(), rnd() * Math.PI * 2, 0.03 * rnd()];
    const [cx, cz] = [Math.cos(way), Math.sin(way)];
    const base = pos.length / 3;
    for (let r = 0; r <= 3; r++) {
      const t = r / 3;
      const half = wide * (1 - 0.85 * t);
      const [px, pz] = [x + cx * lean * t * t, z + cz * lean * t * t];
      for (const side of [-1, 1]) {
        pos.push(px - side * half * cz, tall * t, pz + side * half * cx);
        const n = Math.hypot(cx, 0.3, cz);
        nor.push(cx / n, 0.3 / n, cz / n);
        col.push(0.05 + 0.12 * t, 0.16 + 0.36 * t, 0.04 + 0.05 * t, 1);
        wind.push(linear(t), linear(0.4 * t), linear((b % 7) / 7), 1);
      }
    }
    for (let r = 0; r < 3; r++) {
      const k = base + r * 2;
      idx.push(k, k + 1, k + 3, k, k + 3, k + 2);
    }
  }
  const n = pos.length / 3;
  const parts = [new Float32Array(pos), new Float32Array(nor), new Float32Array(col), new Float32Array(wind), new Uint32Array(idx)].map((a) => Buffer.from(a.buffer));
  let offset = 0;
  const bufferViews = parts.map((p, k) => {
    const v = { buffer: 0, byteOffset: offset, byteLength: p.length, target: k === 4 ? 34963 : 34962 };
    offset += p.length;
    return v;
  });
  const lo = [0, 1, 2].map((a) => Math.min(...pos.filter((_, i) => i % 3 === a)));
  const hi = [0, 1, 2].map((a) => Math.max(...pos.filter((_, i) => i % 3 === a)));
  const gltf = {
    asset: { version: '2.0', generator: 'grasscheck' },
    scene: 0,
    scenes: [{ nodes: [0] }],
    nodes: [{ mesh: 0, name: 'grass' }],
    materials: [{ name: 'grass', pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 1], metallicFactor: 0, roughnessFactor: 1 } }],
    meshes: [{ name: 'grass', extras: { bf_wind: 1 }, primitives: [{ attributes: { POSITION: 0, NORMAL: 1, COLOR_0: 2, COLOR_1: 3 }, indices: 4, material: 0 }] }],
    buffers: [{ byteLength: offset }],
    bufferViews,
    accessors: [
      { bufferView: 0, componentType: 5126, count: n, type: 'VEC3', min: lo, max: hi },
      { bufferView: 1, componentType: 5126, count: n, type: 'VEC3' },
      { bufferView: 2, componentType: 5126, count: n, type: 'VEC4' },
      { bufferView: 3, componentType: 5126, count: n, type: 'VEC4' },
      { bufferView: 4, componentType: 5125, count: idx.length, type: 'SCALAR' },
    ],
  };
  const pad = (b, fill) => Buffer.concat([b, Buffer.alloc((4 - (b.length % 4)) % 4, fill)]);
  const json = pad(Buffer.from(JSON.stringify(gltf)), 0x20);
  const bin = pad(Buffer.concat(parts), 0);
  const chunk = (type, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32LE(data.length, 0);
    head.writeUInt32LE(type, 4);
    return Buffer.concat([head, data]);
  };
  const body = Buffer.concat([chunk(0x4e4f534a, json), chunk(0x004e4942, bin)]);
  const head = Buffer.alloc(12);
  head.writeUInt32LE(0x46546c67, 0);
  head.writeUInt32LE(2, 4);
  head.writeUInt32LE(12 + body.length, 8);
  return { glb: Buffer.concat([head, body]), vertices: n };
}
const model = grassGlb(BLADES);

// the maps: 64 x 64, tall grass but a strip of short grass along x 0 to 2, a plateau a level
// up from 24 to 40 each way; the second the same of tall ground growing nothing (the engine's
// own blades)
const S = 64;
const field = (id, tall) => {
  const terrain = [];
  const levels = [];
  for (let z = 0; z < S; z++) {
    let t = '';
    let l = '';
    for (let x = 0; x < S; x++) {
      t += x < 3 ? 'g' : 't';
      l += x >= 24 && x < 40 && z >= 24 && z < 40 ? '1' : '0';
    }
    terrain.push(t);
    levels.push(l);
  }
  return { id, name: id, width: S, height: S, legend: { g: 'grass', t: tall }, terrain, levels };
};
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/grass_check.json'] = field('grass_check', 'tall_grass');
  bundle.files['maps/grass_plain.json'] = field('grass_plain', 'tall_plain');
  bundle.files['props/grass_check.json'] = { id: 'grass_check', name: 'Check grass', category: 'grass', footprint: [1, 1], height: 0.8, parts: [], model: 'models/grass_check.glb', toon: true };
  const tall = bundle.files['terrains/tall_grass.json'];
  bundle.files['terrains/tall_plain.json'] = { ...tall, id: 'tall_plain', name: 'Tall plain' };
  tall.growth = { props: [{ prop: 'grass_check', weight: 1 }], density: 4, scale: [0.8, 1.25], patchy: 0.35, turn: true, tiled: true };
  // the colours as they are: no cloud shadows drifting over, no colour steps
  const world = bundle.files['settings/world.json'];
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
  return Buffer.from(JSON.stringify(bundle));
};
const handle = (req, res, path) => {
  if (path !== 'content/assets/models/grass_check.glb') return false;
  res.writeHead(200, { 'content-type': 'model/gltf-binary', 'cache-control': 'no-store' });
  res.end(model.glb);
  return true;
};

const server = await serve(0, { rewrite, handle });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9519, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
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
const shot = (name) => browser.screenshot(join(shots, `grass-${name}.png`));
// a look from `view` (x, z, distance, yaw, pitch): once the squares are in and a second's
// stats are, what the grass costs, its copies, the frame rate
const look = async (view, name) => {
  await cmd(`view:${view}`);
  await sleep(3500);
  const out = { copies: await num('grass'), vertices: await num('grassverts'), fps: await num('fps'), levels: (await data('grasslevels')).split(',').map(Number) };
  await shot(name);
  console.log(`  ${name} (${view}): ${out.copies} copies, ${out.vertices} vertices, ${out.fps} fps; by level ${out.levels.join(' ')}`);
  return out;
};

try {
  await browser.viewport(1400, 900);
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:grass_check');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "grass_check"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await cmd('view:32,32,24,0,50');
  await browser.waitFor('Number(document.body.dataset.grass ?? 0) > 0', { timeout: 60000, what: 'grass copies drawn' });
  check('culled on the GPU (WebGPU)', (await data('grassgpu')) === '1', await data('grassgpu'));
  const mid = await look('32,32,24,0,50', 'mid');
  const near = await look('20,52,8,0,40', 'near');
  const far = await look('32,32,70,0,55', 'far');
  // the made-up model as made, a copy a cell: 4,096 copies of 16,000 vertices
  const each = model.vertices;
  check('copies drawn, some out of sight', mid.copies > 500 && mid.copies < S * (S - 3), `${mid.copies} of ${S * (S - 3)}`);
  // (about DENSITY 1.5 vertices a pixel of the picture: 1.9 million at most, give or take)
  for (const [name, at] of [['mid', mid], ['near', near], ['far', far]]) {
    check(`  ${name}: within what the picture shows, far fewer vertices than every copy as made`, at.vertices > 0 && at.vertices < 1400 * 900 * 2.5 && at.vertices < at.copies * each / 3,
      `${at.vertices} drawn against ${at.copies * each} as made`);
  }
  // the finest level drawn (0 as made); of the copies in sight (those just outside it, casting
  // their shadows in, are at the thinnest: the last level drawn), the share as made
  const finest = (at) => at.levels.findIndex((n) => n > 0);
  const made = (at) => at.levels[0] / Math.max(1, at.copies - at.levels[at.levels.findLastIndex((n) => n > 0)]);
  check('  up close fewer copies, nearly all in sight as made', near.copies < mid.copies && made(near) >= 0.8 && made(near) > made(mid),
    `${near.copies} copies, ${(made(near) * 100).toFixed(0)}% of those in sight as made (${(made(mid) * 100).toFixed(0)}% further off)`);
  check('  far off more copies, none at the finest levels', far.copies > mid.copies && finest(far) > finest(mid),
    `${far.copies} copies, the finest at level ${finest(far)}`);
  check('  the frame keeps up', mid.fps >= 20 && far.fps >= 20, `${mid.fps} and ${far.fps} fps`);
  // the engine's own blades on the same map, for the record (from the menu again)
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu again' });
  await cmd('map:grass_plain');
  await browser.waitFor('document.body.dataset.map === "grass_plain"', { timeout: 60000, what: 'the plain map' });
  await sleep(1500);
  const plain = await look('32,32,24,0,50', 'blades');
  check('  the engine\'s own blades where nothing grows: no copies', plain.copies === 0, `${plain.copies} copies, ${plain.fps} fps (copies: ${mid.fps} fps)`);
  check('no error in the page', browser.errors.length === 0, browser.errors.slice(0, 3).join(' | '));
} catch (e) {
  check('ran to its end', false, e.message);
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
