// The toon look, driven in a headless Chrome: the well, the barrel and the crate ticked toon
// in the asset browser get ink lines round them (pixels of the ink's colour on the screen
// where there were none); the Post-processing window turns the lines off and on, makes them
// wider, puts the whole picture in colour steps, and saves the look (a download of the world
// settings); a creature ticked toon under Creatures; the asset browser's Save downloads the
// records, toon. Screenshots go to screenshots/post-*.png.
//     node tools/postcheck.mjs
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9495, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(200);
};
let failed = 0;
const check = (what, ok, detail = '') => {
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
};
const press = async (name) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  await browser.click(x, y);
  await sleep(250);
};
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(150);
  return (await data('point')).split(',').map(Number);
};
// ink: pixels of about the ink's colour (#2b2633), in boxes around points of the world
const INK = [0x2b, 0x26, 0x33];
let last = null;
const ink = async (name, boxes) => {
  await sleep(700);
  const shot = await browser.pixels(join(shots, `post-${name}.png`));
  last = shot;
  let n = 0;
  for (const [cx, cy, r] of boxes) {
    for (let y = Math.max(0, cy - r); y < Math.min(shot.height, cy + r); y++) {
      for (let x = Math.max(0, cx - r); x < Math.min(shot.width, cx + r); x++) {
        const i = (y * shot.width + x) * 4;
        if (INK.every((v, k) => Math.abs(shot.rgba[i + k] - v) < 22)) n++;
      }
    }
  }
  return n;
};
// how different two screenshots are in a box: the mean difference of a channel
const differ = (a, b, [cx, cy, r]) => {
  let sum = 0, n = 0;
  for (let y = cy - r; y < cy + r; y++) {
    for (let x = cx - r; x < cx + r; x++) {
      const i = (y * a.width + x) * 4;
      for (let k = 0; k < 3; k++) sum += Math.abs(a.rgba[i + k] - b.rgba[i + k]);
      n += 3;
    }
  }
  return sum / n;
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  // still: no roaming creatures, no flag flapping, no grid lines
  await press('uiCreatures');
  await press('uiAnimate');
  await browser.key('g');
  await cmd('view:14,9,13,25,40');
  await sleep(1500);
  const well = await at(12.0, 0.6, 6.0);
  const crates = await at(16.0, 0.4, 8.5);
  const boxes = [[well[0], well[1], 70], [crates[0], crates[1], 60]];
  const before = await ink('1-plain', boxes);

  // 1. the well, the barrel and the crate made toon in the asset browser
  await press('uiAssets');
  await sleep(300);
  for (const id of ['stone_well', 'barrel', 'crate']) {
    // scrolled into the list's sight
    let [x, y] = (await data(`uiToon_${id}`)).split(',').map(Number);
    for (let i = 0; i < 30 && !(y > 650 && y < 850); i++) {
      await browser.wheel(700, 760, Math.max(-120, Math.min(120, y - 750)));
      await sleep(120);
      [x, y] = (await data(`uiToon_${id}`)).split(',').map(Number);
    }
    check(`the toon toggle of ${id} in sight`, y > 650 && y < 850, `${x},${y}`);
    await browser.click(x, y);
    await sleep(200);
  }
  check('three props changed', (await data('assetsdirty')) === '3', await data('assetsdirty'));
  await press('uiAssets');
  const toon = await ink('2-toon', boxes);
  check('ink lines round them', toon > before + 150, `${before} ink pixels before, ${toon} now`);

  // 2. the Post-processing window: lines off, back on, wider
  await press('uiPost');
  check('the window opens', (await data('post')) === 'open');
  await press('uiPostInk');
  check('  a change waits to be saved', (await data('postdirty')) === '1');
  await press('uiPost');
  const off = await ink('3-ink-off', boxes);
  check('  lines off: gone', off < toon * 0.3, `${off} ink pixels`);
  await press('uiPost');
  await press('uiPostInk');
  const width0 = await data('toon');
  await press('uiPostWidth');
  const width1 = await data('toon');
  check('  wider lines', width0 !== width1 && width1.includes('ink'), `${width0} -> ${width1}`);
  await press('uiPost');
  const wide = await ink('4-wide', boxes);
  check('  more ink', wide > toon * 1.3, `${toon} -> ${wide} ink pixels`);
  // the whole picture in colour steps (the ground too, nothing toon there)
  const smooth = last;
  await press('uiPost');
  await press('uiPostSteps');
  check('  colour steps', /[2-9]\d* colour steps|1\d colour steps/.test(await data('toon')), await data('toon'));
  await press('uiPost');
  await ink('5-steps', boxes);
  // grass in front, under the view's middle
  const changed = differ(smooth, last, [700, 760, 40]);
  check('  the ground in steps', changed > 1.5, `${changed.toFixed(2)} levels apart on average`);

  // 3. a creature ticked toon under Creatures (the first listed)
  await press('uiAssets');
  await press('uiFolder_Creatures');
  const shown = (await data('shown')).split(',').filter(Boolean);
  check('the creatures listed', shown.length > 0, `${shown.length}`);
  if (shown.length) {
    await press(`uiToon_${shown[0]}`);
    check('  one creature changed', (await data('creaturesdirty')) === '1', await data('creaturesdirty'));
  }
  await press('uiAssets');

  // 4. saved: the props' and the creature's records, and the look
  await press('uiPost');
  await press('uiPostSave');
  await press('uiPost');
  check('the look saved', (await data('postdirty')) === '', await data('status'));
  await browser.key('s', { ctrl: true });
  const got = (name) => existsSync(join(downloads, name));
  const wanted = ['world.json', 'stone_well.json', 'barrel.json', 'crate.json', ...(shown.length ? [`${shown[0]}.json`] : [])];
  for (let i = 0; i < 50 && !wanted.every(got); i++) await sleep(200);
  check('downloads', wanted.every(got), wanted.map((n) => `${n}: ${got(n) ? 'yes' : 'no'}`).join(', '));
  if (got('world.json')) {
    const w = JSON.parse(readFileSync(join(downloads, 'world.json'), 'utf8'));
    check('  the world settings, with the look', w.id === 'world' && w.toon?.ink === true && w.toon?.ink_width > 2 && w.toon?.color_steps >= 2, JSON.stringify(w.toon));
  }
  if (got('stone_well.json')) {
    check('  the well, toon', JSON.parse(readFileSync(join(downloads, 'stone_well.json'), 'utf8')).toon === true);
  }
  if (shown.length && got(`${shown[0]}.json`)) {
    check('  the creature, toon', JSON.parse(readFileSync(join(downloads, `${shown[0]}.json`), 'utf8')).toon === true);
  }
  await press('uiAssets');
  check('nothing left to save', (await data('assetsdirty')) === '0' && (await data('creaturesdirty')) === '0', `${await data('assetsdirty')} props, ${await data('creaturesdirty')} creatures`);
  console.log('fps', await data('fps'), 'worst frame', await data('maxgap'), 'ms');
  if (browser.errors.length) {
    failed++;
    console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  }
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
