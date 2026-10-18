// The overworld's HUD in the built site: its buttons in their groups (Settings left of Exit,
// top right; the Team bottom left), frosted; Esc no longer leaving (it puts the ring on the
// buttons); the Team's list (opening beside the Team, closing with the Team again, a click
// anywhere else, Settings, View all), always a main (the team's first in a new game), pinned
// first, staying when clicked again, the creatures that fought most in the last battles next;
// the story keeping them; Exit back to the menu; a controller working all of it (Start, the
// d-pad, A, B). And the pixelation from the Options (a category of them closing and opening):
// the world in big square pixels, as dense as asked, bigger as the camera comes closer and
// smaller as it pulls back; an LCD's gaps between them. Last, a battle from the overworld (an
// event the check puts on the start map, with the placeholders' first wild encounter): the
// main sent out first.
// Screenshots to screenshots/hud-*.png. Exits 1 when something fails.
//     node tools/hudcheck.mjs
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
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

// a stand-in controller, as the Gamepad API has one ("standard" layout: 0 South, 1 East,
// 9 Start, 12 to 15 the d-pad up, down, left, right)
const STAND_INS = `(() => {
  const slots = [null, null, null, null];
  window.__pads = {
    connect(index, id) {
      slots[index] = { id, index, connected: true, mapping: 'standard', timestamp: performance.now(),
        buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })), axes: [0, 0, 0, 0] };
    },
    disconnect(index) { slots[index] = null; },
    set(index, button, down) {
      const b = slots[index].buttons[button];
      b.pressed = down; b.touched = down; b.value = down ? 1 : 0;
      slots[index].timestamp = performance.now();
    },
  };
  Object.defineProperty(Navigator.prototype, 'getGamepads', { value: function () { return slots.slice(); }, configurable: true, writable: true });
})();`;
const XBOX = 'Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)';
const B = { south: 0, east: 1, start: 9, up: 12, down: 13, left: 14, right: 15 };

const W = 1400;
const H = 900;
// a battle the check can start from the overworld: an event on the cell west of where games
// start, its encounter the placeholders' first wild one (none without them: no battle then)
const FIGHT = [18, 22];
const temporary = join(root, 'temporary', 'bundle.json');
const wild = existsSync(temporary)
  ? Object.entries(JSON.parse(readFileSync(temporary, 'utf8')).files).find(([path, e]) => path.startsWith('encounters/') && e.kind === 'wild')?.[1].id
  : undefined;
const rewrite = (path, body) => {
  if (path !== 'content/bundle.json' || !wild) return body;
  const bundle = JSON.parse(body.toString('utf8'));
  const map = bundle.files['maps/test_meadow.json'];
  map.events = [...(map.events ?? []), { id: 'check_fight', trigger: 'enter', area: [...FIGHT, 1, 1], script: [`battle ${wild}`] }];
  return Buffer.from(JSON.stringify(bundle));
};
const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: W, height: H, port: 9502, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(250);
};
const shot = (name) => browser.screenshot(join(shots, `hud-${name}.png`));
const at = async (name) => (await data(name)).split(',').map(Number);
const press = async (name, wait = 300) => {
  const [x, y] = await at(name);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(wait);
};
const state = (s, what) => browser.waitFor(`document.body.dataset.state === ${JSON.stringify(s)}`, { timeout: 30000, what });
const pads = (js) => browser.evaluate(`(window.__pads.${js}, true)`);
const tap = async (button) => {
  await pads(`set(0, ${button}, true)`);
  await sleep(120);
  await pads(`set(0, ${button}, false)`);
  await sleep(200);
};
const load = async () => {
  await browser.goto(url);
  await state('menu', 'the main menu');
  await sleep(600);
};
const explore = async (what) => {
  await state('explore', what);
  await browser.waitFor('document.body.dataset.frost === "1" && document.body.dataset.loading === "0"', { timeout: 30000, what: 'the world and its glass' });
  await sleep(800);
};
const open = (name) => data(name).then((v) => v === 'open');

