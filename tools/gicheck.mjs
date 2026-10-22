// The indirect light, in the built site's map editor: a made-up map at night with a lava band
// along its west side, a plateau two levels up east of it, and a patch of arcane moss (its
// veins and dots glowing, its meadow's glowbells too) further east. The ground beside the
// lava is lit by it (warm), the ground beside the moss by the moss (cold); the plateau's far
// foot, in its shadow from the lava, is darker than open ground as far from the lava; with
// the indirect light turned off (`indirect:off`, then the Post-processing window's Light
// slider) the ground beside the lava goes dark; the frame keeps up. With --webgl, the page
// without WebGPU: the editor reaches the map, the lava still shines, nothing lit by it (no
// compute shaders there), no error. Screenshots to screenshots/gi-*.png (gi-webgl-*.png).
// Exits 1 when something fails.
//     node tools/gicheck.mjs [--webgl]
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

// the map: 40 x 30; the lava in its six westmost columns, a plateau two levels up from x 7 to
// 10 and z 8 to 16 (a cell off the lava), the moss from x 26 to 32 and z 18 to 26, grass
// elsewhere
const [W, H] = [40, 30];
const terrain = [];
const levels = [];
for (let z = 0; z < H; z++) {
  let t = '';
  let l = '';
  for (let x = 0; x < W; x++) {
    t += x < 6 ? 'l' : x >= 26 && x < 32 && z >= 18 && z < 26 ? 'm' : 'g';
    l += x >= 7 && x < 10 && z >= 8 && z < 16 ? '2' : '0';
  }
  terrain.push(t);
  levels.push(l);
}
const map = { id: 'gi_check', name: 'Indirect light check', width: W, height: H, legend: { g: 'grass', l: 'lava_check', m: 'moss_check' }, terrain, levels };
const own = (id, name, colors, look, walk = 'walk') => ({ id, name, order: 20, color: colors[0], color2: colors[1], side: colors[2], pattern: 'custom', walk, look });
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/gi_check.json'] = map;
  bundle.files['terrains/lava_check.json'] = own('lava_check', 'Lava check', ['#ff5a10', '#ffb030', '#2a2220'], { scale: 1.5, sharpness: 0.4, grain: 0.1, glow: 2.2, pulse: 0.0, layers: [{ shape: 'stones', color: '#2b2320', scale: 1.1, amount: 0.5, softness: 0.25, relief: 0.7 }] }, 'block');
  const moss = own('moss_check', 'Moss check', ['#241a3e', '#34245c', '#2a2236'], { scale: 2, grain: 0.25, layers: [{ shape: 'patches', color: '#2c4a3e', scale: 1.5, amount: 0.35, softness: 0.4 }, { shape: 'veins', color: '#5ef2ff', scale: 3, amount: 0.35, softness: 0.3, glow: 2.2 }, { shape: 'dots', color: '#d8a8ff', scale: 0.45, amount: 0.1, softness: 0.2, glow: 2 }] });
  moss.growth = {
    meadow: {
      plants: [
        { name: 'Dusk grass', height: [0.12, 0.25], width: 0.03, color: '#2a1f45', tip: '#6a50c0', glow: 0.4 },
        { name: 'Glowbell', shape: 'flower', weight: 0.06, height: [0.18, 0.3], width: 0.012, lean: 0.1, bend: 0.15, sway: 0.45, petals: 5, petal: '#8af0ff', heart: '#ffffff', bloom: 0.08, clusters: 0.6, glow: 2.5 },
      ],
      density: 36,
    },
  };
  bundle.files['terrains/moss_check.json'] = moss;
  // the colours as they are: no cloud shadows drifting over, no colour steps; the light as
  // the defaults have it
  const world = bundle.files['settings/world.json'];
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
  delete world.light;
  if (process.env.GI_BLOOM !== undefined) world.light = { bloom: Number(process.env.GI_BLOOM) };
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9533, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
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
const tag = (name) => `gi-${webgl ? 'webgl-' : ''}${name}`;
// where a point of the ground lands on the screen
const at = async (x, z) => {
  await cmd(`point:${x},0.05,${z}`);
  await sleep(150);
  return (await data('point')).split(',').map(Number);
};
// the brightness (linear, out of 255) and the colour (the mean rgb) of the ground round a
// point, in the screenshot `shot`
const linear = (v) => Math.pow(v / 255, 2.2) * 255;
const ground = (shot, [px, py]) => {
  if (!(px > 8 && py > 8 && px < shot.width - 8 && py < shot.height - 8)) return { bright: NaN, rgb: [0, 0, 0] };
  let sum = 0;
  const rgb = [0, 0, 0];
  let n = 0;
  for (let y = py - 6; y <= py + 6; y++) {
    for (let x = px - 6; x <= px + 6; x++) {
      const i = (y * shot.width + x) * 4;
      sum += linear(shot.rgba[i]) * 0.2126 + linear(shot.rgba[i + 1]) * 0.7152 + linear(shot.rgba[i + 2]) * 0.0722;
      for (let k = 0; k < 3; k++) rgb[k] += shot.rgba[i + k];
      n++;
    }
  }
  return { bright: sum / n, rgb: rgb.map((v) => Math.round(v / n)) };
};
// a look from `view` (x, z, distance, yaw, pitch), settled, screenshot `name`, and the ground
// at each of `points` (x, z) in it
const look = async (view, name, points) => {
  await cmd(`view:${view}`);
  await sleep(2500);
  const spots = [];
  for (const [x, z] of points) spots.push(await at(x, z));
  await sleep(300);
  const shot = await browser.pixels(join(shots, `${tag(name)}.png`));
  const out = points.map((p, k) => ({ at: p, ...ground(shot, spots[k]) }));
  console.log(`  ${name} (${view}): ${out.map((o) => `(${o.at}) ${o.bright.toFixed(1)} #${o.rgb.map((v) => v.toString(16).padStart(2, '0')).join('')}`).join(', ')}`);
  return out;
};

try {
  await browser.viewport(1400, 900);
  if (webgl) await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: "Object.defineProperty(Navigator.prototype, 'gpu', { get: () => undefined, configurable: true });" });
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:gi_check');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "gi_check"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  // night, still: no roaming creatures, no grid lines
  await cmd('period:23:30');
  check("the editor's time at 23:30", (await data('hours')) === '23:30', await data('hours'));
  await press('uiCreatures');
  await browser.key('g');
  await sleep(1500);

  // 1. the lava and the plateau: the ground beside the lava (four cells off it, clear of the
  // bloom) lit by it, warm; the plateau's far foot in its shadow darker than open ground as far
  // from the lava (north of the plateau)
  const west = await look('12,9,48,0,55', 'lava', [[10.5, 3.5], [11.5, 2.5], [11.5, 12.5], [3, 10], [24.5, 2.5]]);
  const [beside, open, foot, lava, off] = west;
  // (what the lava adds: over the night's own light on the grass far from it)
  const added = (p) => p.bright - off.bright;
  check('the lava shines at night', lava.bright > 60 && lava.rgb[0] > 180 && lava.rgb[0] > lava.rgb[2] * 2, `${lava.bright.toFixed(1)} #${lava.rgb.join(',')}`);
  if (!webgl) {
    check('the ground beside the lava is lit by it, warm', added(beside) > 4 && beside.rgb[0] > beside.rgb[2], `${beside.bright.toFixed(1)} beside against ${off.bright.toFixed(1)} further off; #${beside.rgb.join(',')}`);
    check("  the plateau's far foot, in its shadow, darker than open ground as far from the lava", added(open) > 1.5 && added(foot) < added(open) * 0.5, `${foot.bright.toFixed(1)} against ${open.bright.toFixed(1)}, ${off.bright.toFixed(1)} far off`);
  } else {
    check('nothing lit by the lava without compute shaders (WebGL 2)', added(beside) < 2, `${beside.bright.toFixed(1)} beside, ${off.bright.toFixed(1)} further off`);
  }
  const fps = Number(await data('fps'));
  check('the frame keeps up', fps >= 20, `${fps} fps`);

  // 2. the moss: the ground beside it lit by it, cold
  const east = await look('30,21,40,0,55', 'moss', [[23, 22.5], [39.5, 12.5], [29, 22]]);
  const [byMoss, farGrass, moss] = east;
  check('the moss shines at night', moss.bright > 10 && moss.rgb[2] > moss.rgb[0], `${moss.bright.toFixed(1)} #${moss.rgb.join(',')}`);
  if (!webgl) {
    check('the ground beside the moss is lit by it, cold', byMoss.bright > farGrass.bright + 4 && byMoss.rgb[2] + byMoss.rgb[1] > byMoss.rgb[0] * 2, `${byMoss.bright.toFixed(1)} beside against ${farGrass.bright.toFixed(1)} far off; #${byMoss.rgb.join(',')}`);

    // 3. turned off, the ground beside the lava goes dark: the Options' way, then the
    // Post-processing window's Light slider at its left end (and Defaults bring it back)
    await cmd('indirect:off');
    const [dark] = await look('12,9,48,0,55', 'off', [[10.5, 3.5]]);
    check('with the indirect light off (indirect:off) the ground beside the lava is dark', added(dark) < added(beside) * 0.3 + 0.5, `${dark.bright.toFixed(1)} against ${beside.bright.toFixed(1)}`);
    await cmd('indirect:on');
    await press('uiPost');
    const light = await data('light');
    check("the Post-processing window's Light: exposure, tone map, bloom, strength, reach", light === '1.00 soft 0.35 1.00 32', light);
    const [gx, gy, , gh] = (await data('uiPostGiRect')).split(',').map(Number);
    await browser.click(gx + 2, gy + gh / 2);
    await sleep(250);
    check('  the Indirect light slider at its left end turns it off', (await data('light')).split(' ')[3] === '0.00' && (await data('postdirty')) === '1', await data('light'));
    const [slid] = await look('12,9,48,0,55', 'slider', [[10.5, 3.5]]);
    check('  and the ground beside the lava is dark', added(slid) < added(beside) * 0.3 + 0.5, `${slid.bright.toFixed(1)} against ${beside.bright.toFixed(1)}`);
    await press('uiPostDefaults');
    check('  Defaults bring it back', (await data('light')) === '1.00 soft 0.35 1.00 32', await data('light'));
    await press('uiPost');
    const [back] = await look('12,9,48,0,55', 'back', [[10.5, 3.5]]);
    check('  lit again', added(back) > 4, `${back.bright.toFixed(1)} against ${off.bright.toFixed(1)} far off`);
  }
  check('no error in the page', browser.errors.length === 0, browser.errors.slice(0, 3).join(' | '));
} catch (e) {
  check('ran to its end', false, e.message);
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
