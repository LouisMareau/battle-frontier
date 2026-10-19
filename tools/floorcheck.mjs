// Floors laid by wave function collapse, in the built site's map editor, on a made-up map
// served in the bundle: a house of 4 by 3 cells drawn with the Build tool, its ground floor
// painted all over with a set of floor tiles (the flagstones: small stones, a slab, and a long
// slab over two cells, either way): every two cells side by side agree on the edge between
// them (no long slab's half without its other half); "Lay it again" lays them another way,
// still fitting; a slab put by hand, turned, stays as it was put when the rest is laid again
// round it (which fits it); then out of doors, the Ground tool's Floors tab laying the flagstones on the
// grass, every two cells agreeing again, one taken away; downloaded, the map has each cell's
// tile and its turn, the house's and the ground's.
// Screenshots to screenshots/floor-*.png. Exits 1 when something fails.
//     node tools/floorcheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-floors');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

const W = 20;
const H = 16;
const map = { id: 'floor_test', name: 'Floor test', width: W, height: H, legend: { g: 'grass' }, terrain: Array(H).fill('g'.repeat(W)) };
// the set's tiles as the bundle has them: each one's edges (north, east, south, west) and turns
const tiles = {};
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/floor_test.json'] = map;
  for (const [file, rec] of Object.entries(bundle.files)) {
    if (file.startsWith('props/') && rec.floor?.set === 'flagstones') tiles[rec.id] = rec.floor;
  }
  return Buffer.from(JSON.stringify(bundle));
};
const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9516, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(200);
};
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(100);
  return (await data('point')).split(',').map(Number);
};
const press = async (name) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(200);
};
const shot = async (name) => {
  await sleep(500);
  await browser.screenshot(join(shots, `floor-${name}.png`));
};
// "x,z:tile:turn;..." (the `buildlaid` hook) as cells, and whether every two side by side agree
const SIDES = [[0, -1], [1, 0], [0, 1], [-1, 0]];
const parse = (laid) => Object.fromEntries(laid.split(';').filter(Boolean).map((e) => {
  const [cell, tile, turn, hand] = e.split(':');
  return [cell, { tile, turn: Number(turn), hand: hand === 'hand' }];
}));
const saved = (tiles) => Object.fromEntries((tiles ?? []).map((t) => [`${t.cell[0]},${t.cell[1]}`, { tile: t.tile, turn: t.turn ?? 0, hand: !!t.by_hand }]));
const same = (a, b) => JSON.stringify(Object.keys(a).sort().map((c) => [c, a[c]])) === JSON.stringify(Object.keys(b).sort().map((c) => [c, b[c]]));
const edge = (l, side) => tiles[l.tile]?.edges[(side + 4 - l.turn) % 4];
const mismatches = (cells) => {
  const out = [];
  for (const [cell, l] of Object.entries(cells)) {
    const [x, z] = cell.split(',').map(Number);
    SIDES.forEach(([dx, dz], s) => {
      const n = cells[`${x + dx},${z + dz}`];
      const mine = edge(l, s);
      if (n && mine !== edge(n, (s + 2) % 4)) out.push(`${cell} ${l.tile}/${l.turn} vs ${x + dx},${z + dz} ${n.tile}/${n.turn}`);
      // past the floor: only joints (never half a long slab)
      if (!n && mine !== 'joint') out.push(`${cell} ${l.tile}/${l.turn}: ${mine} facing out`);
    });
  }
  return out;
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:floor_test');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "floor_test"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  check('the set in the bundle: four tiles', Object.keys(tiles).length === 4, Object.keys(tiles).join(' '));
  await cmd('view:10,7.5,13,0,62');
  await browser.click(700, 889);

  // 1. a house of 4 by 3 cells (its corner at 8, 6)
  await browser.key('5');
  for (const z of [6.5, 7.5, 8.5]) {
    await browser.drag([await at(8.5, 0, z), await at(9.5, 0, z), await at(10.5, 0, z), await at(11.5, 0, z)]);
  }
  await press('uiNext');
  check('a house of 4 by 3 cells, designed', (await data('build')) === 'design', await data('build'));

  // 2. its floor painted all over with the flagstones, a cell at a time (a drag through all)
  await press('uiTiles');
  await press('uiTileSet_flagstones');
  check('  the set as the brush', (await data('tilebrush')) === '#set:flagstones', await data('tilebrush'));
  const path = [];
  for (const [k, z] of [6.5, 7.5, 8.5].entries()) {
    const xs = k % 2 ? [11.5, 10.5, 9.5, 8.5] : [8.5, 9.5, 10.5, 11.5];
    for (const x of xs) path.push(await at(x, 0, z));
  }
  await browser.drag(path);
  await sleep(500);
  const first = parse(await data('buildlaid'));
  const wrong = mismatches(first);
  const longs = Object.values(first).filter((l) => l.tile.startsWith('flagstones_long')).length;
  check('every cell laid with the set, every two side by side agreeing', Object.keys(first).length === 12 && Object.values(first).every((l) => tiles[l.tile]) && wrong.length === 0,
    wrong.length ? wrong.join('; ') : `${Object.keys(first).length} cells, ${longs} long halves`);
  check('  each long slab whole (its halves in pairs)', longs % 2 === 0, `${longs}`);
  await shot('1-laid');

  // 3. laid again another way, still fitting
  await press('uiTileRelay');
  await sleep(400);
  const again = parse(await data('buildlaid'));
  const wrongAgain = mismatches(again);
  check('laid again: another way, every two still agreeing', JSON.stringify(again) !== JSON.stringify(first) && Object.keys(again).length === 12 && wrongAgain.length === 0,
    wrongAgain.join('; ') || 'another way');
  await shot('2-again');

  // 3b. a slab put by hand, a quarter turned: kept as it was put when the rest is laid again
  await press('uiTileBrush_flagstones_slab');
  await press('uiTileTurn');
  check('a tile of the set to put by hand, turned', (await data('tilebrush')) === 'flagstones_slab' && (await data('tileturn')) === '1', `${await data('tilebrush')} ${await data('tileturn')}`);
  await browser.click(...(await at(9.5, 0, 7.5)));
  await sleep(300);
  check('  put where clicked, as turned', parse(await data('buildlaid'))['1,1']?.tile === 'flagstones_slab' && parse(await data('buildlaid'))['1,1']?.turn === 1 && parse(await data('buildlaid'))['1,1']?.hand,
    JSON.stringify(parse(await data('buildlaid'))['1,1']));
  await press('uiTileSet_flagstones');
  await press('uiTileRelay');
  await sleep(400);
  const round = parse(await data('buildlaid'));
  const wrongRound = mismatches(round);
  check('  the rest laid again round it, fitting it; it stays', round['1,1']?.tile === 'flagstones_slab' && round['1,1'].turn === 1 && round['1,1'].hand && wrongRound.length === 0,
    wrongRound.join('; ') || JSON.stringify(round['1,1']));

  // 4. confirmed, downloaded: each cell's tile and its turn
  await press('uiConfirm');
  check('confirmed', (await data('build')) === '', await data('build'));
  await cmd('view:10,7.5,10,20,50');
  await shot('3-confirmed');

  // 5. out of doors: the Floors tab lays the flagstones on the grass (4 by 3 cells), each pair
  // agreeing again; one cell's taken away
  await cmd('view:4,11.5,13,0,62');
  await browser.click(700, 889);
  await browser.key('2');
  await press('uiGroundFloors');
  check('the Ground tool, its Floors tab', (await data('groundmode')) === 'floors', await data('groundmode'));
  await press('uiFloorSet_flagstones');
  check('  the set as the brush', (await data('floorbrush')) === '#set:flagstones', await data('floorbrush'));
  await cmd('brush:0');
  const ground = [];
  for (const [k, z] of [10.5, 11.5, 12.5].entries()) {
    const xs = k % 2 ? [5.5, 4.5, 3.5, 2.5] : [2.5, 3.5, 4.5, 5.5];
    for (const x of xs) ground.push(await at(x, 0, z));
  }
  await browser.drag(ground);
  await sleep(500);
  const outside = parse(await data('floorlaid'));
  const wrongOut = mismatches(outside);
  check('the grass laid with the set, every two side by side agreeing', Object.keys(outside).length === 12 && wrongOut.length === 0,
    wrongOut.join('; ') || `${Object.keys(outside).length} cells`);
  await shot('4-outside');
  await press('uiFloorBrush_flagstones_slab');
  await press('uiFloorTurn');
  await browser.click(...(await at(4.5, 0, 11.5)));
  await sleep(300);
  await press('uiFloorSet_flagstones');
  await press('uiFloorRelay');
  await sleep(400);
  const outRound = parse(await data('floorlaid'));
  const wrongOutRound = mismatches(outRound);
  check('  a slab put by hand out there too, the rest laid again round it', outRound['4,11']?.tile === 'flagstones_slab' && outRound['4,11'].turn === 1 && outRound['4,11'].hand && wrongOutRound.length === 0,
    wrongOutRound.join('; ') || JSON.stringify(outRound['4,11']));
  await press('uiFloorNone');
  await browser.click(...(await at(2.5, 0, 10.5)));
  await sleep(300);
  const after = parse(await data('floorlaid'));
  check('  one cell taken away', Object.keys(after).length === 11 && !after['2,10'], `${Object.keys(after).length} cells`);
  await press('uiSave');
  await sleep(1500);
  const record = JSON.parse(readFileSync(join(downloads, 'floor_test.json'), 'utf8'));
  check('downloaded: each cell its tile, its turn, whether put by hand', same(saved(record.buildings?.[0]?.floors?.[0]?.tiles), round),
    JSON.stringify(record.buildings?.[0]?.floors?.[0]?.tiles ?? null).slice(0, 300));
  check('  and the ground out of doors', same(saved(record.floors), after), JSON.stringify(record.floors ?? null).slice(0, 300));
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
