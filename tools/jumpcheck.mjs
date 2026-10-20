// Jumps in the built site's map editor, on a made-up map served in the bundle (a field on
// level 1 with a ditch a level down along x 10, a pond south of it, a rise a level up north-east):
// two clicks two cells apart over the ditch make a jump, both ways; one between cells it can't
// join (onto the rise, into the pond) is refused, saying why; on foot, over the ditch and back,
// in an arc; down into the ditch off its edge (a ledge a level high: facing it, its arrow
// again with Shift held; not without Shift, never back up); one way, there and never back,
// then turned round; a jump's end keeps its level
// under the brush (a cell away from it sinks); taken away with Delete and back with an undo; saved
// in the map's record. Screenshots to screenshots/jump-*.png. Exits 1 when something fails.
//     node tools/jumpcheck.mjs
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-jumps');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

// the map: 20 x 14 of grass on level 1; the ditch (0) along x 10; the pond at 11, 9; the rise
// (2) from x 15, north of z 4
const W = 20;
const H = 14;
const terrain = [];
const levels = [];
for (let z = 0; z < H; z++) {
  let t = '';
  let l = '';
  for (let x = 0; x < W; x++) {
    t += x === 11 && z === 9 ? 'w' : 'g';
    l += x === 10 ? '0' : x >= 15 && z < 4 ? '2' : '1';
  }
  terrain.push(t);
  levels.push(l);
}
const map = { id: 'jump_test', name: 'Jump test', width: W, height: H, legend: { g: 'grass', w: 'water' }, terrain, levels };
let levelHeight = 1;
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  bundle.files['maps/jump_test.json'] = map;
  const world = bundle.files['settings/world.json'];
  // a level of ground: half a floor (a character and room above it)
  levelHeight = ((world.character_height ?? 1.5) + (world.level_padding ?? 0.5)) / 2;
  // the colours as they are: no cloud shadows drifting over, no colour steps
  world.toon = { ...(world.toon ?? {}), cloud_shadow: 0, color_steps: 0 };
  return Buffer.from(JSON.stringify(bundle));
};

