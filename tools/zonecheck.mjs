// The Zones tool and roaming creatures, driven in a headless Chrome: creatures come out of
// the sample map's treeline and roam; the debug view; a new zone painted, given a
// creature, and linked to a treeline by dragging; the download (the map, and the spawns
// record apart). Screenshots go to screenshots/zones-*.png.
//     node tools/zonecheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
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
const browser = await Browser.launch({ width: 1400, height: 900, port: 9485, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(200);
};
let failed = 0;
const check = (what, ok, detail = '') => {
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
};
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(100);
  return (await data('point')).split(',').map(Number);
};
const press = async (name) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  await browser.click(x, y);
};
const shot = async (name) => {
  await sleep(300);
  await browser.screenshot(join(shots, `zones-${name}.png`));
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await cmd('view:22,20,38,0,60');
  await browser.click(700, 889);
  const zones0 = Number(await data('zones'));
  const links0 = Number(await data('links'));

  // 1. creatures come out of the treeline and roam
  await sleep(9000);
  const roamers = Number(await data('roamers'));
  const roaming = await data('roaming');
  check('creatures roam', roamers > 0, `${roamers}: ${roaming}`);
  const free = roaming.split(';').filter(Boolean).some((r) => {
    const [x, z] = r.split(':')[1].split(',').map(Number);
    return Math.abs((x % 1) - 0.5) > 0.02 || Math.abs((z % 1) - 0.5) > 0.02;
  });
  check('they move freely, off the grid', free);
  await shot('1-roaming');
  await press('uiDebug');
  await shot('2-debug');

  // 2. a new zone, painted, with a creature in it
  await cmd('view:10,24,28,0,62');
  await browser.key('6');
  await press('uiPaintZones');
  await browser.drag([await at(3.5, 0, 24.5), await at(5.5, 0, 24.5), await at(7.5, 0, 24.5), await at(8.5, 0, 24.5)]);
  await browser.drag([await at(3.5, 0, 25.5), await at(5.5, 0, 25.5), await at(7.5, 0, 25.5), await at(8.5, 0, 25.5)]);
  check('a new zone', Number(await data('zones')) === zones0 + 1, `zones = ${await data('zones')}`);
  await press('uiCreatureFilter');
  await browser.type('a');
  await sleep(200);
  await press('uiAddFirst');
  check('a creature lives there', Number(await data('pool')) === 1, `pool = ${await data('pool')}`);

  // 3. linked to the south treeline by dragging
  await press('uiLinkMode');
  await browser.drag([await at(6.5, 0, 28.5), await at(6.5, 0, 27.5), await at(6.5, 0, 26.5), await at(6.0, 0, 25.5)]);
  check('a new link', Number(await data('links')) === links0 + 1, `links = ${await data('links')}`);
  await sleep(6000);
  await shot('3-new-link');

  // 4. the download: the map, and who lives where apart
  await browser.click(525, 17);
  await sleep(1500);
  const map = JSON.parse(readFileSync(join(downloads, 'test_meadow.json'), 'utf8'));
  check('the map keeps zones and links', map.zones.length === zones0 + 1 && map.links.length === links0 + 1 && map.zone_cells.length === map.height,
    `${map.zones.length} zones, ${map.links.length} links`);
  check('no creature named in the map', !JSON.stringify(map).includes('"creature"') && !map.spawns);
  const spawns = JSON.parse(readFileSync(join(downloads, 'test_meadow.spawns.json'), 'utf8'));
  check('the spawns record apart', Object.keys(spawns.zones).length === zones0 + 1, Object.keys(spawns.zones).join(','));
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
