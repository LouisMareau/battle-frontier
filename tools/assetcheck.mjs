// The asset browser, driven in a headless Chrome: opens it, shows a folder, drags a prop's
// row onto another folder (it's in both), makes a prop unstackable, finds the Requests button
// greyed out (no Claude sign-in: requestcheck.mjs makes a request, against a stand-in for
// Anthropic), drops a .glb from "Blender" on a folder (it becomes a prop there, its model
// loads, the Place tool puts it down), refuses an unstackable prop on another, and saves
// (downloads) what changed.
// The game's and the temporary assets apart; a model dropped on Creatures makes a creature
// record (its model standing for it); a character's free movement, walked off the grid; the
// category picked through the dropdown's search. Screenshots go to screenshots/assets-*.png.
//     node tools/assetcheck.mjs
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads');
const work = join(root, 'build', 'assetcheck');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
// a model "from Blender": the well, under another name
const model = join(work, 'Garden Lamp.glb');
copyFileSync(join(root, 'content', 'assets', 'models', 'stone_well.glb'), model);
// a picture for the Requests window
const picture = join(work, 'creature front.png');
copyFileSync(join(root, 'crates', 'game', 'assets', 'icon-192.png'), picture);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9489, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
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
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
  await sleep(150);
};
const shown = async () => (await data('shown')).split(',').filter(Boolean);
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `assets-${name}.png`));
};
const at = async (x, y, z) => {
  await cmd(`point:${x},${y},${z}`);
  await sleep(100);
  return (await data('point')).split(',').map(Number);
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  // saving downloads each file here, as this check reads them (the browser keeps them otherwise)
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html?saves=downloads`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  await cmd('view:20,12,16,0,55');

  // 1. open (it comes open, along the bottom), every prop listed, a folder shown
  if (!(await data('layout')).includes('assets=bottom')) await press('uiAssets');
  await sleep(300);
  const all = Number(await data('assets'));
  check('the browser lists every prop', all > 30 && (await shown()).length === all, `${all} props, ${(await shown()).length} shown`);
  await press('uiFolder_Buildings_Inside');
  await expect('a folder shown', 'folder', 'Buildings/Inside');
  const inside = await shown();
  check('its props', inside.includes('bed') && !inside.includes('flowers'), inside.join(' '));
  await shot('1-inside');

  // 2. flowers dragged onto Buildings/Inside: in both folders now
  await press('uiFolder_Nature');
  check('flowers are Nature', (await shown()).includes('flowers'));
  const [fx, fy] = await ui('uiAsset_flowers');
  const [tx, ty] = await ui('uiFolder_Buildings_Inside');
  await browser.drag([[fx, fy], [fx + 10, fy - 4], [fx - 60, fy - 20], [(fx + tx) / 2, (fy + ty) / 2], [tx, ty + 2], [tx, ty]]);
  await sleep(300);
  await press('uiFolder_Buildings_Inside');
  check('flowers dragged in', (await shown()).includes('flowers'), (await shown()).join(' '));
  await press('uiFolder_Nature');
  check('and still in Nature', (await shown()).includes('flowers'));
  await expect('one prop changed', 'assetsdirty', 1);
  await sleep(700);
  check('  marked as changed, unsaved: a dot by its row, one on the Assets toggle', (await data('uiChanged_props_flowers')) !== '' && (await data('editdots')).includes('assets:'), `row ${await data('uiChanged_props_flowers')} · dots ${await data('editdots')}`);

  // 3. flowers made unstackable
  await press('uiUnstackable_flowers');
  await expect('still one prop changed', 'assetsdirty', 1);
  await shot('2-dragged');

  // 3b. the Requests window: with no Claude sign-in (no API key in this browser) its button is
  // greyed out and opens nothing; a picture dropped meanwhile goes to the asset browser, which
  // takes only models, and becomes no prop
  await press('uiRequests');
  await expect('the Requests button greyed out: no Claude sign-in', 'requests', '');
  await expect('  the sign-in says so', 'claude', 'out');
  const before = Number(await data('assets'));
  await browser.drop([picture], 700, 450);
  for (let i = 0; i < 40 && !(await data('status')).includes('only glTF models'); i++) await sleep(100);
  check('  a picture dropped is no model, and why', (await data('status')).includes('only glTF models'), await data('status'));
  await expect('  and becomes no prop', 'assets', before);
  await shot('2b-requests');

  // 4. with the Requests window closed, a .glb dropped on the Buildings/Outside folder: a
  // prop, there, its model loading
  await press('uiFolder_Buildings_Outside');
  const models0 = Number(await data('models'));
  const [ox, oy] = await ui('uiFolder_Buildings_Outside');
  await browser.drop([model], ox, oy);
  await browser.waitFor('document.body.dataset.dropped === "Garden Lamp.glb"', { what: 'the dropped file read' });
  await sleep(500);
  await expect('one more prop', 'assets', all + 1);
  check('in Buildings/Outside', (await shown()).includes('garden_lamp'), (await shown()).join(' '));
  await browser.waitFor(`Number(document.body.dataset.models) > ${models0}`, { what: 'its model loaded' });
  check('its model loaded', true, `${await data('models')} models`);
  await shot('3-dropped');

  // 5. the Place tool puts it down (it was picked when it came in)
  await browser.click(700, 889);
  await browser.key('3');
  const props0 = Number(await data('props'));
  await browser.click(...(await at(18.0, 0, 12.0)));
  await expect('placed on the map', 'props', props0 + 1);
  // a barrel on the crate at 16.5, 8.5: neither stacks
  await press('uiAsset_barrel');
  await browser.click(...(await at(16.5, 0, 8.5)));
  await expect('a barrel on the crate: refused', 'props', props0 + 1);
  check('  and why', (await data('status')).startsWith('No room'), await data('status'));
  await shot('4-placed');

  // 5b. the placed lamp's record from the Select inspector: a light, and blocking off
  await browser.key('1');
  await browser.click(...(await at(18.0, 0.5, 12.0)));
  await sleep(300);
  check('the placed prop selected', (await data('selection')).startsWith('prop:'), await data('selection'));
  await press('uiPropLight');
  await expect('its light window', 'lightedit', 'garden_lamp');
  await press('uiLightGive');
  await sleep(200);
  await expect('two props changed', 'assetsdirty', 2);
  await press('uiPropBlocks');
  await sleep(200);
  // moved to another folder from the selection panel
  await press('uiPropMoveTo');
  await sleep(300);
  await press('uiMoveTo_Town');
  await sleep(200);
  await shot('5-prop');
  await browser.key('Escape');

  // 6. saved: the map, the two changed props' records and the model downloaded
  await browser.key('s', { ctrl: true });
  const got = (name) => existsSync(join(downloads, name));
  const wanted = ['test_meadow.json', 'flowers.json', 'garden_lamp.json', 'garden_lamp.glb'];
  for (let i = 0; i < 50 && !wanted.every(got); i++) await sleep(200);
  check('downloads', got('test_meadow.json') && got('flowers.json') && got('garden_lamp.json') && got('garden_lamp.glb'),
    ['test_meadow.json', 'flowers.json', 'garden_lamp.json', 'garden_lamp.glb'].map((n) => `${n}: ${got(n) ? 'yes' : 'no'}`).join(', '));
  if (got('flowers.json')) {
    const flowers = JSON.parse(readFileSync(join(downloads, 'flowers.json'), 'utf8'));
    check('flowers: unstackable, both folders', flowers.unstackable === true && flowers.folders?.includes('Buildings/Inside') && flowers.folders?.includes('Nature'), JSON.stringify(flowers.folders));
  }
  if (got('garden_lamp.json')) {
    const lamp = JSON.parse(readFileSync(join(downloads, 'garden_lamp.json'), 'utf8'));
    check('the new prop: measured, its model', lamp.footprint?.join('x') === '2x2' && lamp.model === 'models/garden_lamp.glb',
      `${lamp.footprint} · ${lamp.height} high · ${lamp.model}`);
    check('  moved from its drop folder to Town in the selection panel', lamp.folders?.length === 1 && lamp.folders[0] === 'Town', JSON.stringify(lamp.folders));
    check('  with the light and the blocking set from the inspector', lamp.light?.kind === 'spot' && lamp.blocks !== true, `light ${JSON.stringify(lamp.light)} · blocks ${lamp.blocks}`);
  }
  await expect('nothing left to save', 'assetsdirty', 0);
  await sleep(700);
  check('  no marks left once saved', (await data('editdots')) === '', `dots "${await data('editdots')}"`);

  // 7. every prop to Blender, in one file
  await press('uiExportProps');
  for (let i = 0; i < 50 && !got('props.glb'); i++) await sleep(200);
  const catalogue = got('props.glb') ? readFileSync(join(downloads, 'props.glb')) : Buffer.alloc(0);
  check('every prop exported for Blender', catalogue.length > 10000 && catalogue.subarray(0, 4).toString() === 'glTF', `${catalogue.length} bytes`);
  check('  with the lamps\' lights', catalogue.includes('KHR_lights_punctual') && catalogue.includes('lamp_post light'));

  // 8. the lamp renamed, its id too, from the pencil on its row (in Town, where it was
  // moved): its record downloaded under its new id and its model's file after it, the map
  // placing it written again naming it anew, its row's mark following
  await press('uiFolder_Town');
  await press('uiRename_garden_lamp');
  await sleep(500);
  const typeInto = async (name, text) => {
    await press(name);
    await browser.key('a', { ctrl: true });
    await browser.type(text);
  };
  await typeInto('uiRenameName', 'Garden lantern');
  await typeInto('uiRenameId', 'garden_lantern');
  await sleep(200);
  await press('uiRenameOk');
  await sleep(1500);
  check('the lamp renamed, its id too', (await data('renamed')) === 'garden_lamp->garden_lantern' && (await data('uiAsset_garden_lantern')) !== '', `${await data('renamed')} · ${await data('status')}`);
  for (let i = 0; i < 50 && !(got('garden_lantern.json') && got('garden_lantern.glb')); i++) await sleep(200);
  check("  its record under its new id, its model's file after it", got('garden_lantern.json') && got('garden_lantern.glb'), `record ${got('garden_lantern.json')}, model ${got('garden_lantern.glb')}`);
  if (got('garden_lantern.json')) {
    const lantern = JSON.parse(readFileSync(join(downloads, 'garden_lantern.json'), 'utf8'));
    check('  called so, its model named after it', lantern.name === 'Garden lantern' && lantern.model === 'models/garden_lantern.glb', `${lantern.name} · ${lantern.model}`);
  }
  const meadows = readdirSync(downloads).filter((f) => f.startsWith('test_meadow') && f.endsWith('.json'));
  check('  the map placing it written again, naming it anew', meadows.some((f) => readFileSync(join(downloads, f), 'utf8').includes('"garden_lantern"')), meadows.join(', '));
  await shot('8-renamed');

  // 9. the lantern deleted from the bin on its row: the dialog says it's placed on the map;
  // gone from the list, the map written again without it
  const before9 = Number(await data('assets'));
  await press('uiDelete_garden_lantern');
  await sleep(500);
  await press('uiDeleteOk');
  await sleep(1500);
  check('the lantern deleted, its use on the map with it', (await data('deleted')) === 'garden_lantern' && Number(await data('assets')) === before9 - 1, `${await data('deleted')} · ${await data('assets')} of ${before9} props · ${await data('status')}`);
  const latest = readdirSync(downloads).filter((f) => f.startsWith('test_meadow') && f.endsWith('.json')).map((f) => join(downloads, f)).sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
  check('  the map written again without it', !!latest && !readFileSync(latest, 'utf8').includes('garden_lantern'), latest ?? 'no map downloaded');
  await shot('9-deleted');

  // 10. the game's own assets and the temporary ones apart (all = game + temporary); a model
  // dropped on Creatures: a creature's, its prop of the creature category under
  // Creatures/Models, and a creature record made for it (its model standing for it), listed
  // under Creatures, to save with the rest
  await press('uiFolder_Town');
  await press('uiAssetSpace_game');
  const game = await shown();
  await press('uiAssetSpace_temporary');
  const temp = await shown();
  await press('uiAssetSpace_all');
  const every = await shown();
  check("the game's assets and the temporary ones apart", (await data('assetspace')) === 'all' && game.length + temp.length === every.length && game.every((id) => every.includes(id)) && !game.some((id) => temp.includes(id)),
    `${game.length} game + ${temp.length} temporary = ${every.length}`);
  const beast = join(work, 'Shrub Beast.glb');
  copyFileSync(model, beast);
  await press('uiFolder_Creatures');
  const creatures0 = (await shown()).length;
  const [bx, by] = await ui('uiFolder_Creatures');
  await browser.drop([beast], bx, by);
  await browser.waitFor('document.body.dataset.dropped === "Shrub Beast.glb"', { what: 'the dropped file read' });
  await sleep(500);
  check('a model dropped on Creatures: a creature record made for it, listed there', (await shown()).includes('shrub_beast') && (await shown()).length === creatures0 + 1 && (await data('creaturesdirty')) === '1',
    `${(await shown()).join(' ')} · ${await data('creaturesdirty')} to save · ${await data('status')}`);
  await press('uiFolder_Creatures_Models');
  check('  its prop under Creatures/Models', (await shown()).includes('shrub_beast'), (await shown()).join(' '));
  await shot('10-creature');

  // 11. the stand-in character's "free movement" (a character's toggle, under Does): walking,
  // the arrow held carries it off the grid, between the cells' middles; the category picked
  // from the dropdown's hierarchy through its search (the barrel a creature: one made for it)
  await press('uiFolder_Characters');
  check("a character's toggle: free movement, lit for none yet", (await data('uiFree_stand_in_character')) !== '' && (await data('uiFree_barrel')) === '', await data('uiFree_stand_in_character'));
  await press('uiFree_stand_in_character');
  await sleep(200);
  await press('uiWalk');
  await sleep(400);
  const stood = (await data('walkerpos')).split(',').map(Number);
  check('  walking, free to move', (await data('freemove')) === 'free' && stood.length === 2, `${await data('freemove')} at ${stood}`);
  await browser.key('ArrowRight', { hold: 450 });
  await sleep(300);
  const went = (await data('walkerpos')).split(',').map(Number);
  const off = Math.abs(went[0] - Math.floor(went[0]) - 0.5);
  check('  the arrow held: carried east, off the grid', went[0] > stood[0] + 0.5 && went[0] < stood[0] + 6 && Math.abs(went[1] - stood[1]) < 0.3 && off > 0.03, `${stood} -> ${went} (${off.toFixed(2)} from a middle)`);
  await shot('11-free');
  await browser.key('Escape');
  await sleep(300);
  await press('uiFree_stand_in_character');
  // (the barrel found by the filter: Town's row may sit below the folder list's fold by now)
  await press('uiFolder_All');
  await press('uiAssetFilter');
  await browser.key('a', { ctrl: true });
  await browser.type('barrel');
  await sleep(300);
  await press('uiCategoryPick_barrel');
  await sleep(300);
  check('the category dropdown: its search focused, the types listed', (await data('uiCategorySearch')) !== '' && (await data('uiCategory_creature')) !== '' && (await data('uiCategory_decoration')) !== '', `search "${await data('uiCategorySearch')}"`);
  await browser.type('crea');
  await sleep(300);
  await press('uiCategory_creature');
  await sleep(500);
  await press('uiAssetFilter');
  await browser.key('a', { ctrl: true });
  await browser.key('Backspace');
  await press('uiFolder_Creatures');
  check('  typed and picked: the barrel a creature, a creature record made for it', (await shown()).includes('barrel') && Number(await data('creaturesdirty')) >= 2, `${(await shown()).join(' ')} · ${await data('creaturesdirty')} to save`);
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
