// Floors laid by wave function collapse, in the built site's map editor, on a made-up map
// served in the bundle: a house of 4 by 3 cells drawn with the Build tool, its ground floor
// painted all over with a set of floor tiles (the flagstones: small stones, a slab, and a long
// slab over two cells, either way): every two cells side by side agree on the edge between
// them (no long slab's half without its other half); "Lay it again" lays them another way,
// still fitting; a slab put by hand, turned, stays as it was put when the rest is laid again
// round it (which fits it); a long slab's half put by hand mirrored left to right across the
// house's middle (its copy mirrored, the set laid again round both, still fitting); then out of
// doors, the Ground tool's Floors tab laying the flagstones on the grass, every two cells
// agreeing again, one taken away; a half put by hand with quarter turns round a corner picked
// on the map (four, each turned on), then mirrored both ways (four, mirrored where the
// symmetry mirrors them), those taken away all round from one of them and the click undone in
// one step; downloaded, the map has each cell's tile, its turn and whether it's mirrored, the
// house's and the ground's.
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
// "x,z:tile:turn[:flip][:hand];..." (the `buildlaid` hook) as cells, and whether every two
// side by side agree (a mirrored tile's east and west swapped, then turned)
const SIDES = [[0, -1], [1, 0], [0, 1], [-1, 0]];
const parse = (laid) => Object.fromEntries(laid.split(';').filter(Boolean).map((e) => {
  const [cell, tile, turn, ...rest] = e.split(':');
  return [cell, { tile, turn: Number(turn), flip: rest.includes('flip'), hand: rest.includes('hand') }];
}));
const saved = (tiles) => Object.fromEntries((tiles ?? []).map((t) => [`${t.cell[0]},${t.cell[1]}`, { tile: t.tile, turn: t.turn ?? 0, flip: !!t.flip, hand: !!t.by_hand }]));
const same = (a, b) => JSON.stringify(Object.keys(a).sort().map((c) => [c, a[c]])) === JSON.stringify(Object.keys(b).sort().map((c) => [c, b[c]]));
const edge = (l, side) => {
  const s = (side + 4 - l.turn) % 4;
  return tiles[l.tile]?.edges[l.flip ? (4 - s) % 4 : s];
};
const lies = (l, tile, turn, flip) => !!l && l.tile === tile && l.turn === turn && l.flip === flip && l.hand;
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
  // saving downloads each file here, as this check reads them (the browser keeps them otherwise)
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html?saves=downloads`);
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

  // 3c. a long slab's half put by hand mirrored left to right across the house's middle (x 2
  // from its corner, between its second and third cells): its copy at the other end, mirrored;
  // the set laid again round both, fitting each (the halves' other halves beside them)
  await press('uiTileBrush_flagstones_long_a');
  for (let i = 0; i < 3; i++) await press('uiTileTurn');
  await press('uiTileSym_mirror_x');
  check('  mirrored left to right round the floor\'s middle', (await data('tilesym')) === 'mirror_x' && (await data('tilecentre')) === '4,3' && (await data('tileturn')) === '0',
    `${await data('tilesym')} ${await data('tilecentre')} ${await data('tileturn')}`);
  await browser.click(...(await at(8.5, 0, 6.5)));
  await sleep(300);
  let sym = parse(await data('buildlaid'));
  check('  a half put by hand, its copy mirrored at the other end', lies(sym['0,0'], 'flagstones_long_a', 0, false) && lies(sym['3,0'], 'flagstones_long_a', 0, true),
    `${JSON.stringify(sym['0,0'])} ${JSON.stringify(sym['3,0'])}`);
  await press('uiTileSym_none');
  await press('uiTileSet_flagstones');
  await press('uiTileRelay');
  await sleep(400);
  sym = parse(await data('buildlaid'));
  const wrongSym = mismatches(sym);
  check('  the set laid again round them, fitting both (each half against another)', lies(sym['3,0'], 'flagstones_long_a', 0, true) && edge(sym['2,0'], 1) === 'long' && edge(sym['1,0'], 3) === 'long' && wrongSym.length === 0,
    wrongSym.join('; ') || `${JSON.stringify(sym['1,0'])} ${JSON.stringify(sym['2,0'])}`);
  await shot('2b-mirrored');

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

  // 6. a half put by hand with quarter turns round a corner picked on the map (14, 6): four,
  // each a quarter further on; then mirrored both ways round it, mirrored first: four more,
  // mirrored where the symmetry mirrors them; taken away all round from one of them, and back
  // in one step with an undo
  await cmd('view:14,6,14,0,62');
  await browser.click(700, 889);
  // (the set's tiles are listed once the set is picked)
  await press('uiFloorSet_flagstones');
  await press('uiFloorBrush_flagstones_long_a');
  for (let i = 0; i < 3; i++) await press('uiFloorTurn');
  await press('uiFloorSym_turn4');
  check('quarter turns: no centre yet, the next click picks it', (await data('floorsym')) === 'turn4' && (await data('floorcentre')) === '' && (await data('floorturn')) === '0',
    `${await data('floorsym')} "${await data('floorcentre')}" ${await data('floorturn')}`);
  const before = await data('floorlaid');
  await browser.click(...(await at(14.04, 0, 5.97)));
  await sleep(300);
  check('  a corner picked, nothing laid by that click', (await data('floorcentre')) === '28,12' && (await data('floorlaid')) === before, await data('floorcentre'));
  await browser.click(...(await at(13.5, 0, 4.5)));
  await sleep(300);
  let four = parse(await data('floorlaid'));
  check('  one click, four halves round it, each a quarter on', lies(four['13,4'], 'flagstones_long_a', 0, false) && lies(four['15,5'], 'flagstones_long_a', 1, false)
    && lies(four['14,7'], 'flagstones_long_a', 2, false) && lies(four['12,6'], 'flagstones_long_a', 3, false),
    ['13,4', '15,5', '14,7', '12,6'].map((c) => `${c} ${JSON.stringify(four[c])}`).join(' '));
  await press('uiFloorFlip');
  await press('uiFloorSym_mirror_both');
  await browser.click(...(await at(11.5, 0, 3.5)));
  await sleep(300);
  four = parse(await data('floorlaid'));
  check('  mirrored both ways, put mirrored: four more, mirrored where the symmetry mirrors them', (await data('floorflip')) === '1'
    && lies(four['11,3'], 'flagstones_long_a', 0, true) && lies(four['16,3'], 'flagstones_long_a', 0, false)
    && lies(four['11,8'], 'flagstones_long_a', 2, false) && lies(four['16,8'], 'flagstones_long_a', 2, true),
    ['11,3', '16,3', '11,8', '16,8'].map((c) => `${c} ${JSON.stringify(four[c])}`).join(' '));
  await shot('5-symmetry');
  const mirrored = await data('floorlaid');
  await press('uiFloorNone');
  await browser.click(...(await at(16.5, 0, 8.5)));
  await sleep(300);
  const taken = parse(await data('floorlaid'));
  check('  taken away from one: from all four, the quarter turns' + "'" + ' kept', ['11,3', '16,3', '11,8', '16,8'].every((c) => !taken[c]) && ['13,4', '15,5', '14,7', '12,6'].every((c) => taken[c]),
    Object.keys(taken).join(' '));
  await browser.key('z', { ctrl: true });
  await sleep(500);
  check('  undone in one step: all four back', (await data('floorlaid')) === mirrored, (await data('floorlaid')).slice(0, 200));
  const after6 = parse(await data('floorlaid'));
  await press('uiSave');
  await sleep(1500);
  const record = JSON.parse(readFileSync(join(downloads, 'floor_test.json'), 'utf8'));
  check('downloaded: each cell its tile, its turn, whether mirrored, whether put by hand', same(saved(record.buildings?.[0]?.floors?.[0]?.tiles), sym),
    JSON.stringify(record.buildings?.[0]?.floors?.[0]?.tiles ?? null).slice(0, 300));
  check('  and the ground out of doors', same(saved(record.floors), after6), JSON.stringify(record.floors ?? null).slice(0, 300));
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
