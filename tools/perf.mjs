// How fast the game draws, with the frame rate uncapped (no vsync, no 60 FPS limit) in a
// headless Chrome on the machine's GPU: frames per second, the time each frame keeps the
// page busy (the game's work, and waiting on the GPU when it's behind) per frame in the menu, the map editor (close, mid and whole-map views; at half resolution,
// without shadows, with sharper ones, without the creatures), walking, and a battle; and
// what was drawn (draw calls, triangles, squares of the world left out of sight). Uncapped, frames per second is what the machine
// could do, not the screen's rate: 120 means room for a 120 Hz screen.
//
//     trunk build --release && node tools/perf.mjs [map=test_meadow] [tile=1] [seconds=4] [size=1400x900] [only=NAME]
// tile=N plays a map made of N x N copies of `map` (props, buildings and all), to see how
// the drawing holds up on big maps; it's added to the served bundle, nothing is written. profile=NAME also samples where the
// processor's time goes in the scenes whose names contain NAME (the 25 functions taking most).
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { pick } from './encounters.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const opt = Object.fromEntries(process.argv.slice(2).filter((a) => a.includes('=')).map((a) => a.split('=')));
const source = opt.map ?? 'test_meadow';
const tile = Number(opt.tile ?? 1);
const map = tile > 1 ? 'perf_stress' : source;
const seconds = Number(opt.seconds ?? 4);
const [width, height] = (opt.size ?? '1400x900').split('x').map(Number);
const uncapped = ['--disable-gpu-vsync', '--disable-frame-rate-limit'];
const gpu = ['--ignore-gpu-blocklist', '--enable-gpu-rasterization', '--use-angle=d3d11', '--enable-unsafe-webgpu'];

