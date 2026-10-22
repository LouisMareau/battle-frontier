// Terrains of their own, in the built site's map editor: Tall grass duplicated (its group then
// folding two), the copy opened in the Terrain window and closed; a new kind made from the
// Volcanic rock preset and painted on the map (a right-click on what's painted picks it); the
// Paint tab's brush pinned at the foot of the Toolbox. A made-up map in bands (plain grass, then
// three terrains of their own look: cracks glowing orange, veins glowing cyan, a lava lake; and
// grass to paint on, out of sight of the bands) looked at at noon and at night: at night what
// glows of itself still shines (bright and warm, cyan) where the plain grass is dark. No error.
// Screenshots to screenshots/terrain-*.png. Exits 1 when something fails.
//     node tools/terraincheck.mjs
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

// the map: 48 x 44, its first 32 rows in bands of 12 cells (plain grass, glowing cracks,
// glowing veins, lava), then grass to paint on
const [W, H] = [48, 44];
const terrain = [];
for (let z = 0; z < H; z++) terrain.push(z < 32 ? 'gggggggggggg' + 'vvvvvvvvvvvv' + 'aaaaaaaaaaaa' + 'llllllllllll' : 'g'.repeat(W));
const map = { id: 'terrain_check', name: 'Terrain check', width: W, height: H, legend: { g: 'grass', v: 'cracks_check', a: 'veins_check', l: 'lava_check' }, terrain };
const own = (id, name, colors, look, walk = 'walk') => ({ id, name, order: 20, color: colors[0], color2: colors[1], side: colors[2], pattern: 'custom', walk, look });
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/terrain_check.json'] = map;
  bundle.files['terrains/cracks_check.json'] = own('cracks_check', 'Cracks check', ['#2a2422', '#3a302c', '#2a2220'], { scale: 2.5, sharpness: 0.55, grain: 0.35, layers: [{ shape: 'cracks', color: '#ff5a14', scale: 1.6, amount: 0.3, softness: 0.35, glow: 2.6, pulse: 0.35, relief: -0.6 }] });
  bundle.files['terrains/veins_check.json'] = own('veins_check', 'Veins check', ['#241a3e', '#34245c', '#2a2236'], { scale: 2, grain: 0.25, layers: [{ shape: 'veins', color: '#5ef2ff', scale: 3, amount: 0.35, softness: 0.3, glow: 2.2 }, { shape: 'dots', color: '#d8a8ff', scale: 0.45, amount: 0.1, softness: 0.2, glow: 2 }] });
  bundle.files['terrains/lava_check.json'] = own('lava_check', 'Lava check', ['#ff5a10', '#ffb030', '#2a2220'], { scale: 1.5, sharpness: 0.4, grain: 0.1, glow: 2.2, pulse: 0.25, flow: 0.08, angle: 30, layers: [{ shape: 'stones', color: '#2b2320', scale: 1.1, amount: 0.62, softness: 0.25, flow: 0.08, angle: 30, relief: 0.7 }] }, 'block');
  const world = bundle.files['settings/world.json'];
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9527, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(300);
};
const press = async (name, wait = 400) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(wait);
};
// the pixels of the view's middle (clear of the panels) that shine as `bright` says
const count = async (name, bright) => {
  const { width, rgba } = await browser.pixels(join(shots, `terrain-${name}.png`));
  let n = 0;
  for (let y = 250; y < 650; y += 2) {
    for (let x = 520; x < 1000; x += 2) {
      const i = (y * width + x) * 4;
      if (bright(rgba[i], rgba[i + 1], rgba[i + 2])) n++;
    }
  }
  return n;
};
// (the glow of lava and cracks runs from orange to yellow where it's brightest)
const warm = (r, g, b) => r > 200 && b < r * 0.55;
const cyan = (r, g, b) => b > 150 && g > 120 && r < 140;

