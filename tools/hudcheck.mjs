// The overworld's HUD in the built site: its buttons in their groups (Settings left of Exit,
// top right; the Team bottom left), frosted; Esc no longer leaving (it puts the ring on the
// buttons); the Team's list (opening beside the Team, closing with the Team again, a click
// anywhere else, Settings, View all), always a main (the team's first in a new game), pinned
// first, staying when clicked again, the creatures that fought most in the last battles next;
// View all's board of cards (the list's five first; sorted in its dropdown, reversed, only
// some types in theirs, All and None at its top, the main first all the same; a card's pin,
// shown while it's hovered, pinning it; a card clicked opening its details: thumbnails, a
// move's levels, Esc back to the board), with the controller too; one lost for good (no
// lifeforce left) shown so and sent into no battle;
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
const B = { south: 0, east: 1, lb: 4, rb: 5, start: 9, up: 12, down: 13, left: 14, right: 15 };

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
  check('  the map named top left, the keys under it', (await data('hudhint')) === 'Arrows move  ·  Shift runs  ·  Z speaks  ·  Esc: the buttons', await data('hudhint'));
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

  // 2b. the Bag: its window from the Bag button (bottom left, by the Team): the pockets'
  // tabs, the usual few items in Consumables, the IDPs in their dispenser on the side; a
  // stack dragged to free cells moves there; the Items tab; Close
  const [bx, by] = await at('uiHudBag');
  check('the Bag bottom left, by the Team', bx > tx && by === ty, `${bx},${by} (the Team ${tx},${ty})`);
  await press('uiHudBag', 500);
  check('  its window open on Consumables', (await data('hudbagwindow')) === 'open' && (await data('bagtab')) === 'consumables', `${await data('hudbagwindow')} ${await data('bagtab')}`);
  const bag0 = await data('bag');
  check('  the usual few in it: potions and the like in Consumables, IDPs in the dispenser', /consumables [1-9]\d*\/30/.test(bag0) && /idps [1-9]\/4/.test(bag0), bag0);
  await shot('2b-bag');
  const [s0x, s0y] = await at('uiBagSlot_consumables_0');
  await browser.drag([[s0x, s0y], [s0x + 4, s0y + 30], [s0x, s0y + 49 * 2], [s0x, s0y + 49 * 3]]);
  await sleep(400);
  const [s1x, s1y] = await at('uiBagSlot_consumables_0');
  check('  a stack dragged down to free cells moves there', Math.abs(s1x - s0x) < 10 && s1y > s0y + 100, `${s0x},${s0y} -> ${s1x},${s1y} · ${await data('bag')}`);
  await press('uiBagTab_items', 300);
  check('  the Items tab', (await data('bagtab')) === 'items', await data('bagtab'));
  await press('uiBagClose', 400);
  check('  closed', (await data('hudbagwindow')) === '', await data('hudbagwindow'));

  // 3. the Team's list: opens beside the Team; closes with the Team again
  const all = (await data('hudteamall')).split(',').filter(Boolean);
  // the one the story says is lost for good (part 6), none before
  let lostOne = '';
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
    let main = (await data('hudteamshown')).split(',')[1];
    await press('uiTeam_1');
    check('  another pinned', (await data('hudmain')) === main, `${await data('hudmain')} (${main})`);

    // 5. View all: the window opens, the list closes; a board of cards, every creature, the
    // list's five first; by level the highest first, reversed the lowest, by a type only
    // those (the main first all the same); a card clicked pinned as the main, first
    const listed = (await data('hudteamshown')).split(',');
    await press('uiTeamAll', 400);
    check('View all: its window, the list closed', (await open('hudteamwindow')) && !(await open('hudteam')));
    const board = async () => (await data('hudboard')).split(',').filter(Boolean);
    const levels = async () => (await data('hudboardlevels')).split(',').filter(Boolean).map(Number);
    const first = await board();
    check("  every creature, the first row the list's five", first.length === all.length && first.slice(0, 5).join() === listed.slice(0, 5).join(), `${first} (${listed})`);
    await shot('5-view-all');
    await press('uiTeamSort');
    check("  the sort's dropdown open", (await data('hudboardpopup')) === 'sort', await data('hudboardpopup'));
    await shot('5a-sort');
    await press('uiTeamSort_Level');
    check('    a sort picked: it closes', (await data('hudboardpopup')) === '', await data('hudboardpopup'));
    const cards = await board();
    let lv = (await levels()).slice(1);
    check('  by level: the main first, then the highest', (await data('hudboardsort')) === 'Level' && cards[0] === main && lv.every((l, i) => i === 0 || l <= lv[i - 1]), `${cards} ${lv}`);
    await press('uiTeamReverse');
    lv = (await levels()).slice(1);
    check('    reversed: the main first, then the lowest', (await data('hudboardsort')) === 'Level reversed' && (await board())[0] === main && lv.every((l, i) => i === 0 || l >= lv[i - 1]), `${await board()} ${lv}`);
    await press('uiTeamReverse');
    await press('uiTeamSort');
    await press('uiTeamSort_Mostused');
    check('    most used: as when it opened', (await board()).join() === first.join(), `${await board()} (${first})`);
    // the types' dropdown: None, the main alone; one type checked, those; All, every one;
    // Esc closes the dropdown, not the board
    await press('uiTeamTypes');
    check("  the types' dropdown open", (await data('hudboardpopup')) === 'types', await data('hudboardpopup'));
    const everyType = await data('hudboardtypes');
    await press('uiTeamTypes_None');
    check('    None: the main alone', (await board()).join() === main && (await data('hudboardtypes')) === '', `${await board()}`);
    const types = await browser.evaluate("Object.keys(document.body.dataset).filter((k) => k.startsWith('uiTeamType_'))");
    if (types.length) {
      await press(types[0]);
      const only = await board();
      check(`    one type checked (${types[0].slice(11)}): those, the main first, the dropdown still open`, only[0] === main && only.length <= all.length && (await data('hudboardtypes')) !== '' && (await data('hudboardpopup')) === 'types', `${only}`);
      await shot('5b-one-type');
    }
    await press('uiTeamTypes_All');
    check('    All: every one again', (await board()).length === all.length && (await data('hudboardtypes')) === everyType, `${await board()}`);
    await browser.key('Escape');
    await sleep(300);
    check('    Esc: the dropdown closed, the board still open', (await data('hudboardpopup')) === '' && (await open('hudteamwindow')), `${await data('hudboardpopup')} ${await data('hudteamwindow')}`);
    // a card's pin: shown (grey) while the card is hovered; pressed, that one is the main,
    // first
    const lastCard = (await board())[all.length - 1];
    const [lx, ly] = await at(`uiTeamCard_${all.length - 1}`);
    await browser.move(lx, ly);
    await sleep(350);
    await press(`uiTeamPin_${all.length - 1}`);
    check("  a card's pin (shown while it's hovered): pinned as the main, first", (await data('hudmain')) === lastCard && (await board())[0] === lastCard && (await data('hudboarddetail')) === '', `${await data('hudmain')} (${lastCard}) ${await board()}`);
    main = lastCard;
    // a card clicked: its details, the cards turned into thumbnails; a move picked shows its
    // levels; another thumbnail, its details; Esc: the board again
    const second = (await board())[1];
    await press('uiTeamCard_1', 700);
    check('  a card clicked: its details, not pinned', (await data('hudboarddetail')) === second && (await data('hudmain')) === main, `${await data('hudboarddetail')} (${second})`);
    check('    its lifeforce: all of the gauge, never knocked out', (await data('hudboardlife')) === '100/100', await data('hudboardlife'));
    check('    the cards turned into thumbnails', (await data('uiTeamThumb_0')) !== '' && (await data('uiTeamBack')) !== '', await data('uiTeamThumb_0'));
    const moves = await browser.evaluate("Object.keys(document.body.dataset).filter((k) => k.startsWith('uiTeamMove_'))");
    if (moves.length >= 2) {
      await press('uiTeamMove_1', 300);
      const [mvName, mvLevel, mvLevels] = (await data('hudboardmove')).split(':');
      check(`    a move picked (${mvName}, Lv ${mvLevel}): its levels`, Number(mvLevels) >= 1 && Number(mvLevel) >= 1, await data('hudboardmove'));
    }
    await shot('5c-details');
    await press('uiTeamThumb_0', 400);
    check('    another thumbnail: its details', (await data('hudboarddetail')) === (await board())[0], `${await data('hudboarddetail')} (${(await board())[0]})`);
    await browser.key('Escape');
    await sleep(700);
    check('    Esc: the board again, still open', (await data('hudboarddetail')) === '' && (await open('hudteamwindow')), `${await data('hudboarddetail')} ${await data('hudteamwindow')}`);
    await press('uiTeamWindowClose');
    check('  Close: gone', !(await open('hudteamwindow')));

    // 6. the creatures that fought most in the last battles come next, the main first; the
    // story keeps it all
    const story = JSON.parse(await browser.evaluate('localStorage.getItem("story")') || '{}');
    check('the story keeps the main', story.team?.main === main, JSON.stringify(story.team));
    const late = all[all.length - 1];
    // and one lost for good (no lifeforce left): neither the main nor next in the list
    lostOne = all.find((id, k) => k >= 3 && id !== main && id !== late) ?? '';
    story.team = { battles: [[all[2], late], [all[2]], [late], [late, all[0]]], main, lifeforce: lostOne ? { [lostOne]: 0 } : {} };
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
    if (lostOne) {
      await press('uiTeamAll', 500);
      check(`  ${lostOne}, out of lifeforce: lost for good on the board`, (await data('hudboardlost')) === lostOne, await data('hudboardlost'));
      await shot('5d-lost');
      await press('uiTeamWindowClose');
    } else {
      await press('uiHudTeam', 450);
    }
  } else {
    check('no team to pin: the list says so', all.length === 0 || shown.length === all.length, `${all}`);
  }

  // 7. a controller: Start puts the ring on Settings; down, the Team; A opens the list (the
  // ring in it); A pins; B closes the list (the ring back on the Team), B again takes it off
  await pads(`connect(0, ${JSON.stringify(XBOX)})`);
  await sleep(500);
  check('a controller connected: listened to', (await data('device')) === 'pad', await data('device'));
  check("  the keys named as it has them", (await data('walkhint')) === 'D-pad moves  ·  B runs  ·  A speaks  ·  Menu: the buttons', await data('walkhint'));
  await tap(B.start);
  check('  Start: the ring on Settings', (await data('hudring')) === 'uiHudSettings', await data('hudring'));
  await tap(B.down);
  check('  down: the Bag, the nearest below', (await data('hudring')) === 'uiHudBag', await data('hudring'));
  await tap(B.left);
  check('  left: the Team', (await data('hudring')) === 'uiHudTeam', await data('hudring'));
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
  if (all.length >= 2) {
    // the board with the controller: A on the Team, right along the list to View all, A: the
    // board, the ring on its first card (that A pins nothing); right: the next card; RB: by
    // level; LB: back; B: closed, the ring back on the Team
    await tap(B.south);
    await sleep(400);
    for (let i = 0; i < Math.min(5, all.length); i++) await tap(B.right);
    check('  A, right along the list: View all', (await data('hudring')) === 'uiTeamAll', await data('hudring'));
    const before = await data('hudmain');
    await tap(B.south);
    await sleep(400);
    check('  A: the board, the ring on its first card, nothing pinned', (await open('hudteamwindow')) && (await data('hudboardring')) === 'uiTeamCard_0' && (await data('hudmain')) === before, `${await data('hudboardring')} ${await data('hudmain')}`);
    await tap(B.right);
    check('    right: the next card', (await data('hudboardring')) === 'uiTeamCard_1', await data('hudboardring'));
    await tap(B.rb);
    check('    RB: by level', (await data('hudboardsort')) === 'Level', await data('hudboardsort'));
    await tap(B.lb);
    check('    LB: most used again', (await data('hudboardsort')) === 'Most used', await data('hudboardsort'));
    // up to the controls, along to the types' dropdown; A: open, the ring on All; right, A:
    // None (the main alone); left, A: All; B: the dropdown closed, the board still open
    await tap(B.up);
    const xOf = async (name) => Number((await data(name)).split(',')[0]);
    const typesX = await xOf('uiTeamTypes');
    for (let i = 0; i < 6 && (await data('hudboardring')) !== 'uiTeamTypes'; i++) {
      await tap((await xOf(await data('hudboardring'))) < typesX ? B.right : B.left);
    }
    check("    up, along: the types' dropdown", (await data('hudboardring')) === 'uiTeamTypes', await data('hudboardring'));
    await tap(B.south);
    await sleep(300);
    check('    A: it opens, the ring on All', (await data('hudboardpopup')) === 'types' && (await data('hudboardring')) === 'uiTeamTypes_All', `${await data('hudboardpopup')} ${await data('hudboardring')}`);
    await tap(B.right);
    await tap(B.south);
    check('    right, A: None, the main alone', (await data('hudboard')).split(',').length === 1, await data('hudboard'));
    await tap(B.left);
    await tap(B.south);
    check('    left, A: All, every one', (await data('hudboard')).split(',').length === all.length, await data('hudboard'));
    await tap(B.east);
    check('    B: the dropdown closed, the board still open, the ring on its button', (await data('hudboardpopup')) === '' && (await open('hudteamwindow')) && (await data('hudboardring')) === 'uiTeamTypes', `${await data('hudboardpopup')} ${await data('hudboardring')}`);
    // down onto a card; A: its details, the ring on its thumbnail; B: the board again, the
    // ring back on its card
    await tap(B.down);
    const onCard = await data('hudboardring');
    check('    down: onto a card', onCard.startsWith('uiTeamCard_'), onCard);
    await tap(B.south);
    await sleep(600);
    check('    A: its details, the ring on its thumbnail', (await data('hudboarddetail')) !== '' && (await data('hudboardring')).startsWith('uiTeamThumb_'), `${await data('hudboarddetail')} ${await data('hudboardring')}`);
    await shot('6c-details-pad');
    await tap(B.east);
    await sleep(600);
    check('    B: the board again, the ring back on its card', (await data('hudboarddetail')) === '' && (await open('hudteamwindow')) && (await data('hudboardring')) === onCard, `${await data('hudboarddetail')} ${await data('hudboardring')}`);
    await shot('6b-board-pad');
    await tap(B.east);
    check('    B: closed, the ring back on the Team', !(await open('hudteamwindow')) && (await data('hudring')) === 'uiHudTeam', `${await data('hudteamwindow')} ${await data('hudring')}`);
  }
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
  // (as few as the screen's size allows, times the camera's zoom: the overworld's camera
  // stands back, so its pixels are smaller there than at the distance they're set for)
  const fewest = Math.round(Math.sqrt((W * H) / (160 * 144)));
  const zoom = Number(await data('pixelzoom'));
  const k = Number(await data('pixel'));
  const plain = cells(await browser.pixels(join(shots, 'hud-7-pixels.png')), k);
  check(`  at density 0, the fewest: squares of ${k} pixels, one colour each`, k === Math.max(1, Math.round(Math.min(Math.sqrt((W * H) / (160 * 144)) * zoom, Math.sqrt((W * H) / (160 * 144))))) && plain.even > 0.97, `${(plain.even * 100).toFixed(1)}% (${fewest} a side at zoom 1, ${zoom} here)`);
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
    check(`  the others after it, in the team's order${lostOne ? ` (not ${lostOne}, lost for good)` : ''}`, party.slice(1).join() === all.filter((id) => id !== main && id !== lostOne).join(), `${party}`);
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