/// `m` copied n x n times.
function tiled(m, n) {
  const [w, h] = [m.width, m.height];
  const out = { ...m, id: 'perf_stress', name: `Stress test: ${m.name}, ${n} x ${n}`, width: w * n, height: h * n, props: [], buildings: [], links: [] };
  for (const key of ['terrain', 'levels', 'areas', 'zone_cells']) {
    if (m[key]) out[key] = Array.from({ length: h * n }, (_, z) => m[key][z % h].repeat(n));
  }
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      const [dx, dz] = [i * w, j * h];
      for (const p of m.props ?? []) out.props.push({ ...p, at: [p.at[0] + dx, p.at[1] + dz] });
      for (const b of m.buildings ?? []) out.buildings.push({ ...b, id: `${b.id}_${i}_${j}`, at: [b.at[0] + dx, b.at[1] + dz] });
      for (const l of m.links ?? []) out.links.push({ ...l, treeline: l.treeline.map(([x, z]) => [x + dx, z + dz]) });
    }
  }
  return out;
}
const rewrite = (path, body) => {
  if (tile <= 1 || path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  const m = bundle.files[`maps/${source}.json`];
  if (!m) throw new Error(`no map ${source} in the content`);
  bundle.files['maps/perf_stress.json'] = tiled(m, tile);
  return Buffer.from(JSON.stringify(bundle));
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width, height, port: 9483, profiles: join(root, 'build', 'chrome'), args: [...gpu, ...uncapped] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = (c) => browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
const press = async (name) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  await browser.click(x, y);
};
const results = [];
const edits = [];
// where the time goes: Chrome's sampling profiler, self time by function
const profile = async (name) => {
  await browser.send('Profiler.enable');
  await browser.send('Profiler.setSamplingInterval', { interval: 100 });
  await browser.send('Profiler.start');
  await sleep(3000);
  const { profile: p } = await browser.send('Profiler.stop');
  const byId = new Map(p.nodes.map((n) => [n.id, n]));
  const self = new Map();
  for (const id of p.samples) {
    const f = byId.get(id).callFrame;
    const key = f.functionName || `(anonymous ${f.url.split('/').pop()}:${f.lineNumber})`;
    self.set(key, (self.get(key) ?? 0) + 1);
  }
  console.log(`profile of ${name} (${p.samples.length} samples):`);
  for (const [k, n] of [...self].sort((a, b) => b[1] - a[1]).slice(0, 25)) console.log(`  ${((100 * n) / p.samples.length).toFixed(1).padStart(5)}%  ${k}`);
};
// the median of `seconds` one-second windows (the first one after a change is skipped)
const measure = async (name) => {
  if (opt.only && !name.includes(opt.only)) return;
  await sleep(1500);
  const fps = [], cpu = [], peak = [], gap = [];
  for (let i = 0; i < seconds; i++) {
    await sleep(1000);
    fps.push(Number(await data('fps')));
    cpu.push(Number(await data('cpu')));
    peak.push(Number(await data('cpupeak')));
    gap.push(Number(await data('maxgap')));
  }
  const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
  if (opt.profile && name.includes(opt.profile)) await profile(name);
  const row = {
    scene: name, fps: median(fps), frameMs: +(1000 / median(fps)).toFixed(2), busyMs: median(cpu), busyWorstMs: Math.max(...peak), worstGapMs: Math.max(...gap),
    draws: Number(await data('draws')), triangles: Number(await data('tris')), shadowTriangles: Number(await data('shadowtris')), culled: Number(await data('culled')),
  };
  results.push(row);
  console.log(JSON.stringify(row));
};

try {
  await browser.viewport(width, height);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  console.log('graphics:', await data('backend'), `· ${width}x${height}`, tile > 1 ? `· ${source} x ${tile * tile}` : `· ${map}`);
  await measure('menu');

  await cmd(`time:day;map:${map}`);
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1500);
  const [w, h] = (await data('mapsize') || '40x30').split('x').map(Number);
  const [cx, cz] = [w / 2, h / 2];
  await cmd(`view:${cx},${cz},14,30,40`);
  await measure('maps: close');
  await cmd(`view:${cx},${cz},35,30,40`);
  await measure('maps: mid');
  await cmd(`view:${cx},${cz},${Math.max(w, h) * 1.6},30,50`);
  await measure('maps: whole map');
  await cmd('scale:50');
  await measure('maps: whole map, 50% resolution');
  await cmd('scale:100;shadows:off');
  await measure('maps: whole map, no shadows');
  await cmd('shadows:high');
  await measure('maps: whole map, high shadows');
  await cmd('shadows:medium');
  await press('uiCreatures');
  await measure('maps: whole map, no creatures');
  await press('uiCreatures');
  // editing: props placed one after the other (each rebuilds what changed): the worst frame
  if (!opt.only || 'editing'.includes(opt.only)) {
    await cmd(`view:${cx},${cz},22,30,45`);
    await sleep(1000);
    await browser.click(700, 889); // the status bar: the canvas gets the keys
    await browser.key('3');
    await sleep(500);
    const gaps = [];
    for (let k = 0; k < 6; k++) {
      await browser.click(560 + k * 40, 480);
      await sleep(1100);
      gaps.push(Number(await data('maxgap')));
    }
    const row = { scene: 'editing: a prop placed', worstGapMs: Math.max(...gaps), perPlacementMs: gaps };
    edits.push(row);
    console.log(JSON.stringify(row));
    await browser.key('z', { ctrl: true });
    await browser.key('1');
  }
  await cmd(`view:${cx},${cz},22,30,45`); // walking: the camera close, as when playing
  await press('uiWalk');
  await measure('walking');
  await press('uiWalk');

  const encounter = pick('trainer', { size: 3 }) ?? pick('trainer');
  if (encounter) {
    await cmd('back');
    await browser.waitFor('document.body.dataset.state === "menu"', { what: 'the menu' });
    await cmd(`auto:1;text:fast;time:day;start:${encounter}`);
    await browser.waitFor('document.body.dataset.state === "battle"', { timeout: 60000, what: 'the battle' });
    await measure('battle');
  }
  if (results.length) {
    const slowest = results.reduce((a, b) => (b.fps < a.fps ? b : a), results[0]);
    console.log(`slowest: ${slowest.scene} at ${slowest.fps} fps (${slowest.frameMs} ms a frame; ${slowest.fps >= 120 ? 'room for 120 Hz' : slowest.fps >= 60 ? 'room for 60 Hz' : 'under 60'})`);
  }
  for (const e of edits) console.log(`${e.scene}: worst frame ${e.worstGapMs} ms (${e.worstGapMs <= 8.3 ? 'no hitch at 120 Hz' : e.worstGapMs <= 16.7 ? 'no hitch at 60 Hz' : 'a visible hitch'})`);
} finally {
  if (browser.errors.length) console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  await browser.close();
  server.close();
}