try {
  await browser.viewport(1400, 900);
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:terrain_check');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "terrain_check"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  await browser.waitFor('(document.body.dataset.layout ?? "") !== ""', { what: 'the panels laid out' });
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await browser.click(700, 889); // the status bar: gives the canvas the keyboard
  await browser.key('2');
  await browser.waitFor('document.body.dataset.tool === "ground"', { what: 'the Ground tool' });

  // a variant of Tall grass (its row's Duplicate): its group folds two, the copy edited (its
  // row's Edit) and closed
  await press('uiTerrainDuplicate_tall_grass', 800);
  check('Tall grass duplicated: its group of two', (await data('terraingroups')).includes('Tall grass:2') && (await data('terrain')) === 'tall_grass_2', `${await data('terraingroups')}; ${await data('terrain')} picked`);
  await press('uiTerrainEdit_tall_grass_2', 800);
  check('  the copy in the Terrain window', (await data('terrainwindow')) === 'tall_grass_2', await data('terrainwindow'));
  await browser.screenshot(join(shots, 'terrain-1-window.png'));
  await press('uiTerrainClose');
  check('  closed', (await data('terrainwindow')) === '', await data('terrainwindow'));

  // a new kind from a preset, painted on the grass past the bands (out of their views); a
  // right-click on it, another terrain picked, picks it again
  await press('uiTerrainNew', 600);
  await press('uiTerrainPreset_volcanic_rock', 800);
  check('a new kind from the Volcanic rock preset, in the window', (await data('terrainwindow')) === 'volcanic_rock', await data('terrainwindow'));
  await press('uiTerrainClose');
  check('  closed', (await data('terrainwindow')) === '', await data('terrainwindow'));
  const [, by] = (await data('uiBrush')).split(',').map(Number);
  check('the brush pinned at the foot of the Toolbox', by > 700, `its size at y ${by}`);
  await cmd('view:24,38,14,0,55');
  await sleep(1500);
  const [l, t, r, b] = (await data('viewrect')).split(',').map(Number);
  const [cx, cy] = [Math.round((l + r) / 2), Math.round((t + b) / 2)];
  await press('uiTerrain_volcanic_rock');
  await browser.drag([[cx - 90, cy - 25], [cx - 30, cy - 25], [cx + 30, cy - 25], [cx + 90, cy - 25], [cx + 90, cy + 25], [cx + 30, cy + 25], [cx - 30, cy + 25], [cx - 90, cy + 25]]);
  await sleep(1500);
  await browser.screenshot(join(shots, 'terrain-2-painted.png'));
  await press('uiTerrain_grass');
  await browser.click(cx, cy, 'right');
  await sleep(400);
  check('  painted: a right-click on it picks it', (await data('terrain')) === 'volcanic_rock', `${await data('terrain')} at ${await data('groundcell')}`);

  // the bands at noon and at night: what glows of itself shines at night where grass is dark
  const looks = [['grass', 6], ['cracks', 18], ['veins', 30], ['lava', 42]];
  const at = {};
  for (const time of ['12:00', '23:30']) {
    await cmd(`period:${time}`);
    check(`the editor's time at ${time}`, (await data('hours')) === time, await data('hours'));
    for (const [name, x] of looks) {
      await cmd(`view:${x},16,9,0,55`);
      await sleep(2000);
      const tag = `${name}-${time === '12:00' ? 'noon' : 'night'}`;
      at[tag] = { warm: await count(tag, warm), cyan: await count(tag, cyan) };
      console.log(`  ${tag}: ${at[tag].warm} warm, ${at[tag].cyan} cyan`);
    }
  }
  check('at night the cracks glow warm where the grass has none', at['cracks-night'].warm > 400 && at['grass-night'].warm < 20, `${at['cracks-night'].warm} against ${at['grass-night'].warm}`);
  check('  the veins cyan', at['veins-night'].cyan > 300 && at['grass-night'].cyan < 20, `${at['veins-night'].cyan} against ${at['grass-night'].cyan}`);
  check('  the lava as much as by day (lit by nothing)', at['lava-night'].warm > 400 && at['lava-night'].warm > 0.6 * at['lava-noon'].warm, `${at['lava-night'].warm} at night, ${at['lava-noon'].warm} at noon`);
  check('no error in the page', browser.errors.length === 0, browser.errors.slice(0, 3).join(' | '));
} catch (e) {
  check('ran to its end', false, e.message);
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
