// The game's start in the built site: the main menu (Continue only with a story saved, New
// game where the world says games start, its question when a story would be given up, the
// Options kept after the page is loaded again), and the Editor in the corner: its welcome
// (once a run, never again once asked), its Menu (the main menu, the content, the test
// battles) and its "Start new games here". Screenshots to screenshots/menu-*.png. Exits 1
// when something fails.
//     node tools/menucheck.mjs
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
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
  check('  no story yet: Continue can\'t be picked', (await data('continue')) === '');
  await shot('1-menu');
  await press('uiContinue');
  check('  a click on it does nothing', (await data('state')) === 'menu');

  // 2. the Options: changed, kept after the page is loaded again
  await press('uiOptions');
  check('Options opens', (await data('options')) === 'open');
  await press('uiTime_Night');
  await press('uiText_Slow');
  await press('uiShadows_Low');
  await shot('2-options');
  await press('uiOptionsDone');
  check('  Done closes it', (await data('options')) === '');
  const kept = JSON.parse((await data('keptoptions')) || '{}');
  check('  the choices taken', kept.time === 'night' && kept.text === 'slow' && kept.shadows === 'low', JSON.stringify(kept));
  await load();
  const again = JSON.parse((await data('keptoptions')) || '{}');
  check('  and kept after the page is loaded again', again.time === 'night' && again.text === 'slow' && again.shadows === 'low', JSON.stringify(again));
  await press('uiOptions');
  await browser.key('x');
  await sleep(300);
  check('  B (X, Esc) closes it too', (await data('options')) === '');

  // 3. New game: where the world says games start; the story saved as it goes
  await press('uiNewGame');
  await state('explore', 'the new game');
  await sleep(1500);
  check('New game: on the start map, on its start cell', (await data('map')) === 'test_meadow' && (await data('walker')) === '19,22', `${await data('map')} at ${await data('walker')}`);
  await shot('3-new-game');
  await browser.click(700, 450);
  await browser.key('ArrowUp');
  await sleep(2600);
  await browser.key('Escape');
  await state('menu', 'back on the menu');
  await sleep(1300);
  check('  back on the menu: Continue can be picked now', (await data('continue')) === '1');
  await shot('4-continue');
  await press('uiContinue');
  await state('explore', 'the story again');
  await sleep(1200);
  check('Continue: the story where it was left', (await data('map')) === 'test_meadow' && (await data('walker')) !== '19,22', await data('walker'));
  await browser.key('Escape');
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
  await browser.key('Escape');
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
