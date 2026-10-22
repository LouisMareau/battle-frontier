// The map editor's panels in the built site: as they first come (the Toolbox left, the
// Inspector right, the Assets along the bottom, the rest closed, a toggle each at the top bar's
// right end); the Inspector floated (a window, moved by its title bar), docked again on the
// side it's nearest; a panel closed by its header and opened by its toggle; a toggle
// right-clicked pinging its panel (a closed one opened); the toggles' names folded away; all of
// it as it was left after the page is loaded again; what's selected (props of each kind,
// the buildings, the map itself) fitting the Inspector as it first comes; and the panels
// dragged: a docked one's header let go on the view's left side (docked there, inside the
// Toolbox), then out in the middle of the view (a window there), the window's title bar let
// go at the view's foot (docked along the bottom), kept so after a reload. Screenshots to
// screenshots/layout-*.png. Exits 1 when something fails.
//     node tools/layoutcheck.mjs
import { mkdirSync } from 'node:fs';
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

const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9511, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(300);
};
const at = async (name) => (await data(name)).split(',').map(Number);
const press = async (name, wait = 500, button = 'left') => {
  const [x, y] = await at(name);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y, button);
  await sleep(wait);
};
const shot = (name) => browser.screenshot(join(shots, `layout-${name}.png`));
// each panel's place, from the layout's mark ("inspector=right")
const place = async (key) => (await data('layout')).split(' ').find((p) => p.startsWith(`${key}=`))?.split('=')[1];
const editor = async () => {
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.map === "test_meadow"', { timeout: 60000, what: 'the map editor' });
  await sleep(1500);
  if (await data('uiWelcomeOk')) await press('uiWelcomeOk', 600);
};

