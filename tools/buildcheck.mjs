// The Build tool, driven with the mouse and keys in a headless Chrome: draws an L-shaped
// footprint, puts a double door and a window on the ground floor, adds a floor and, Inside,
// a bed (a table on it is refused: neither stacks), a table placed then deleted, a lamp on
// an inner wall, stairs (up to the flat roof, and from the ground floor up through floor 2,
// where nothing may stand over them), a pillar; confirms; edits it again and, Outside, picks
// a wall, hangs a flower box on it and puts a barrel in front of it (not further away);
// tries stairs in the cottage (refused under its gabled roof, allowed once it's flat); walks
// in through the door (and into walls), turns and moves the building, undoes, and downloads
// the map.
// Screenshots go to screenshots/build-*.png.
//     node tools/buildcheck.mjs
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
const browser = await Browser.launch({ width: 1400, height: 900, port: 9483, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
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
const expect = async (what, key, want) => {
  const got = await data(key);
  check(what, String(got) === String(want), `${key} = ${got}${String(got) === String(want) ? '' : ` (wanted ${want})`}`);
};
// where a world point is on the screen
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(100);
  return (await data('point')).split(',').map(Number);
};
// a widget the editor marked
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
};
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `build-${name}.png`));
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  await cmd('view:22.5,18.5,15,0,58');
  await browser.click(700, 889); // the status bar: the canvas gets the keyboard
  const buildings0 = Number(await data('buildings'));

  // 1. the footprint: an L
  await browser.key('5');
  await expect('the Build tool', 'tool', 'build');
  await browser.drag([await at(21.5, 0, 17.5), await at(22.5, 0, 17.5), await at(23.5, 0, 17.5)]);
  await browser.drag([await at(21.5, 0, 18.5), await at(22.5, 0, 18.5), await at(23.5, 0, 18.5)]);
  const foot = await at(21.5, 0, 19.5);
  await browser.click(...foot);
  await expect('drawing the footprint', 'build', 'footprint');
  await shot('1-footprint');
  await press('uiNext');
  await expect('designing it', 'build', 'design');

  // 2. a double door on the south wall, a window on the west
  await browser.drag([await at(22.5, 0, 19.0), await at(23.0, 0, 19.0), await at(23.5, 0, 19.0)]);
  await press('uiPut');
  await expect('a double door', 'draft', 'floors:1;floor:1;openings:1;inside:0;pillars:0');
  await press('uiWindows');
  await browser.click(...(await at(21.0, 0, 19.5)));
  await press('uiPut');
  await expect('a window', 'draft', 'floors:1;floor:1;openings:2;inside:0;pillars:0');
  await shot('2-ground-floor');

  // 3. a second floor: Inside, a bed; a table on the bed refused; a table placed then
  // deleted; a lamp on an inner wall; then a pillar on a corner (Structure)
  await press('uiAddFloor');
  await press('uiInside');
  await press('uiPick_bed');
  await browser.click(...(await at(22.5, 2.0, 17.6)));
  await expect('a bed inside', 'draft', 'floors:2;floor:2;openings:0;inside:1;pillars:0');
  await shot('3-inside');
  await press('uiPick_table');
  await browser.click(...(await at(22.5, 2.0, 17.42)));
  await expect('a table on the bed: refused', 'draft', 'floors:2;floor:2;openings:0;inside:1;pillars:0');
  check('  and why', (await data('status')).startsWith('No room'), await data('status'));
  await browser.click(...(await at(21.5, 2.0, 22.5)));
  await expect('a table outside the building: refused', 'draft', 'floors:2;floor:2;openings:0;inside:1;pillars:0');
  await browser.click(...(await at(23.5, 2.0, 17.5)));
  await expect('a table next to it', 'draft', 'floors:2;floor:2;openings:0;inside:2;pillars:0');
  await browser.key('Delete');
  await expect('the table deleted', 'draft', 'floors:2;floor:2;openings:0;inside:1;pillars:0');
  await browser.click(...(await at(22.5, 2.0, 18.0)));
  check('the bed selected', (await data('rooms')).endsWith('sel:Some(Floor(0))'), await data('rooms'));
  await browser.key('Escape');
  await press('uiPick_wall_lamp');
  await browser.click(...(await at(22.5, 3.3, 17.12)));
  check('a lamp on the inner north wall', (await data('rooms')).includes('inner:1;outer:0'), await data('rooms'));
  // stairs on the top floor: an L has a flat roof, so they come up to it (an opening)
  await press('uiPick_stairs');
  await browser.click(...(await at(21.5, 2.0, 17.6)));
  await expect('stairs on the top floor', 'draft', 'floors:2;floor:2;openings:0;inside:2;pillars:0');
  check('  the roof open over them', (await data('rooms')).includes('roof:2'), await data('rooms'));
  // stairs on the ground floor open floor 2 over them, and nothing goes there
  await press('uiFloor1');
  await browser.click(...(await at(23.5, 0, 17.6)));
  await expect('stairs on the ground floor', 'draft', 'floors:2;floor:1;openings:2;inside:1;pillars:0');
  await press('uiFloor2');
  check('floor 2 open over them', (await data('rooms')).includes('holes:2'), await data('rooms'));
  await press('uiPick_table');
  await browser.click(...(await at(23.5, 2.0, 17.5)));
  await expect('a table over the stairwell: refused', 'draft', 'floors:2;floor:2;openings:0;inside:2;pillars:0');
  check('  and why', (await data('status')).startsWith("That's open"), await data('status'));
  await press('uiStructure');
  await press('uiPillars');
  await browser.click(...(await at(21.0, 2.0, 17.0)));
  await expect('the second floor', 'draft', 'floors:2;floor:2;openings:0;inside:2;pillars:1');
  await shot('4-second-floor');

  // 4. confirmed: on the map
  await press('uiConfirm');
  await expect('confirmed', 'build', '');
  await expect('one more building', 'buildings', buildings0 + 1);
  await cmd('view:22.5,18.5,15,-35,40');
  await shot('5-confirmed');
  await cmd('view:22.5,18.5,15,0,58');

  // 5. edited again, Outside: a wall first, a flower box on it, a barrel in front of it
  // (one further away refused)
  await browser.key('1');
  await browser.click(...(await at(22.5, 1.0, 18.0)));
  await expect('selected', 'selection', `building:${buildings0}`);
  await press('uiEditBuilding');
  await expect('editing it', 'build', 'design');
  await press('uiOutside');
  await browser.click(...(await at(21.5, 0, 20.0)));
  check('a wall picked (the foot of the L, south)', (await data('rooms')).startsWith('wall:0,2,s;'), await data('rooms'));
  await press('uiPick_flower_box');
  await browser.click(...(await at(21.5, 1.4, 20.0)));
  check('a flower box on it', (await data('rooms')).includes('inner:1;outer:1;outside:0'), await data('rooms'));
  await press('uiPick_barrel');
  await browser.click(...(await at(21.5, 0, 20.5)));
  check('a barrel in front of it', (await data('rooms')).includes('outside:1'), await data('rooms'));
  await browser.click(...(await at(21.5, 0, 22.5)));
  check('one further away: refused', (await data('rooms')).includes('outside:1') && (await data('status')).startsWith('Outside, in front'), `${await data('rooms')} · ${await data('status')}`);
  await shot('6-outside');
  await press('uiConfirm');
  await expect('confirmed again', 'build', '');
  await expect('decorations', 'anchors', 2);
  await browser.key('1');

  // 6. the cottage has a gabled roof: stairs on its only floor are refused until it's flat
  await cmd('view:11,10.5,14,0,58');
  await browser.click(...(await at(11.0, 1.0, 10.5)));
  await expect('the cottage selected', 'selection', 'building:0');
  await press('uiEditBuilding');
  await press('uiInside');
  await press('uiPick_stairs');
  await browser.click(...(await at(9.5, 0, 10.6)));
  check('refused under its gabled roof', (await data('status')).startsWith('Stairs on the top floor need a flat roof'), await data('status'));
  await press('uiStructure');
  await press('uiFlatRoof');
  await press('uiInside');
  await browser.click(...(await at(9.5, 0, 10.6)));
  check('allowed once the roof is flat', (await data('rooms')).includes('roof:2'), await data('rooms'));
  await shot('7-stairs-to-the-roof');
  await press('uiCancelBuilding');
  await expect('the cottage left as it was', 'build', '');
  await expect('  and selected, in Select', 'tool', 'select');
  await cmd('view:22.5,18.5,15,0,58');
  await browser.click(...(await at(22.5, 1.0, 18.0)));
  await expect('the new building selected again', 'selection', `building:${buildings0}`);

  // 6. on foot: in through the door, not through walls
  await press('uiWalk');
  const start = await data('walker');
  check('walking', start !== '', `from ${start}`);
  const step = async (key, want, what) => {
    await browser.key(key);
    await sleep(320);
    await expect(what, 'walker', want);
  };
  await step('ArrowDown', '22,19', 'out through the door');
  await step('ArrowLeft', '22,19', 'the wall stops it');
  await step('ArrowUp', '22,18', 'back in');
  await step('ArrowUp', '22,17', 'inside');
  await shot('8-walking-inside');
  await step('ArrowUp', '22,17', 'the north wall stops it');
  await browser.key('Escape');

  // 7. turned, moved, and the move undone
  const cells = await data('bcells');
  await browser.key('e');
  check('turned', (await data('bcells')) !== cells, `${cells} -> ${await data('bcells')}`);
  const before = await data('bat');
  await browser.drag([await at(22.5, 1.0, 18.0), await at(23.0, 0, 18.0), await at(24.0, 0, 18.0), await at(24.5, 0, 18.0)]);
  const moved = await data('bat');
  check('moved', moved !== before, `${before} -> ${moved}`);
  await shot('9-moved');
  await browser.key('z', { ctrl: true });
  await expect('the move undone', 'bat', before);

  // 8. the download has it
  await press('uiWalk'); // walking again then stopping: no effect on the map
  await browser.key('Escape');
  await press('uiSave');
  await sleep(1500);
  const saved = JSON.parse(readFileSync(join(downloads, 'test_meadow.json'), 'utf8'));
  const b = saved.buildings?.[buildings0];
  check('downloaded', !!b && b.floors.length === 2 && b.floors[0].openings.length === 2 && b.pillars.length === 1 && b.anchors.length === 2
    && b.anchors.filter((a) => a.inside).length === 1 && b.outside?.length === 1 && b.floors[1].props.length === 2 && b.floors[0].props.length === 1,
    b ? `${b.id}: ${b.floors.length} floors, ${b.floors[0].openings.length} openings, ${b.pillars.length} pillar, ${b.anchors.length} on walls, ${b.outside?.length ?? 0} outside` : 'no building');
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
