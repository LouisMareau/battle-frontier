// Templates, props on their cells and stairs, in the built site in a real browser: captures
// an area of the test meadow (five props, Bo, the well's cells) as a template, drags it
// onto the south-west meadow (Bo comes back as bo_2, his script's cells moved along), undoes
// and redoes it; drags the lamp post to other cells and back (undo), then, with Tweak,
// within its cell only; drags the well (it fills its cells: no Tweak) to other cells and
// back; walks up the corner shop's stairs and back down; and finds all of it in the
// downloaded map and template. Exits 1 when something fails.
//     node tools/templatecheck.mjs
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-templates');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9497, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
const cmd = (c) => browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
  await sleep(250);
};
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(120);
  return (await data('point')).split(',').map(Number);
};
const expect = async (what, key, want) => {
  for (let i = 0; i < 30 && String(await data(key)) !== String(want); i++) await sleep(100);
  const got = await data(key);
  check(what, String(got) === String(want), `${key} = ${got}${String(got) === String(want) ? '' : ` (wanted ${want})`}`);
};
const shot = (name) => browser.screenshot(join(shots, `templates-${name}.png`));

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  // saving downloads each file here, as this check reads them (the browser keeps them otherwise)
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html?saves=downloads`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1500);
  // the asset browser closed (it comes open, along the bottom): the view as these steps knew it
  await browser.waitFor('(document.body.dataset.layout ?? "") !== ""', { what: 'the panels laid out' });
  if ((await data('layout')).includes('assets=bottom')) {
    await press('uiAssets');
    await sleep(400);
  }
  await cmd('view:20,15,52,0,60');
  await sleep(600);
  await browser.key('1');
  await expect('the Select tool', 'tool', 'select');

  // 1. an area captured as a template, named first
  const templates0 = Number(await data('templates'));
  await press('uiTemplateName');
  await browser.type('Town square');
  await press('uiCapture');
  await expect('capturing', 'capturing', '1');
  await browser.drag([await at(10.5, 0, 4.5), await at(14, 0, 7), await at(20, 0, 9), await at(23.5, 0, 10.5)]);
  await expect('a new template', 'templates', templates0 + 1);
  await expect('  named after its name', 'tplnew', 'town_square');
  check('  what it holds', (await data('status')).includes('5 props, 1 actor, 1 event'), await data('status'));
  await shot('1-captured');

  // 2. dragged onto the south-west meadow: everything in it, as it was
  const props0 = Number(await data('props'));
  const actors0 = Number(await data('actors'));
  const events0 = Number(await data('events'));
  const [tx, ty] = await ui('uiTemplate_town_square');
  const [mx, my] = await at(9.5, 0, 26.5);
  await browser.drag([[tx, ty], [tx + 20, ty], [(tx + mx) / 2, (ty + my) / 2], [mx - 10, my], [mx, my]]);
  await expect('put down there', 'stamped', 'town_square@2,23');
  await expect('  five more props', 'props', props0 + 5);
  await expect('  one more actor', 'actors', actors0 + 1);
  await expect('  his event and the well\'s', 'events', events0 + 2);
  await shot('2-stamped');
  await browser.key('z', { ctrl: true });
  await expect('undone', 'props', props0);
  await browser.key('y', { ctrl: true });
  await expect('and redone', 'props', props0 + 5);

  // 3. the lamp post dragged two cells east, and back (undo); with Tweak, a drag moves it
  // within its cell only
  await browser.click(...(await at(14.5, 1.2, 4.5)));
  await sleep(300);
  check('the lamp post selected', (await data('selprop')).startsWith('14.50,4.50'), await data('selprop'));
  const east = async (x) => browser.drag([await at(x, 0, 4.5), await at(x + 0.5, 0, 4.5), await at(x + 1, 0, 4.5), await at(x + 2, 0, 4.5)]);
  await east(14.5);
  check('  dragged to other cells', (await data('selprop')).startsWith('16.50,4.50'), await data('selprop'));
  await browser.key('z', { ctrl: true });
  check('  back (undone)', (await data('selprop')).startsWith('14.50,4.50'), await data('selprop'));
  await press('uiTweak');
  await expect('  Tweak on', 'tweak', '1');
  await east(14.5);
  const [cell, off] = (await data('selprop')).split(';');
  const ox = Number(off.split(',')[0]);
  check('  still on its cell', cell === '14.50,4.50', cell);
  check('  moved within it, up to its edge', ox > 0.2 && ox < 0.5, off);
  await shot('3-nudged');
  // the well fills its two-by-two cells: no Tweak; a drag moves it to others
  await browser.click(...(await at(12.0, 1.0, 6.0)));
  await sleep(300);
  check('the well selected', (await data('selprop')).startsWith('12.00,6.00'), await data('selprop'));
  await expect('  Tweak off for it', 'tweak', '');
  await browser.drag([await at(12.0, 0, 6.0), await at(13.0, 0, 6.0), await at(14.0, 0, 6.0)]);
  check('  dragged to other cells', (await data('selprop')) === '14.00,6.00;0.00,0.00', await data('selprop'));
  await browser.key('z', { ctrl: true });
  check('  back (undone)', (await data('selprop')) === '12.00,6.00;0.00,0.00', await data('selprop'));
  await browser.key('Escape');

  // 4. up the corner shop's stairs and back down
  await press('uiWalk');
  await cmd('walkto:33,24');
  await sleep(400);
  check('in front of the stairs', (await data('walker')) === '33,24' && (await data('inside')) === 'corner_shop', `${await data('walker')} in ${await data('inside')}`);
  await browser.key('ArrowRight');
  await sleep(900);
  await expect('up on the first floor', 'floor', 'corner_shop:1');
  await expect('  past the top step', 'walker', '36,24');
  await shot('4-upstairs');
  await browser.key('ArrowLeft');
  await sleep(900);
  await expect('back down', 'floor', '');
  await expect('  at the bottom', 'walker', '33,24');
  await browser.key('Escape');

  // 5. downloaded: the map with its copies, the template itself
  await browser.key('s', { ctrl: true });
  const got = (n) => existsSync(join(downloads, n));
  for (let i = 0; i < 50 && !(got('test_meadow.json') && got('town_square.json')); i++) await sleep(200);
  check('the template downloaded', got('town_square.json'));
  if (got('town_square.json')) {
    const t = JSON.parse(readFileSync(join(downloads, 'town_square.json'), 'utf8'));
    check('  from its corner, as it was', t.size?.join('x') === '14x7' && t.props?.length === 5 && t.actors?.[0]?.at?.join(',') === '12,5' && t.events?.[0]?.area?.join(',') === '0,3,4,2',
      `${t.size} · ${t.props?.length} props · bo at ${t.actors?.[0]?.at} · well cells ${t.events?.[0]?.area}`);
  }
  if (got('test_meadow.json')) {
    const m = JSON.parse(readFileSync(join(downloads, 'test_meadow.json'), 'utf8'));
    const bo2 = m.actors?.find((a) => a.id === 'bo_2');
    check('the copy of Bo, his talk moved along', bo2?.at?.join(',') === '14,28' && bo2?.events?.[0]?.id === 'bo_talk_2' && bo2?.events?.[0]?.script?.includes('camera 4,25'),
      JSON.stringify(bo2 && { at: bo2.at, event: bo2.events?.[0]?.id, script: bo2.events?.[0]?.script }));
    const lamp = m.props?.find((p) => p.prop === 'lamp_post' && p.at[0] === 14.5 && p.at[1] === 4.5);
    check('the lamp post\'s offset saved, its cell kept', lamp && lamp.offset?.[0] > 0.2, JSON.stringify(lamp));
  }
  console.log('fps', await data('fps'));
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