// The screen in cells of `k` pixels from the top left: how many cells in the middle of the
// screen are one colour (inside a pixel's rim: `inset`), and how much darker their edges are
// than their middles.
const cells = (img, k, inset = 1) => {
  const px = (x, y) => {
    const i = (y * img.width + x) * 4;
    return [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]];
  };
  const lum = (c) => c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11;
  let even = 0;
  let all = 0;
  let edges = 0;
  let middles = 0;
  for (let cy = Math.floor(H * 0.3 / k); cy < Math.floor(H * 0.7 / k); cy += 3) {
    for (let cx = Math.floor(W * 0.3 / k); cx < Math.floor(W * 0.7 / k); cx += 3) {
      const inner = [];
      for (let y = inset; y < k - inset; y++) for (let x = inset; x < k - inset; x++) inner.push(px(cx * k + x, cy * k + y).join());
      all++;
      if (new Set(inner).size === 1) even++;
      edges += lum(px(cx * k, cy * k + (k >> 1)));
      middles += lum(px(cx * k + (k >> 1), cy * k + (k >> 1)));
    }
  }
  return { even: even / all, edges: edges / all, middles: middles / all };
};

try {
  await browser.viewport(W, H);
  await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: STAND_INS });
  await load();
  await browser.evaluate('(localStorage.clear(), true)');
  await load();
  await press('uiNewGame');
  await explore('the new game');

  // 1. the buttons in their groups, frosted
  const [sx, sy] = await at('uiHudSettings');
  const [ex, ey] = await at('uiHudExit');
  const [tx, ty] = await at('uiHudTeam');
  check('Settings and Exit top right, Settings on the left', sx < ex && sy === ey && sy < 120 && ex > W - 120, `${sx},${sy} ${ex},${ey}`);
  check('  the Team bottom left', tx < 120 && ty > H - 120, `${tx},${ty}`);
  check('  frosted: the world blurred behind them', (await data('frost')) === '1');
  check('  the map named top left, the keys under it', (await data('hudhint')) === 'Arrows move  ·  Z speaks  ·  Esc: the buttons', await data('hudhint'));
  await shot('1-overworld');

  // 2. Esc doesn't leave: it puts the ring on the buttons (Settings), the arrows move it, the
  // world waits; Esc again takes it off
  const walker = await data('walker');
  await browser.key('Escape');
  await sleep(300);
  check('Esc: still in the world', (await data('state')) === 'explore');
  check('  the ring on Settings', (await data('hudring')) === 'uiHudSettings', await data('hudring'));
  check('  the keys for the ring', (await data('hudhint')) === 'Arrows: choose  ·  Z: press  ·  X: back', await data('hudhint'));
  await browser.key('ArrowRight');
  check('  right: Exit', (await data('hudring')) === 'uiHudExit', await data('hudring'));
  await browser.key('ArrowLeft');
  await browser.key('ArrowUp');
  await sleep(500);
  check('  the walker stays while the ring is on', (await data('walker')) === walker, `${walker} -> ${await data('walker')}`);
  await shot('2-ring');
  await browser.key('Escape');
  check('  Esc again: the ring off', (await data('hudring')) === '' && (await data('state')) === 'explore');

  // 3. the Team's list: opens beside the Team; closes with the Team again
  const all = (await data('hudteamall')).split(',').filter(Boolean);
  await press('uiHudTeam', 450);
  check('the Team: its list open', await open('hudteam'));
  const shown = (await data('hudteamshown')).split(',').filter(Boolean);
  check('  five creatures at most, the team in its order before any battle', shown.length === Math.min(5, all.length) && shown.join() === all.slice(0, 5).join(), `${shown} of ${all}`);
  const [ax] = await at('uiTeamAll');
  check('  View all at its end, right of the Team', ax > tx, `${ax} > ${tx}`);
  await shot('3-team');
  await press('uiHudTeam', 450);
  check('  the Team again: closed', !(await open('hudteam')));
  // a click anywhere else closes it
  await press('uiHudTeam', 450);
  await browser.click(W / 2, H / 2);
  await sleep(300);
  check('  a click on the world: closed', !(await open('hudteam')));
  // Settings too (the Options open)
  await press('uiHudTeam', 450);
  await press('uiHudSettings', 500);
  check('  Settings: closed, the Options open', !(await open('hudteam')) && (await open('hudoptions')));
  await press('uiOptionsDone');
  check('  Done: the Options closed', !(await open('hudoptions')));

  if (all.length >= 3) {
    // 4. the main: in a new game the team's first; a creature clicked is pinned, first;
    // clicked again, it stays (there's always a main)
    check('a new game: its first creature the main', (await data('hudmain')) === all[0], `${await data('hudmain')} (${all[0]})`);
    await press('uiHudTeam', 450);
    await press('uiTeam_2');
    check('a creature clicked: pinned as the main', (await data('hudmain')) === all[2], `${await data('hudmain')} (${all[2]})`);
    check('  first in the list', (await data('hudteamshown')).split(',')[0] === all[2], await data('hudteamshown'));
    check('  the list stays open', await open('hudteam'));
    await sleep(400);
    const [hx, hy] = await at('uiTeam_0');
    await browser.move(hx, hy);
    await sleep(300);
    check('  its caption: the main, sent out first', (await data('hudcaption')).includes('your main, sent out first'), await data('hudcaption'));
    await shot('4-main');
    await press('uiTeam_0');
    check('  clicked again: still the main', (await data('hudmain')) === all[2] && (await data('hudteamshown')).split(',')[0] === all[2], await data('hudteamshown'));
    const main = (await data('hudteamshown')).split(',')[1];
    await press('uiTeam_1');
    check('  another pinned', (await data('hudmain')) === main, `${await data('hudmain')} (${main})`);

    // 5. View all: the window opens, the list closes
    await press('uiTeamAll', 400);
    check('View all: its window, the list closed', (await open('hudteamwindow')) && !(await open('hudteam')));
    await shot('5-view-all');
    await press('uiTeamWindowClose');
    check('  Close: gone', !(await open('hudteamwindow')));

    // 6. the creatures that fought most in the last battles come next, the main first; the
    // story keeps it all
    const story = JSON.parse(await browser.evaluate('localStorage.getItem("story")') || '{}');
    check('the story keeps the main', story.team?.main === main, JSON.stringify(story.team));
    const late = all[all.length - 1];
    story.team = { battles: [[all[2], late], [all[2]], [late], [late, all[0]]], main };
    await press('uiHudExit');
    await state('menu', 'the menu');
    check('Exit: back on the menu', (await data('state')) === 'menu');
    await browser.evaluate(`(localStorage.setItem("story", ${JSON.stringify(JSON.stringify(story))}), true)`);
    await load();
    await press('uiContinue');
    await explore('the story again');
    await press('uiHudTeam', 450);
    const ranked = [...new Set([late, all[2], all[0]])].filter((id) => id !== main);
    const expect = [main, ...ranked, ...all.filter((id) => id !== main && !ranked.includes(id))].slice(0, 5);
    check('  the main first, then who fought most (the latest first)', (await data('hudteamshown')) === expect.join(), `${await data('hudteamshown')} (${expect})`);
    await press('uiHudTeam', 450);
  } else {
    check('no team to pin: the list says so', all.length === 0 || shown.length === all.length, `${all}`);
  }

  // 7. a controller: Start puts the ring on Settings; down, the Team; A opens the list (the
  // ring in it); A pins; B closes the list (the ring back on the Team), B again takes it off
  await pads(`connect(0, ${JSON.stringify(XBOX)})`);
  await sleep(500);
  check('a controller connected: listened to', (await data('device')) === 'pad', await data('device'));
  check("  the keys named as it has them", (await data('walkhint')) === 'D-pad moves  ·  A speaks  ·  Menu: the buttons', await data('walkhint'));
  await tap(B.start);
  check('  Start: the ring on Settings', (await data('hudring')) === 'uiHudSettings', await data('hudring'));
  await tap(B.down);
  check('  down: the Team', (await data('hudring')) === 'uiHudTeam', await data('hudring'));
  await tap(B.south);
  await sleep(400);
  check('  A: the list open, the ring on its first', (await open('hudteam')) && (await data('hudring')) === (all.length ? 'uiTeam_0' : 'uiHudTeam'), `${await data('hudteam')} ${await data('hudring')}`);
  if (all.length >= 2) {
    await tap(B.right);
    const second = (await data('hudteamshown')).split(',')[1];
    check('  right: its caption names A', (await data('hudcaption')).includes('A: pin as your main'), await data('hudcaption'));
    await tap(B.south);
    check('  A: the second pinned', (await data('hudmain')) === second, `${await data('hudmain')} (${second})`);
    check('  its caption: the main now', (await data('hudcaption')).includes('your main, sent out first'), await data('hudcaption'));
  }
  await shot('6-pad');
  await tap(B.east);
  check('  B: the list closed, the ring on the Team', !(await open('hudteam')) && (await data('hudring')) === 'uiHudTeam', `${await data('hudteam')} ${await data('hudring')}`);
  await tap(B.up);
  await tap(B.right);
  check('  up, right: Exit', (await data('hudring')) === 'uiHudExit', await data('hudring'));
  await tap(B.east);
  check('  B: the ring off', (await data('hudring')) === '');
  await pads('disconnect(0)');
  await sleep(400);
  await browser.click(W / 2, H / 2);
  await sleep(200);

  // 8. the pixelation: Settings, Graphics, its category (closing it and opening it again), on:
  // the world in squares of whole pixels; at density 0 the fewest (about as many as the GBC's
  // screen had); an LCD's gaps between them
  await press('uiHudSettings', 500);
  await press('uiTab_Graphics');
  await press('uiSection_Postprocessing', 500);
  check('Post-processing closed', (await data('optionsclosed')) === 'Post-processing', await data('optionsclosed'));
  await press('uiSection_Postprocessing', 600);
  check('  and open again', (await data('optionsclosed')) === '', await data('optionsclosed'));
  await press('uiPixelation_On', 800);
  check('  the pixelation on, as dense as it comes', (await data('retro')) === '0.35/off', await data('retro'));
  await press('uiOptionsDone', 800);
  await cmd('pixelation:0');
  await sleep(600);
  const fewest = Math.round(Math.sqrt((W * H) / (160 * 144)));
  const k = Number(await data('pixel'));
  const plain = cells(await browser.pixels(join(shots, 'hud-7-pixels.png')), k);
  check(`  at density 0, the fewest: squares of ${k} pixels, one colour each`, k === fewest && plain.even > 0.97, `${(plain.even * 100).toFixed(1)}% (${fewest} a side)`);
  await press('uiHudSettings', 500);
  await press('uiLcd_GBA', 800);
  check("  the GBA's LCD", (await data('retro')) === '0.00/gba', await data('retro'));
  await press('uiOptionsDone', 800);
  const lcd = cells(await browser.pixels(join(shots, 'hud-8-lcd.png')), k);
  check('  the gaps between its pixels: their edges darker', lcd.edges < lcd.middles * 0.975, `edges ${lcd.edges.toFixed(1)}, middles ${lcd.middles.toFixed(1)}`);
  await cmd('lcd:gbc');
  await sleep(600);
  await shot('9-gbc');
  check("  the GBC's LCD", (await data('retro')) === '0.00/gbc', await data('retro'));
  // kept with the options, as they were when the Options last changed one (the density the
  // check set, the GBA's LCD)
  const kept = JSON.parse((await browser.evaluate('localStorage.getItem("options")')) || '{}');
  check('  kept with the options', kept.pixelation === true && kept.pixel_density === 0 && kept.lcd === 'gba', JSON.stringify(kept));
  // as dense as asked: the camera closer, bigger pixels; further out, smaller (what's far
  // keeps its shape); none at all far enough out
  await cmd('lcd:off');
  await cmd('pixelation:0.5');
  await sleep(600);
  const mid = Number(await data('pixel'));
  await browser.move(W / 2, H / 2);
  for (let i = 0; i < 6; i++) await browser.wheel(W / 2, H / 2, -400);
  await sleep(1200);
  const near = Number(await data('pixel'));
  const close = cells(await browser.pixels(join(shots, 'hud-10-closer.png')), near);
  check(`  density 0.5: ${mid} pixels a side; the camera closer, bigger: ${near}`, near > mid && close.even > 0.97, `${(close.even * 100).toFixed(1)}%`);
  for (let i = 0; i < 12; i++) await browser.wheel(W / 2, H / 2, 400);
  await sleep(1200);
  const far = Number(await data('pixel'));
  await shot('11-further');
  check(`  further out, smaller: ${far}`, far < mid, `${far}`);
  await cmd('pixelation:0');
  await sleep(600);
  const farK = Number(await data('pixel'));
  const farCells = cells(await browser.pixels(), farK, 0);
  check(`  there at density 0: squares of ${farK}, smaller than the usual distance's ${k}`, farK < k && farK > 1 && farCells.even > 0.97, `${(farCells.even * 100).toFixed(1)}%`);
  await cmd('pixelation:off');
  await sleep(600);
  const back = cells(await browser.pixels(), k);
  check("  off: the screen's own pixels again", back.even < 0.9 && (await data('pixel')) === '1', `${(back.even * 100).toFixed(1)}%, ${await data('pixel')}`);

  // 9. a battle from the overworld: the main sent out first, the others after it in the
  // team's order
  if (wild && all.length >= 2) {
    const main = await data('hudmain');
    await cmd(`walkto:${FIGHT.join(',')}`);
    await state('battle', 'the battle');
    const party = (await data('battleparty')).split(',');
    check('a battle from the overworld: the main sent out first', party[0] === main, `${party} (${main})`);
    check("  the others after it, in the team's order", party.slice(1).join() === all.filter((id) => id !== main).join(), `${party}`);
  }
} catch (e) {
  console.log(`FAIL ${e.message}`);
  failed++;
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