try {
  await browser.viewport(1400, 900);
  await editor();

  // 1. as it first comes
  check('as it first comes: the Toolbox left, the Inspector right, the Assets along the bottom, the rest closed', (await data('layout')) === 'toolbox=left inspector=right assets=bottom world=- timeline=- requests=- post=- abilities=- builds=-', await data('layout'));
  const marks = ['uiToolbox', 'uiInspector', 'uiAssets', 'uiWorld', 'uiTimeline', 'uiRequests', 'uiPost', 'uiAbilities', 'uiBuilds'];
  const xs = [];
  for (const m of marks) xs.push((await at(m))[0]);
  check('  a toggle each, at the top bar\'s right end, in that order', xs.every((x, i) => x > 700 && (i === 0 || x > xs[i - 1])), `${xs}`);
  const [, top, , bottom] = (await data('viewrect')).split(',').map(Number);
  check('  the view between them', bottom - top > 300, await data('viewrect'));
  await shot('1-first');

  // 2. what's selected fits the Inspector as it first comes (Select, a click on it): a
  // stand-in (its wind's sliders), a model, one with a sound, a lamp, the buildings, the map
  await browser.click(700, 889); // the status bar: the canvas gets the keys
  await browser.key('1');
  check('the map itself fits the Inspector', (await data('inspectorfits')) === '1', await data('inspectorfits'));
  for (const [what, x, z, y, want] of [
    ['the signpost', 21.5, 25.5, 0.1, 'prop:'],
    ['the flag pole', 16.5, 12.5, 0.1, 'prop:'],
    ['the well', 11.6, 5.6, 0.1, 'prop:'],
    ['a lamp post', 14.5, 4.5, 0.1, 'prop:'],
    ['the cottage', 11, 10.5, 1, 'building:'],
    ['the corner shop', 35, 24.5, 1, 'building:'],
  ]) {
    await cmd(`view:${x},${z},16,0,58`);
    await cmd(`point:${x},${y},${z}`);
    await sleep(300);
    await browser.click(...(await at('point')));
    await sleep(600);
    check(`${what} selected`, (await data('selection')).startsWith(want), await data('selection'));
    check('  its rows fit the Inspector', (await data('inspectorfits')) === '1', await data('inspectorfits'));
    if (what === 'the signpost') await shot('2-prop');
  }

  // 3. the Inspector floated: a window where it was docked; moved by its title bar; docked
  // again on the side it's nearest
  await press('uiPaneFloat_inspector', 700);
  check('the Inspector floated', (await place('inspector')) === 'float', await data('layout'));
  const [dx0, dy0] = await at('uiPaneDock_inspector');
  // its title bar: above its dock button
  await browser.drag([[dx0 - 120, dy0 - 30], [dx0 - 300, dy0 + 40], [dx0 - 520, dy0 + 120]]);
  await sleep(600);
  const [dx1, dy1] = await at('uiPaneDock_inspector');
  check('  its window moved by its title bar', dx1 < dx0 - 300 && dy1 > dy0 + 60, `${dx0},${dy0} -> ${dx1},${dy1}`);
  await shot('3-floated');
  await press('uiPaneDock_inspector', 700);
  check('  docked again, on the side it was nearest (the middle: along the bottom)', (await place('inspector')) === 'bottom', await data('layout'));
  await press('uiPaneFloat_inspector', 700);
  await browser.drag([[...(await at('uiPaneDock_inspector')).map((v, i) => v - [120, 30][i])], [1250, 120], [1300, 140]]);
  await sleep(600);
  await press('uiPaneDock_inspector', 700);
  check('  floated and dragged right, docked on the right', (await place('inspector')) === 'right', await data('layout'));

  // 4. the Assets closed by their header; opened by their toggle
  await press('uiPaneClose_assets', 500);
  check('the Assets closed by their header', (await place('assets')) === '-', await data('layout'));
  await press('uiAssets', 500);
  check('  opened by their toggle', (await place('assets')) === 'bottom', await data('layout'));

  // 5. pinged: a toggle right-clicked; a closed panel opened as it's pinged
  await press('uiToolbox', 150, 'right');
  check('a toggle right-clicked: its panel pinged', (await data('ping')) === 'toolbox', await data('ping'));
  await sleep(1100);
  await press('uiWorld', 250, 'right');
  check('  a closed one opened and pinged', (await place('world')) === 'bottom' && (await data('ping')) === 'world', `${await data('layout')} ${await data('ping')}`);
  await shot('5-ping');
  await sleep(1000);
  await press('uiWorld', 500);

  // 6. the toggles' names folded away: their icons only, narrower
  const [wide] = await at('uiToolbox');
  await press('uiPanesFold', 500);
  const [narrow] = await at('uiToolbox');
  check("the toggles' names folded away: their icons, narrower", (await data('layoutwords')) === '' && narrow > wide + 200, `${wide} -> ${narrow}`);
  await shot('6-folded');

  // 7. the page loaded again: all of it as it was left
  await press('uiPaneFloat_post', 0).catch(() => {});
  await press('uiPost', 700);
  await sleep(1200);
  const left = await data('layout');
  await editor();
  check('loaded again: as it was left', (await data('layout')) === left && (await data('layoutwords')) === '', `${await data('layout')} (left ${left})`);
  await shot('7-again');

  // 8. dragged and dropped: the Inspector's header taken over to the view's left side and
  // let go there (the target in from the edge): docked on the left, inside the Toolbox; taken
  // out to the middle of the view: a window where it's let go, its title bar under the
  // pointer; the window's title bar taken down to the view's foot: docked along the bottom;
  // and all of it kept
  const view = async () => (await data('viewrect')).split(',').map(Number);
  let [vl, vt, vr, vb] = await view();
  const [gx, gy] = await at('uiPaneGrip_inspector');
  const my = (vt + vb) / 2;
  await browser.drag([[gx, gy], [gx - 40, gy + 20], [(gx + vl) / 2, my], [vl + 60, my], [vl + 36, my]]);
  await sleep(700);
  check("the Inspector's header dragged to the view's left side: docked on the left", (await place('inspector')) === 'left' && (await data('dockzone')) === 'left', `${await data('layout')} zone ${await data('dockzone')}`);
  const [tx] = await at('uiPaneGrip_toolbox');
  const [ix] = await at('uiPaneGrip_inspector');
  check('  inside the Toolbox', ix > tx + 100, `toolbox ${tx}, inspector ${ix}`);
  await shot('8-left');
  [vl, vt, vr, vb] = await view();
  const mx = (vl + vr) / 2;
  const [gx2, gy2] = await at('uiPaneGrip_inspector');
  await browser.drag([[gx2, gy2], [gx2 + 30, gy2 + 30], [(gx2 + mx) / 2, (gy2 + my) / 2], [mx, my]]);
  await sleep(700);
  check('  dragged out to the middle of the view: a window there', (await place('inspector')) === 'float' && (await data('dockzone')) === '', `${await data('layout')} zone ${await data('dockzone')}`);
  const [dx, dy] = await at('uiPaneDock_inspector');
  check('  its title bar under the pointer', Math.abs(dy - my) < 70 && dx > mx - 100 && dx < mx + 320, `dock button ${dx},${dy}; let go at ${mx},${my}`);
  await shot('8-window');
  [vl, vt, vr, vb] = await view();
  await browser.drag([[dx - 120, dy - 30], [dx - 120, dy + 40], [mx, vb - 120], [mx, vb - 36]]);
  await sleep(700);
  check("  its window's title bar dragged to the view's foot: docked along the bottom", (await place('inspector')) === 'bottom' && (await data('dockzone')) === 'bottom', `${await data('layout')} zone ${await data('dockzone')}`);
  await shot('8-bottom');
  await sleep(800);
  const dropped = await data('layout');
  await editor();
  check('  kept so', (await data('layout')) === dropped, `${await data('layout')} (left ${dropped})`);
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
