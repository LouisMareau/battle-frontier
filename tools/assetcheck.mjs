// The asset browser, driven in a headless Chrome: opens it, shows a folder, drags a prop's
// row onto another folder (it's in both), makes a prop unstackable, drops a .glb from
// "Blender" on a folder (it becomes a prop there, its model loads, the Place tool puts it
// down), refuses an unstackable prop on another, and saves (downloads) what changed.
// Screenshots go to screenshots/assets-*.png.
//     node tools/assetcheck.mjs
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads');
const work = join(root, 'build', 'assetcheck');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
// a model "from Blender": the well, under another name
const model = join(work, 'Garden Lamp.glb');
copyFileSync(join(root, 'content', 'assets', 'models', 'stone_well.glb'), model);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9489, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
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
const expect = async (what, key, want) => {
  const got = await data(key);
  check(what, String(got) === String(want), `${key} = ${got}${String(got) === String(want) ? '' : ` (wanted ${want})`}`);
};
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
  await sleep(150);
};
const shown = async () => (await data('shown')).split(',').filter(Boolean);
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `assets-${name}.png`));
};
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(100);
  return (await data('point')).split(',').map(Number);
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  await cmd('view:20,12,16,0,55');

  // 1. open, every prop listed, a folder shown
  await press('uiAssets');
  await sleep(300);
  const all = Number(await data('assets'));
  check('the browser lists every prop', all > 30 && (await shown()).length === all, `${all} props, ${(await shown()).length} shown`);
  await press('uiFolder_Buildings_Inside');
  await expect('a folder shown', 'folder', 'Buildings/Inside');
  const inside = await shown();
  check('its props', inside.includes('bed') && !inside.includes('flowers'), inside.join(' '));
  await shot('1-inside');

  // 2. flowers dragged onto Buildings/Inside: in both folders now
  await press('uiFolder_Nature');
  check('flowers are Nature', (await shown()).includes('flowers'));
  const [fx, fy] = await ui('uiAsset_flowers');
  const [tx, ty] = await ui('uiFolder_Buildings_Inside');
  await browser.drag([[fx, fy], [fx + 10, fy - 4], [fx - 60, fy - 20], [(fx + tx) / 2, (fy + ty) / 2], [tx, ty + 2], [tx, ty]]);
  await sleep(300);
  await press('uiFolder_Buildings_Inside');
  check('flowers dragged in', (await shown()).includes('flowers'), (await shown()).join(' '));
  await press('uiFolder_Nature');
  check('and still in Nature', (await shown()).includes('flowers'));
  await expect('one prop changed', 'assetsdirty', 1);

  // 3. flowers made unstackable
  await press('uiUnstackable_flowers');
  await expect('still one prop changed', 'assetsdirty', 1);
  await shot('2-dragged');

  // 4. a .glb dropped on the Buildings/Outside folder: a prop, there, its model loading
  await press('uiFolder_Buildings_Outside');
  const models0 = Number(await data('models'));
  const [ox, oy] = await ui('uiFolder_Buildings_Outside');
  await browser.drop([model], ox, oy);
  await browser.waitFor('document.body.dataset.dropped === "Garden Lamp.glb"', { what: 'the dropped file read' });
  await sleep(500);
  await expect('one more prop', 'assets', all + 1);
  check('in Buildings/Outside', (await shown()).includes('garden_lamp'), (await shown()).join(' '));
  await browser.waitFor(`Number(document.body.dataset.models) > ${models0}`, { what: 'its model loaded' });
  check('its model loaded', true, `${await data('models')} models`);
  await shot('3-dropped');

  // 5. the Place tool puts it down (it was picked when it came in)
  await browser.click(700, 889);
  await browser.key('3');
  const props0 = Number(await data('props'));
  await browser.click(...(await at(18.0, 0, 12.0)));
  await expect('placed on the map', 'props', props0 + 1);
  // a barrel on the crate at 16.5, 8.5: neither stacks
  await press('uiAsset_barrel');
  await browser.click(...(await at(16.5, 0, 8.5)));
  await expect('a barrel on the crate: refused', 'props', props0 + 1);
  check('  and why', (await data('status')).startsWith('No room'), await data('status'));
  await shot('4-placed');

  // 5b. the placed lamp's record from the Select inspector: a light, and blocking off
  await browser.key('1');
  await browser.click(...(await at(18.0, 0.5, 12.0)));
  await sleep(300);
  check('the placed prop selected', (await data('selection')).startsWith('prop:'), await data('selection'));
  await press('uiPropLight');
  await expect('its light window', 'lightedit', 'garden_lamp');
  await press('uiLightGive');
  await sleep(200);
  await expect('two props changed', 'assetsdirty', 2);
  await press('uiPropBlocks');
  await sleep(200);
  await shot('5-prop');
  await browser.key('Escape');

  // 6. saved: the map, the two changed props' records and the model downloaded
  await browser.key('s', { ctrl: true });
  const got = (name) => existsSync(join(downloads, name));
  const wanted = ['test_meadow.json', 'flowers.json', 'garden_lamp.json', 'garden_lamp.glb'];
  for (let i = 0; i < 50 && !wanted.every(got); i++) await sleep(200);
  check('downloads', got('test_meadow.json') && got('flowers.json') && got('garden_lamp.json') && got('garden_lamp.glb'),
    ['test_meadow.json', 'flowers.json', 'garden_lamp.json', 'garden_lamp.glb'].map((n) => `${n}: ${got(n) ? 'yes' : 'no'}`).join(', '));
  if (got('flowers.json')) {
    const flowers = JSON.parse(readFileSync(join(downloads, 'flowers.json'), 'utf8'));
    check('flowers: unstackable, both folders', flowers.unstackable === true && flowers.folders?.includes('Buildings/Inside') && flowers.folders?.includes('Nature'), JSON.stringify(flowers.folders));
  }
  if (got('garden_lamp.json')) {
    const lamp = JSON.parse(readFileSync(join(downloads, 'garden_lamp.json'), 'utf8'));
    check('the new prop: measured, its model, its folder', lamp.footprint?.join('x') === '2x2' && lamp.model === 'models/garden_lamp.glb' && lamp.folders?.[0] === 'Buildings/Outside',
      `${lamp.footprint} · ${lamp.height} high · ${lamp.model} · ${lamp.folders}`);
    check('  with the light and the blocking set from the inspector', lamp.light?.kind === 'spot' && lamp.blocks !== true, `light ${JSON.stringify(lamp.light)} · blocks ${lamp.blocks}`);
  }
  await expect('nothing left to save', 'assetsdirty', 0);

  // 7. every prop to Blender, in one file
  await press('uiExportProps');
  for (let i = 0; i < 50 && !got('props.glb'); i++) await sleep(200);
  const catalogue = got('props.glb') ? readFileSync(join(downloads, 'props.glb')) : Buffer.alloc(0);
  check('every prop exported for Blender', catalogue.length > 10000 && catalogue.subarray(0, 4).toString() === 'glTF', `${catalogue.length} bytes`);
  check('  with the lamps\' lights', catalogue.includes('KHR_lights_punctual') && catalogue.includes('lamp_post light'));
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
