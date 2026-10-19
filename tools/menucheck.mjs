// The game's start in the built site: the main menu over the world (Continue only with a
// story saved, an item picked stepping right while the others leave, New game where the world
// says games start, its question when a story would be given up), the Options in tabs (kept
// after the page is loaded again; a key given to A works on the menu), and the Editor's icon
// in the corner (growing when hovered): its welcome (once a run, never again once asked), its
// Menu (the main menu, the content, the test battles) and where new games start: "Start new
// games here", then picked on the map (a cell clicked, then dragged from the way the player
// faces), marked there, turned, and in the World view.
// Screenshots to screenshots/menu-*.png. Exits 1 when something fails.
//     node tools/menucheck.mjs
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads-menu');
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
const browser = await Browser.launch({ width: 1400, height: 900, port: 9496, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(250);
};
const shot = (name) => browser.screenshot(join(shots, `menu-${name}.png`));
const at = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await at(name);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(300);
};
// a point in the world on the screen, and the cell (x,z) the pointer is over there
const point = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  return (await data('point')).split(',').map(Number);
};
const cellAt = async ([x, y]) => {
  await browser.move(x, y);
  await sleep(250);
  return (await data('groundcell')).split(',').slice(0, 2).join(',');
};
// the world.json downloaded last (the browser numbers the ones after the first)
const lastWorld = () => {
  const files = readdirSync(downloads).filter((f) => /^world.*\.json$/.test(f)).map((f) => join(downloads, f));
  files.sort((a, b) => statSync(a).mtimeMs - statSync(b).mtimeMs);
  return files.length ? JSON.parse(readFileSync(files[files.length - 1], 'utf8')) : {};
};
const state = (s, what) => browser.waitFor(`document.body.dataset.state === ${JSON.stringify(s)}`, { timeout: 30000, what });
const load = async () => {
  await browser.goto(url);
  await state('menu', 'the main menu');
  await sleep(600);
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  // a first run: nothing kept
  await load();
  await browser.evaluate('(localStorage.clear(), true)');
  await load();

  // 1. the main menu: Continue waits for a story; the Editor in the bottom-right corner
  check('the main menu first', (await data('state')) === 'menu');
  const items = await Promise.all(['uiContinue', 'uiNewGame', 'uiOptions', 'uiExit'].map(at));
  check('  Continue, New game, Options, Exit, one under another', items.every(([x, y], i) => x > 0 && (i === 0 || y > items[i - 1][1])), JSON.stringify(items));
  const [ex, ey] = await at('uiEditor');
  check('  the Editor in the bottom-right corner', ex > 1250 && ey > 820, `${ex},${ey}`);
  await browser.move(ex, ey);
  await sleep(350);
  check('  its icon grows when hovered', Number(await data('editorgrow')) > 0.9, await data('editorgrow'));
  await browser.move(700, 450);
  check('  no story yet: Continue can\'t be picked', (await data('continue')) === '');
  // the camera over the world: one angle, never turning, gliding sideways across the map
  const cam = async () => (await data('menucam')).split(',').map(Number);
  const c0 = await cam();
  await sleep(1500);
  const c1 = await cam();
  const yaw = (c0[2] * Math.PI) / 180;
  const moved = Math.hypot(c1[0] - c0[0], c1[1] - c0[1]);
  const sideways = Math.abs((c1[0] - c0[0]) * Math.cos(yaw) - (c1[1] - c0[1]) * Math.sin(yaw));
  check('  the camera never turns, and glides sideways', c0[2] === c1[2] && moved > 0.2 && sideways > 0.9 * moved, `${c0} -> ${c1}`);
  await shot('1-menu');
  await press('uiContinue');
  check('  a click on it does nothing', (await data('state')) === 'menu');

  // 2. the Options, in tabs: changed, kept after the page is loaded again
  const [ox, oy] = await at('uiOptions');
  await browser.click(ox, oy);
  await sleep(60);
  check('an item picked: the menu waits for its tween', (await data('titlebusy')) === '1' && (await data('options')) === '');
  await sleep(400);
  check('  then the Options open, on the Game tab', (await data('options')) === 'open' && (await data('optionstab')) === 'Game');
  check('  the time of day isn\'t an option (the real one)', (await data('uiTime_Night')) === '');
  await press('uiText_Slow');
  await press('uiTab_Graphics');
  await press('uiShadows_Low');
  await shot('2-options');
  // Controls: A's first key (Z) given to K
  await press('uiTab_Controls');
  check("  the Controls tab: the keyboard's keys only (no controller in use)", (await data('optionstab')) === 'Controls' && (await data('controlsview')) === 'keyboard', await data('controlsview'));
  await press('uiKey_A_0');
  check('  a key clicked waits for the new one', (await data('waitingkey')) === 'A');
  await browser.key('k');
  await sleep(200);
  check('  K given to A (B still open: the key was the Controls\')', (await data('waitingkey')) === '' && (await data('options')) === 'open');
  await shot('2b-controls');
  await press('uiTab_Audio');
  await press('uiEffects');
  await press('uiOptionsDone');
  check('  Done closes it', (await data('options')) === '');
  const kept = JSON.parse((await data('keptoptions')) || '{}');
  check('  the choices taken', kept.text === 'slow' && kept.shadows === 'low' && kept.controls?.A?.[0] === 'K' && kept.effects < 100 && !('time' in kept), JSON.stringify(kept));
  await load();
  const again = JSON.parse((await data('keptoptions')) || '{}');
  check('  and kept after the page is loaded again', again.text === 'slow' && again.shadows === 'low' && again.controls?.A?.[0] === 'K', JSON.stringify(again));
  check("  the menu's hint names K", (await data('keyshint')).includes('K or Space: select'), await data('keyshint'));
  // K picks on the menu now: down to Options, K
  await browser.click(700, 450);
  await browser.key('ArrowDown');
  await browser.key('k');
  await sleep(450);
  check('  K picks an item now (A)', (await data('options')) === 'open', await data('options'));
  await press('uiTab_Controls');
  await press('uiOptionsDefaults');
  await browser.key('x');
  await sleep(300);
  check('  the Controls back as they came; B (X, Esc) closes it', (await data('options')) === '' && JSON.parse(await data('keptoptions')).controls?.A?.[0] === 'Z');
  check("  and the hint with them", (await data('keyshint')).includes('Z or Space: select'), await data('keyshint'));

  // 3. New game: where the world says games start; the story saved as it goes
  await press('uiNewGame');
  await state('explore', 'the new game');
  await sleep(1500);
  check('New game: on the start map, on its start cell', (await data('map')) === 'test_meadow' && (await data('walker')) === '19,22', `${await data('map')} at ${await data('walker')}`);
  await shot('3-new-game');
  await browser.click(700, 450);
  await browser.key('ArrowUp');
  await sleep(2600);
  // Esc doesn't leave (it puts the ring on the HUD's buttons); the HUD's Exit does
  await browser.key('Escape');
  await sleep(300);
  check("  Esc doesn't leave: the ring on Settings", (await data('state')) === 'explore' && (await data('hudring')) === 'uiHudSettings', `${await data('state')} ${await data('hudring')}`);
  await browser.key('Escape');
  await press('uiHudExit');
  await state('menu', 'back on the menu');
  await sleep(1300);
  check('  back on the menu: Continue can be picked now', (await data('continue')) === '1');
  await shot('4-continue');
  await press('uiContinue');
  await state('explore', 'the story again');
  await sleep(1200);
  check('Continue: the story where it was left', (await data('map')) === 'test_meadow' && (await data('walker')) !== '19,22', await data('walker'));
  await press('uiHudExit');
  await state('menu', 'the menu');
  await sleep(1300);

  // 4. New game with a story saved: asked first
  await press('uiNewGame');
  check('New game with a story saved: asked first', (await data('confirmnew')) === 'open' && (await data('state')) === 'menu');
  await shot('5-new-game-question');
  await press('uiNewGameNo');
  check('  "Keep my story": nothing lost', (await data('confirmnew')) === '' && (await data('continue')) === '1');
  await press('uiNewGame');
  await press('uiNewGameYes');
  await state('explore', 'the new game');
  await sleep(1200);
  check('  "Start a new game": from the start again', (await data('walker')) === '19,22', await data('walker'));
  await press('uiHudExit');
  await state('menu', 'the menu');
  await sleep(600);

  // 5. the Editor: its welcome first
  await press('uiEditor');
  await state('maps', 'the editor');
  await sleep(1200);
  check('the Editor: the map editor, its welcome on top', (await data('welcome')) === 'open');
  await shot('6-editor-welcome');
  await press('uiWelcomeOk');
  check('  "Start editing" closes it', (await data('welcome')) === '');
  // its Menu: the test battles, the content, the main menu
  await press('uiEditorMenu');
  await shot('7-editor-menu');
  await press('uiTestBattles');
  await state('battles', 'the test battles');
  await sleep(400);
  await shot('8-test-battles');
  await press('uiBackToEditor');
  await state('maps', 'the editor again');
  await sleep(800);
  check('  the test battles, and back: no welcome again this run', (await data('welcome')) === '');
  await press('uiEditorMenu');
  await press('uiContentBrowser');
  await state('editor', 'the content');
  await browser.key('x');
  await state('maps', 'the editor from the content');
  await sleep(600);
  await press('uiEditorMenu');
  await press('uiMainMenu');
  await state('menu', 'the main menu from the editor');
  await press('uiEditor');
  await state('maps', 'the editor once more');
  await sleep(800);
  check('  the main menu and back: still none', (await data('welcome')) === '');

  // 6. another run: the welcome again, until it's asked not to
  await load();
  await press('uiEditor');
  await state('maps', 'the editor');
  await sleep(1000);
  check('another run: the welcome again', (await data('welcome')) === 'open');
  await press('uiWelcomeHide');
  await press('uiWelcomeOk');
  await load();
  await press('uiEditor');
  await state('maps', 'the editor');
  await sleep(1000);
  check('  "Don\'t show this again": not again', (await data('welcome')) === '');

  // 7. "Start new games here": the map panel's, where the view looks
  await cmd('view:12.5,20.5');
  await sleep(400);
  await press('uiStartHere');
  await sleep(600);
  check('"Start new games here": the cell in the middle of the view', (await data('start')) === 'test_meadow:12,20', await data('start'));
  const file = join(downloads, 'world.json');
  const world = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : {};
  check('  world.json downloaded with it (the browser can\'t write the project)', world.start?.map === 'test_meadow' && world.start?.at?.join(',') === '12,20', JSON.stringify(world.start));
  await shot('9-start-here');

  // 8. picked on the map: a cell clicked (facing as it was), marked there; then dragged from
  // another, facing the way it went; turned a quarter; in the World view
  await press('uiStartPick');
  check('"Pick it on the map": picking', (await data('startpick')) === '1');
  const one = await point(10.5, 0, 19.5);
  const clicked = await cellAt(one);
  await browser.click(...one);
  await sleep(600);
  check('  a cell clicked: new games start there, picking over', (await data('start')) === `test_meadow:${clicked}` && (await data('startpick')) === '', `${await data('start')} (clicked ${clicked})`);
  check('  facing as it was (south), marked on the map', (await data('startfacing')) === 'south' && (await data('startmark')) === `${clicked}:south`, `${await data('startfacing')} ${await data('startmark')}`);
  await press('uiStartPick');
  const from = await point(11.5, 0, 21.5);
  const to = await point(14.5, 0, 21.5);
  const dragged = await cellAt(from);
  await browser.drag([from, to]);
  await sleep(600);
  check('  dragged east from a cell: there, facing east', (await data('start')) === `test_meadow:${dragged}` && (await data('startfacing')) === 'east', `${await data('start')} ${await data('startfacing')} (from ${dragged})`);
  const picked = lastWorld();
  check('  world.json downloaded with it: one start, facing east (270)', picked.start?.map === 'test_meadow' && picked.start?.at?.join(',') === dragged && picked.start?.facing === 270, JSON.stringify(picked.start));
  await shot('10-start-picked');
  await press('uiStartFacing');
  await sleep(500);
  check('  "Facing" turned a quarter clockwise: east to south', (await data('startfacing')) === 'south' && lastWorld().start?.facing === 0, `${await data('startfacing')} ${JSON.stringify(lastWorld().start)}`);
  await press('uiStartPick');
  await browser.key('Escape');
  await sleep(300);
  check('  Esc stops picking, the start as it was', (await data('startpick')) === '' && (await data('start')) === `test_meadow:${dragged}`, `${await data('startpick')} ${await data('start')}`);
  await press('uiWorld');
  await sleep(800);
  check('  marked in the World view', /^\d+,\d+$/.test(await data('worldstart')), await data('worldstart'));
  await shot('11-start-world');

  if (browser.errors.length) {
    failed++;
    console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  }
} catch (e) {
  failed++;
  console.log(`FAIL ${e.message}`);
  await browser.screenshot(join(shots, 'menu-error.png')).catch(() => {});
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
