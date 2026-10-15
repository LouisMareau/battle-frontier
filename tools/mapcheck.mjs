// The map editor, driven with the mouse and keys in a headless Chrome: paints a path,
// raises ground a level up, places and turns props, paints a tree area, moves a prop,
// undoes, and downloads the map. Screenshots of each step go to screenshots/maps-*.png.
//     node tools/mapcheck.mjs [map=test_meadow]
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const opt = Object.fromEntries(process.argv.slice(2).filter((a) => a.includes('=')).map((a) => a.split('=')));
const map = opt.map ?? 'test_meadow';
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9481, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
// a widget the editor marked, pressed
const press = async (name) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  await browser.click(x, y);
};
let failed = 0;
const expect = async (what, key, want) => {
  const got = await data(key);
  const ok = String(got) === String(want);
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}: ${key} = ${got}${ok ? '' : ` (wanted ${want})`}`);
};
const check = (what, ok, detail = '') => {
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
};
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `maps-${name}.png`));
};
try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await browser.evaluate(`(document.body.dataset.cmd = "map:${map}", true)`);
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1500);
  await browser.click(700, 889); // the status bar: gives the canvas the keyboard
  const props0 = Number(await data('props'));

  // a dirt path across the grass (Ground tool, terrain from the panel)
  await browser.key('2');
  await expect('tool 2', 'tool', 'ground');
  await press('uiTerrain_dirt');
  await browser.drag([[300, 705], [400, 705], [500, 705], [600, 705], [700, 705], [760, 705]]);
  await shot('1-paint');

  // one level up: a ridge
  await browser.key('PageUp');
  await expect('PageUp', 'level', 1);
  await press('uiTerrain_grass');
  await browser.drag([[1010, 470], [1060, 470], [1110, 470], [1160, 470], [1160, 500], [1010, 500]]);
  await shot('2-level');
  await browser.key('PageDown');
  await expect('PageDown', 'level', 0);

  // props: two, the second turned
  await browser.key('3');
  await expect('tool 3', 'tool', 'place');
  await browser.click(560, 420);
  await browser.key('e', { shift: true });
  await browser.click(600, 420);
  await expect('two props placed', 'props', props0 + 2);
  await shot('3-place');

  // a tree area (a new one: painting without one makes it)
  await browser.key('4');
  await expect('tool 4', 'tool', 'trees');
  const areas0 = Number(await data('areas'));
  await browser.drag([[300, 390], [330, 390], [360, 390], [390, 400], [390, 430], [300, 430]]);
  await expect('a new tree area', 'areas', areas0 + 1);
  await shot('4-trees');

  // select the first new prop and move it
  await browser.key('1');
  await expect('tool 1', 'tool', 'select');
  await browser.drag([[560, 415], [580, 420], [620, 440], [650, 450]]);
  await shot('5-move');

  // undo the move, then everything else up to the path
  await browser.key('z', { ctrl: true });
  await shot('6-undo');
  await expect('unsaved', 'dirty', 1);

  // night
  await press('uiNight');
  await sleep(300);
  check('the lamps lit by night', Number(await data('lights')) >= 3, `${await data('lights')} lamps`);
  await shot('7-night');

  // the download (the browser's save)
  await press('uiSave');
  await sleep(1500);
  const file = join(downloads, `${map}.json`);
  const saved = JSON.parse(readFileSync(file, 'utf8'));
  const ok = saved.id === map && saved.props.length === props0 + 2 && saved.tree_areas.length === areas0 + 1;
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} downloaded ${file}: ${saved.props.length} props, ${saved.tree_areas.length} tree areas, rows ${saved.terrain.length}`);
  await expect('saved', 'dirty', 0);
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
