// Controllers in the built site, with stand-in controllers (the page's navigator.getGamepads
// answers with controllers this script presses, as the Gamepad API would): one connected takes
// over from the keyboard (its word at the foot of the screen, the menu's hint in its buttons,
// no mouse pointer), a key or the mouse takes back, its d-pad, its stick and its A work the
// menu, the Options (tabs on LB/RB, a ring round what the buttons are on, a controller button
// given to A), the New game question, the overworld (walking; Start puts the ring on the HUD's
// buttons, Exit leaves); a second
// controller used takes over, the one in use gone hands over to the other, then to the
// keyboard; nothing done while the page hasn't the focus counts; a battle played with one
// names its buttons (the target picker, the prompts, the results). Screenshots to
// screenshots/pad-*.png. Exits 1 when something fails.
//     node tools/padcheck.mjs
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { pick } from './encounters.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
mkdirSync(shots, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (what, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
  if (!ok) failed++;
};

// the stand-in controllers: four slots, as the Gamepad API has them ("standard" layout:
// 0 South, 1 East, 2 West, 3 North, 4 LB, 5 RB, 6 LT, 7 RT, 8 Select, 9 Start, 10 and 11 the
// sticks' presses, 12 to 15 the d-pad up, down, left, right; axes: left stick x, y (down +))
const STAND_INS = `(() => {
  const slots = [null, null, null, null];
  const make = (index, id) => ({
    id, index, connected: true, mapping: 'standard', timestamp: performance.now(),
    buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })),
    axes: [0, 0, 0, 0],
  });
  window.__pads = {
    connect(index, id) { slots[index] = make(index, id); },
    disconnect(index) { slots[index] = null; },
    set(index, button, down) {
      const b = slots[index].buttons[button];
      b.pressed = down; b.touched = down; b.value = down ? 1 : 0;
      slots[index].timestamp = performance.now();
    },
    axis(index, axis, value) { slots[index].axes[axis] = value; slots[index].timestamp = performance.now(); },
  };
  Object.defineProperty(Navigator.prototype, 'getGamepads', { value: function () { return slots.slice(); }, configurable: true, writable: true });
})();`;
const XBOX = 'Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)';
const DUALSENSE = 'DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)';
const B = { south: 0, east: 1, west: 2, north: 3, lb: 4, rb: 5, select: 8, start: 9, up: 12, down: 13, left: 14, right: 15 };

const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9497, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const shot = (name) => browser.screenshot(join(shots, `pad-${name}.png`));
const state = (s, what) => browser.waitFor(`document.body.dataset.state === ${JSON.stringify(s)}`, { timeout: 30000, what });
const pads = (js) => browser.evaluate(`(window.__pads.${js}, true)`);
const cmd = (c) => browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
// a button tapped on a stand-in: held a few frames, let go
const tap = async (button, index = 0, hold = 120) => {
  await pads(`set(${index}, ${button}, true)`);
  await sleep(hold);
  await pads(`set(${index}, ${button}, false)`);
  await sleep(150);
};
const load = async () => {
  await browser.goto(url);
  await state('menu', 'the main menu');
  await sleep(600);
};
const kept = async () => JSON.parse((await data('keptoptions')) || '{}');

try {
  await browser.viewport(1400, 900);
  await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: STAND_INS });
  await load();
  await browser.evaluate('(localStorage.clear(), true)');
  await load();
  await browser.click(700, 450);
  await sleep(200);
  check('the page has the focus', await browser.evaluate('document.hasFocus() && !document.hidden'));

  // 1. no controller: the keyboard; one connected takes over
  check('no controller: the keyboard is listened to', (await data('device')) === 'keys' && (await data('pads')) === '0', `${await data('device')}, ${await data('pads')} controllers`);
  const keysHint = await data('keyshint');
  await pads(`connect(0, ${JSON.stringify(XBOX)})`);
  await sleep(300);
  check('a controller connected takes over', (await data('device')) === 'pad' && (await data('pads')) === '1', `${await data('device')}, ${await data('padname')}`);
  check('  a word on it at the foot of the screen, its name as people say it', (await data('padnote')) === 'Controller connected: Xbox Wireless Controller', await data('padnote'));
  check("  the menu's hint in its buttons", (await data('keyshint')) === 'D-pad: choose  ·  A: select  ·  B: back', `${keysHint} -> ${await data('keyshint')}`);
  check('  no mouse pointer over the game', (await browser.evaluate("getComputedStyle(document.getElementById('game')).cursor")) === 'none', await browser.evaluate("getComputedStyle(document.getElementById('game')).cursor"));
  await shot('1-connected');

  // 2. its d-pad and stick work the menu; a key or the mouse takes back
  check('  New game first (no story yet)', (await data('titlesel')) === 'New game', await data('titlesel'));
  await tap(B.down);
  check('  its d-pad: down to Options', (await data('titlesel')) === 'Options', await data('titlesel'));
  await browser.key('ArrowUp');
  await sleep(200);
  check('a key pressed: the keyboard again, and the key works', (await data('device')) === 'keys' && (await data('titlesel')) === 'New game', `${await data('device')}, ${await data('titlesel')}`);
  check("  the hint in the keyboard's keys", (await data('keyshint')) === keysHint, await data('keyshint'));
  await pads('axis(0, 1, 0.9)');
  await sleep(200);
  await pads('axis(0, 1, 0)');
  await sleep(200);
  check("the controller's stick: the controller again, down to Options", (await data('device')) === 'pad' && (await data('titlesel')) === 'Options', `${await data('device')}, ${await data('titlesel')}`);
  await browser.move(600, 400);
  await browser.move(660, 430);
  await sleep(200);
  check('the mouse moved: the keyboard and mouse again, the pointer back', (await data('device')) === 'keys' && (await browser.evaluate("getComputedStyle(document.getElementById('game')).cursor")) !== 'none', `${await data('device')}, ${await browser.evaluate("getComputedStyle(document.getElementById('game')).cursor")}`);

  // 3. the Options with the controller: A opens them, a ring where the buttons are, the
  // d-pad changes a value, LB/RB the tabs, a controller button given to A
  await tap(B.south);
  await sleep(450);
  check('A on Options: they open', (await data('options')) === 'open' && (await data('device')) === 'pad', `${await data('options')}, ${await data('device')}`);
  check('  the ring on the first row', (await data('optionsring')) === '0,0', await data('optionsring'));
  await tap(B.right);
  await tap(B.rb);
  check('  Right: the text slower; RB: the next tab', (await data('optionstab')) === 'Graphics', await data('optionstab'));
  await tap(B.lb);
  check('  LB: back', (await data('optionstab')) === 'Game', await data('optionstab'));
  await tap(B.rb);
  await tap(B.rb);
  check('  the Controls: the controller\'s buttons only', (await data('optionstab')) === 'Controls' && (await data('controlsview')) === 'controller', `${await data('optionstab')}, ${await data('controlsview')}`);
  check("  the ring on A's first button", (await data('optionsring')) === '0,0', await data('optionsring'));
  await shot('2-controls');
  await tap(B.south);
  check('  A on it: waits for another', (await data('waitingpad')) === 'A', await data('waitingpad'));
  await tap(B.north);
  check('  North pressed: given to A', (await data('waitingpad')) === '' && (await data('options')) === 'open', `${await data('waitingpad')}, ${await data('options')}`);
  // one more for A ("+", then West), then West taken off (Select), and North kept (Start);
  // A is North now
  await tap(B.right);
  await tap(B.north);
  check('  "+": waits for one more', (await data('waitingpad')) === 'A', await data('waitingpad'));
  await tap(B.west);
  check('  West added', JSON.stringify((await kept()).pad?.A) === '["North","West"]', JSON.stringify((await kept()).pad?.A));
  check('  the ring on it', (await data('optionsring')) === '0,1', await data('optionsring'));
  await tap(B.north);
  await shot('2c-waiting');
  await tap(B.select);
  check('  A on it, Select: taken off', JSON.stringify((await kept()).pad?.A) === '["North"]' && (await data('waitingpad')) === '', JSON.stringify((await kept()).pad?.A));
  await tap(B.left);
  await tap(B.north);
  await tap(B.start);
  check('  A on North, Start: kept as it was', JSON.stringify((await kept()).pad?.A) === '["North"]' && (await data('waitingpad')) === '', JSON.stringify((await kept()).pad?.A));
  // a key pressed: the keyboard's keys instead; the controller again: its buttons
  await browser.key('g');
  await sleep(200);
  check('  a key pressed: the keyboard\'s keys only', (await data('device')) === 'keys' && (await data('controlsview')) === 'keyboard', `${await data('device')}, ${await data('controlsview')}`);
  await shot('2b-controls-keyboard');
  await tap(B.down);
  check('  a controller button (Down: B\'s row): its buttons again', (await data('controlsview')) === 'controller' && (await data('optionsring')) === '1,0', `${await data('controlsview')}, ${await data('optionsring')}`);
  // a key pressed while a button is waited for: the wait is over, nothing changed
  await tap(B.north);
  check("  A on B's button: it waits", (await data('waitingpad')) === 'B', await data('waitingpad'));
  await browser.key('g');
  await sleep(200);
  check('  a key meanwhile: no more waiting, the keyboard\'s keys', (await data('waitingpad')) === '' && (await data('controlsview')) === 'keyboard' && JSON.stringify((await kept()).pad?.A) === '["North"]', `${await data('waitingpad')}, ${await data('controlsview')}`);
  await tap(B.east);
  check("  B closes them; what's kept", (await data('options')) === '', await data('options'));
  const k = await kept();
  check('  kept: the text slow, A on North', k.text === 'slow' && JSON.stringify(k.pad?.A) === '["North"]', JSON.stringify({ text: k.text, pad: k.pad }));
  check("  the hint: Y selects", (await data('keyshint')).includes('Y: select'), await data('keyshint'));
  await tap(B.south);
  await sleep(450);
  check('  South picks nothing now', (await data('options')) === '' && (await data('titlebusy')) === '', `${await data('options')}`);
  await tap(B.north);
  await sleep(450);
  check('  North does (on the tab left open)', (await data('options')) === 'open' && (await data('optionstab')) === 'Controls', `${await data('options')}, ${await data('optionstab')}`);
  // the Controls as they came: up from the top row to Done and Defaults, Right, A (North)
  await tap(B.up);
  await tap(B.right);
  check('  Defaults', (await data('optionsring')) === '8,1', await data('optionsring'));
  await tap(B.north);
  await tap(B.east);
  check('  the Controls as they came; closed', (await data('options')) === '' && JSON.stringify((await kept()).pad?.A) === '["South"]', JSON.stringify((await kept()).pad?.A));

  // 4. a new game with the controller: the overworld walked; Start puts the ring on the HUD
  // (Settings), right: Exit, A leaves
  await tap(B.up);
  check('  up to New game', (await data('titlesel')) === 'New game', await data('titlesel'));
  await tap(B.south);
  await state('explore', 'the new game');
  await sleep(1500);
  check('A on New game: the overworld', (await data('state')) === 'explore', await data('state'));
  check("  its keys name the controller's buttons", (await data('walkhint')) === 'D-pad moves  ·  A speaks  ·  Menu: the buttons', await data('walkhint'));
  const start = (await data('walker')).split(',').map(Number);
  await pads(`set(0, ${B.up}, true)`);
  await sleep(700);
  await pads(`set(0, ${B.up}, false)`);
  await sleep(500);
  const up = (await data('walker')).split(',').map(Number);
  check('  the d-pad held: walking north', up[1] < start[1] && up[0] === start[0], `${start} -> ${up}`);
  await pads('axis(0, 0, 0.95)');
  await sleep(600);
  await pads('axis(0, 0, 0)');
  await sleep(500);
  const right = (await data('walker')).split(',').map(Number);
  check('  the stick pushed: walking east', right[0] > up[0], `${up} -> ${right}`);
  await shot('3-overworld');
  await tap(B.start);
  check('  Start: the ring on Settings, still in the world', (await data('hudring')) === 'uiHudSettings' && (await data('state')) === 'explore', `${await data('hudring')} ${await data('state')}`);
  await tap(B.right);
  await tap(B.south);
  await state('menu', 'the menu again');
  check('  right, A: Exit, back on the menu', (await data('state')) === 'menu');
  await sleep(600);

  // 5. the New game question, answered with the controller
  await tap(B.down);
  if ((await data('titlesel')) !== 'New game') await tap(B.up);
  check('  New game', (await data('titlesel')) === 'New game', await data('titlesel'));
  await tap(B.south);
  await sleep(450);
  check('the question, on "Keep my story"', (await data('confirmnew')) === 'open' && (await data('confirmsel')) === 'no', `${await data('confirmnew')}, ${await data('confirmsel')}`);
  await tap(B.left);
  check('  Left: "Start a new game"', (await data('confirmsel')) === 'yes', await data('confirmsel'));
  await shot('4-question');
  await tap(B.right);
  await tap(B.south);
  check('  Right, A: the story kept', (await data('confirmnew')) === '' && (await data('state')) === 'menu', `${await data('confirmnew')}, ${await data('state')}`);

  // 6. nothing done while the page hasn't the focus counts
  await browser.key('ArrowDown');
  await sleep(200);
  const sel = await data('titlesel');
  await browser.evaluate('(document.hasFocus = () => false, true)');
  await sleep(100);
  await tap(B.down);
  check('the page without the focus: a press neither takes over nor moves', (await data('device')) === 'keys' && (await data('titlesel')) === sel, `${await data('device')}, ${sel} -> ${await data('titlesel')}`);
  await browser.evaluate('(delete document.hasFocus, true)');
  await sleep(100);

  // 7. two controllers: the one used is listened to; the one in use gone, the other; then
  // the keyboard
  await pads(`connect(1, ${JSON.stringify(DUALSENSE)})`);
  await sleep(300);
  check('a second controller connected takes over', (await data('device')) === 'pad' && (await data('pads')) === '2' && (await data('padname')).startsWith('DualSense'), `${await data('pads')}, ${await data('padname')}`);
  check('  its own labels: Cross selects', (await data('keyshint')) === 'D-pad: choose  ·  Cross: select  ·  Circle: back', await data('keyshint'));
  await tap(B.down, 0);
  check('  the first one used: it again', (await data('padname')).startsWith('Xbox') && (await data('keyshint')).includes('A: select'), `${await data('padname')}, ${await data('keyshint')}`);
  await pads('disconnect(0)');
  await sleep(300);
  check('  the one in use gone: the other', (await data('device')) === 'pad' && (await data('pads')) === '1' && (await data('padname')).startsWith('DualSense'), `${await data('pads')}, ${await data('padname')}`);
  check('  a word on it', (await data('padnote')).startsWith('Controller disconnected: Xbox'), await data('padnote'));
  await pads('disconnect(1)');
  await sleep(300);
  check('  both gone: the keyboard', (await data('device')) === 'keys' && (await data('pads')) === '0', `${await data('device')}, ${await data('pads')}`);
  await shot('5-gone');

  // 8. a DualSense in slot 0, where the Xbox controller was: its own name and buttons (gilrs
  // keeps a slot's first controller: the game reads the slot). A battle played with it: the
  // target picker, the prompts and the results name what's printed on it (Cross, Circle); a
  // key pressed, the keyboard's keys
  const wild = pick('wild');
  if (!wild) throw new Error('no wild encounter in the content: the battle part needs one');
  await browser.evaluate(`(() => {
    window.__prompts = [];
    new MutationObserver(() => {
      const p = document.body.dataset.prompt || '';
      if (p && !window.__prompts.includes(p)) window.__prompts.push(p);
    }).observe(document.body, { attributes: true, attributeFilter: ['data-prompt'] });
    return true;
  })()`);
  await pads(`connect(0, ${JSON.stringify(DUALSENSE)})`);
  await sleep(300);
  check('a DualSense where the Xbox controller was: its own name', (await data('padname')) === 'DualSense Wireless Controller' && (await data('padnote')) === 'Controller connected: DualSense Wireless Controller', `${await data('padname')}, ${await data('padnote')}`);
  check('  its own buttons in the hint', (await data('keyshint')) === 'D-pad: choose  ·  Cross: select  ·  Circle: back', await data('keyshint'));
  await shot('6-reused-slot');
  await cmd(`text:fast;time:day;start:${wild}`);
  await browser.waitFor('document.body.dataset.menu === "open"', { timeout: 60000, what: 'the battle menu' });
  await sleep(300);
  check('a battle, the controller in use', (await data('state')) === 'battle' && (await data('device')) === 'pad' && (await data('padname')).startsWith('DualSense'), `${await data('state')}, ${await data('device')}, ${await data('padname')}`);
  await tap(B.south);
  check('  Cross on Attack: the target picker names Cross and Circle', (await data('targethint')) === '◀ ▶ choose · Cross confirm · Circle back', await data('targethint'));
  await shot('6-target');
  await tap(B.south);
  await browser.waitFor('window.__prompts.length > 0', { timeout: 10000, what: 'a prompt' }).catch(() => {});
  await shot('6b-prompt');
  // Cross through the menus to the end (the AI's side at last if it takes long)
  const started = Date.now();
  while ((await data('state')) === 'battle' && Date.now() - started < 120000) {
    if ((await data('menu')) === 'open') {
      await tap(B.south);
      await tap(B.south);
    }
    await sleep(150);
  }
  if ((await data('state')) === 'battle') await cmd('auto:1');
  await state('results', 'the end of the battle').catch(() => {});
  await cmd('auto:0');
  const prompts = await browser.evaluate('window.__prompts');
  check('  the prompts name Cross', prompts.includes('Press Cross at impact') && prompts.every((p) => /\bCross\b/.test(p) && !/\bA\b/.test(p)), JSON.stringify(prompts));
  check("  the results' buttons: Cross, Circle", (await data('results')) === 'Fight again (Cross) · Back to the menu (Circle)', `${await data('state')}, ${await data('results')}`);
  await shot('7-results');
  await browser.key('g');
  await sleep(200);
  check("  a key pressed: the keyboard's keys", (await data('device')) === 'keys' && (await data('results')) === 'Fight again (Z) · Back to the menu (X)', `${await data('device')}, ${await data('results')}`);
  await sleep(400);
  await tap(B.east);
  await state('menu', 'the menu after the battle').catch(() => {});
  check('  Circle: back to the menu', (await data('state')) === 'menu' && (await data('device')) === 'pad', `${await data('state')}, ${await data('device')}`);

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