const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9518, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
// saving downloads each file here, as this check reads them (the browser keeps them otherwise)
const url = `http://127.0.0.1:${server.address().port}/index.html?saves=downloads`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(250);
};
const at = async (name) => (await data(name)).split(',').map(Number);
const press = async (name, wait = 400) => {
  const [x, y] = await at(name);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(wait);
};
// where a point of the world is on the screen
const screen = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(150);
  return at('point');
};
// a click on a cell (its middle, on its level)
const level = (x, z) => (x === 10 ? 0 : x >= 15 && z < 4 ? 2 : 1);
const clickCell = async (x, z) => {
  await browser.click(...(await screen(x + 0.5, level(x, z) * levelHeight, z + 0.5)));
  await sleep(400);
};
const shot = (name) => browser.screenshot(join(shots, `jump-${name}.png`));
const view = 'view:10.5,5.5,16,0,55';

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:jump_test');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "jump_test"', { timeout: 60000, what: 'the test map' });
  await sleep(1500);
  // the asset browser closed (it comes open, along the bottom): more of the view
  if ((await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await cmd(view);
  await browser.click(700, 889); // the status bar: the canvas gets the keys
  await browser.key('2');
  await press('uiGroundJumps');
  check('the Ground tool on jumps', (await data('tool')) === 'ground' && (await data('groundmode')) === 'jumps', `${await data('tool')} ${await data('groundmode')}`);

  // 1. two clicks over the ditch: a jump, both ways
  await clickCell(9, 5);
  check('a first click: its first end', (await data('jumpfirst')) === '9,5', `${await data('jumpfirst')}: ${await data('status')}`);
  await clickCell(11, 5);
  check('a second two cells on: the jump', (await data('jumps')) === '1' && (await data('jump')) === '9,5>11,5;both', `${await data('jumps')} jumps, ${await data('jump')}: ${await data('status')}`);
  check('  and says so', (await data('status')) === 'A jump from 9, 5 to 11, 5 (and back)', await data('status'));
  await browser.move(120, 460);
  await cmd('view:10.5,5.5,6,30,30');
  await sleep(1200);
  await shot('1-both');
  await cmd('view:10.5,5.5,5,90,16');
  await sleep(1200);
  await shot('2-both-side');
  // as it's seen off the Jumps tab: the little ramps alone
  await press('uiGroundPaint');
  await browser.move(120, 460);
  await cmd('view:10.5,5.5,6,30,30');
  await sleep(1200);
  await shot('3-plain');
  await press('uiGroundJumps');

  // 2. refused: onto the rise (a level up), into the pond
  await cmd('view:13.5,3.5,16,0,55');
  await clickCell(13, 2);
  await clickCell(15, 2);
  check('none onto ground a level up', (await data('jumps')) === '1' && (await data('status')).includes('levels 1 and 2'), await data('status'));
  check('  its first end kept', (await data('jumpfirst')) === '13,2', await data('jumpfirst'));
  await browser.key('Escape');
  check('  let go of with Escape', (await data('jumpfirst')) === '', await data('jumpfirst'));
  await cmd(view);
  await clickCell(9, 9);
  await clickCell(11, 9);
  check('none into the pond', (await data('jumps')) === '1' && (await data('status')).includes('11, 9 is water'), await data('status'));
  await browser.key('Escape');

  // 3. on foot: over the ditch and back, in an arc
  await press('uiWalk');
  await cmd('walkto:9,5');
  await sleep(400);
  const ground = Number(await data('walkery'));
  // (its height through the jump: the most of it, sampled as it goes)
  const jump = async (key, want, what, name, opts = {}) => {
    await browser.key(key, opts);
    if (name) {
      await sleep(170);
      await shot(name);
    }
    let top = 0;
    for (let k = 0; k < 10; k++) {
      top = Math.max(top, Number(await data('walkery')));
      await sleep(50);
    }
    await sleep(300);
    const got = await data('walker');
    check(what, got === want, `walker ${got} (wanted ${want})`);
    return top;
  };
  const top = await jump('ArrowRight', '11,5', 'east over the ditch');
  check('  in an arc', top > ground + 0.3 && Math.abs(Number(await data('walkery')) - ground) < 0.05, `on ${ground.toFixed(2)}, up to ${top.toFixed(2)}, down on ${Number(await data('walkery')).toFixed(2)}`);
  await jump('ArrowLeft', '9,5', 'and back', '4-mid-jump');
  // down a ledge a level high, away from the jump: the first arrow turns to face it, the
  // next does nothing without Shift; with Shift held, down into the ditch, up a little first
  await cmd('walkto:9,7');
  await sleep(400);
  await jump('ArrowRight', '9,7', 'a ledge: its arrow first turns to face it');
  await jump('ArrowRight', '9,7', '  again without Shift: no jump');
  const high = await jump('ArrowRight', '10,7', '  again with Shift held: down it', null, { shift: true });
  const below = Number(await data('walkery'));
  check('    up a little first, onto the level below', high > ground + 0.15 && Math.abs(below - (ground - levelHeight)) < 0.05, `on ${ground.toFixed(2)}, up to ${high.toFixed(2)}, down on ${below.toFixed(2)}`);
  await jump('ArrowRight', '10,7', '  never back up, Shift or not', null, { shift: true });
  await press('uiWalk');

  // 4. one way: there, never back; turned round
  await cmd(view);
  await clickCell(9, 5);
  check('its end clicked: picked', (await data('jump')) === '9,5>11,5;both', await data('jump'));
  await press('uiJumpOneWay');
  check('  one way', (await data('jump')) === '9,5>11,5;one', await data('jump'));
  await browser.move(120, 460);
  await cmd('view:10.5,5.5,6,30,30');
  await sleep(1200);
  await shot('5-one-way');
  await cmd(view);
  await press('uiWalk');
  await cmd('walkto:9,5');
  await sleep(400);
  await jump('ArrowRight', '11,5', 'one way: there');
  await jump('ArrowLeft', '11,5', '  never back');
  await press('uiWalk');
  await cmd(view);
  await press('uiJumpTurn');
  check('  turned round', (await data('jump')) === '11,5>9,5;one', await data('jump'));

  // 5. its end keeps its level under the brush (level 0 picked from the ditch); a cell
  // away from it sinks (seen in the record saved)
  await press('uiGroundPaint');
  await browser.click(...(await screen(10.5, 0, 7.5)), 'right');
  await sleep(300);
  await clickCell(9, 5);
  await browser.move(...(await screen(9.5, levelHeight, 5.5)));
  await sleep(300);
  check("a jump's end keeps its level", (await data('groundcell')) === '9,5,1' && (await data('status')).startsWith("A jump's ends keep their level"), `${await data('groundcell')}: ${await data('status')}`);
  await clickCell(8, 3);

  // 6. taken away with Delete, back with an undo
  await press('uiGroundJumps');
  await clickCell(11, 5);
  await browser.key('Escape');
  check('picked again', (await data('jump')) === '11,5>9,5;one' && (await data('jumpfirst')) === '', `${await data('jump')} ${await data('jumpfirst')}`);
  await browser.key('Delete');
  await sleep(300);
  check('  taken away with Delete', (await data('jumps')) === '0', await data('jumps'));
  await browser.key('z', { ctrl: true });
  await sleep(500);
  check('  back with an undo', (await data('jumps')) === '1', await data('jumps'));

  // 7. saved with the map
  await press('uiSave');
  await sleep(1500);
  const saved = JSON.parse(readFileSync(join(downloads, 'jump_test.json'), 'utf8'));
  const j = saved.jumps?.[0];
  check('saved in the record', saved.jumps?.length === 1 && j.from.join(',') === '11,5' && j.to.join(',') === '9,5' && j.one_way === true, JSON.stringify(saved.jumps));
  check('  its end on its level, the cell painted away from it sunk', saved.levels?.[5]?.[9] === '1' && saved.levels?.[3]?.[8] === '0', `${saved.levels?.[5]?.[9]} ${saved.levels?.[3]?.[8]}`);
  if (browser.errors.length) check('no page errors', false, browser.errors.join(' | '));
} catch (e) {
  console.log(`FAIL ${e.message}`);
  failed++;
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
