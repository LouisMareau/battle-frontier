// The World view and walking across maps, in a headless Chrome: opens the test meadow,
// shows the world it's in (with the field east of it, a map kept as chunk files), drags
// that piece away and back (the seam goes and comes back), saves (downloads) the world;
// finds the field drawn east of the meadow (held around it), walks out through the gap in
// the meadow's east woods into it at once (nothing to load, its squares there already)
// and back; and double-clicks the field to edit it, the view staying where it was.
// Screenshots go to screenshots/world-*.png.
//     node tools/worldcheck.mjs
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
const browser = await Browser.launch({ width: 1400, height: 900, port: 9491, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
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
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `world-${name}.png`));
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  // saving downloads each file here, as this check reads them (the browser keeps them otherwise)
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html?saves=downloads`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  // the asset browser closed (it comes open, along the bottom): the view as these steps knew it
  await browser.waitFor('(document.body.dataset.layout ?? "") !== ""', { what: 'the panels laid out' });
  if ((await data('layout')).includes('assets=bottom')) {
    await press('uiAssets');
    await sleep(400);
  }
  await browser.click(700, 889); // the status bar: the canvas gets the keys

  // 1. the World view shows the world this map is in
  await press('uiWorld');
  await sleep(600);
  await expect('the world shown', 'world', 'test_world;pieces:2;seams:1;mismatches:3');
  await expect('its pieces', 'pieces', 'test_meadow@0,0;east_field@40,0');
  await shot('1-world');

  // 2. the field dragged 8 cells east (no seam while apart), then back
  const zoom = Number(await data('worldzoom'));
  const [px, py] = await ui('uiPiece_east_field');
  await browser.drag([[px, py], [px + 3, py], [px + zoom * 4, py], [px + zoom * 8, py]]);
  await expect('the field moved east', 'pieces', 'test_meadow@0,0;east_field@48,0');
  check('no seam while apart', (await data('world')).includes('seams:0'), await data('world'));
  const [qx, qy] = await ui('uiPiece_east_field');
  await browser.drag([[qx, qy], [qx - 3, qy], [qx - zoom * 4, qy], [qx - zoom * 8, qy]]);
  await expect('and back', 'pieces', 'test_meadow@0,0;east_field@40,0');
  await expect('the seam again, 3 cells of it a step', 'world', 'test_world;pieces:2;seams:1;mismatches:3');

  // 3. saved: the download holds both pieces
  await press('uiSaveWorld');
  for (let i = 0; i < 40 && !existsSync(join(downloads, 'test_world.json')); i++) await sleep(200);
  const saved = existsSync(join(downloads, 'test_world.json')) ? JSON.parse(readFileSync(join(downloads, 'test_world.json'), 'utf8')) : null;
  check('the world downloaded', saved?.pieces?.length === 2 && saved.pieces[1].map === 'east_field' && saved.pieces[1].at.join(',') === '40,0', JSON.stringify(saved?.pieces));
  await press('uiWorld');

  // 4. the field drawn east of the meadow: held around it, some of its squares built
  await cmd('view:38,14.5,14,0,58');
  await browser.waitFor('document.body.dataset.around === "east_field"', { what: 'the field held around the meadow' });
  await browser.waitFor('Number((document.body.dataset.aroundsquares || "0/1").split("/")[0]) > 0', { what: 'some of its squares built' });
  await shot('2-around');

  // 5. on foot through the gap in the east woods, into the field at once, and back
  await press('uiWalk');
  await expect('walking from the gap', 'walker', '38,14');
  await browser.key('ArrowRight');
  await sleep(320);
  await expect("to the meadow's edge", 'walker', '39,14');
  await browser.key('ArrowRight');
  await sleep(120);
  await expect('across the seam, at once', 'map', 'east_field');
  await expect('  nothing to load', 'loading', '0');
  check('  its squares there already', !(await data('squares')).startsWith('0/'), await data('squares'));
  await expect('  the meadow held around it now', 'around', 'test_meadow');
  await sleep(300);
  await expect('  where it stepped', 'walker', '0,14');
  await expect('a map kept as chunk files', 'chunked', 1);
  await shot('3-across');
  await browser.key('ArrowRight');
  await sleep(320);
  await expect('on along the path', 'walker', '1,14');
  await browser.key('ArrowLeft');
  await sleep(320);
  await browser.key('ArrowLeft');
  await sleep(120);
  await expect('back in the meadow, at once', 'map', 'test_meadow');
  await sleep(300);
  await expect('back in the meadow', 'walker', '39,14');
  await browser.key('Escape');

  // 6. a double-click on the field: it's the map edited now, the view where it was
  await cmd('point:41.5,0,12.5');
  await sleep(150);
  const [fx, fy] = (await data('point')).split(',').map(Number);
  await browser.dblclick(fx, fy);
  await sleep(300);
  await expect('the field edited after a double-click', 'map', 'east_field');
  await expect('  the meadow around it', 'around', 'test_meadow');
  await shot('4-editing-the-field');
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
