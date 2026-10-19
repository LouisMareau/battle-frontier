// Abilities, their timelines and FX, driven in a headless Chrome: makes an ability in the
// Abilities window and changes its power, gives it a level-up and something it becomes;
// opens its timeline and adds a motion, an FX, an input and a sound; makes the FX record,
// enters the FX editor, adds a sphere to its billboard, paints a stroke on a frame in the
// Drawing window, copies it to the next frame and undoes that, exports the flipbooks
// (downloads); plays the timeline; saves everything with Ctrl+S (downloads) and reads the
// records back. Screenshots go to screenshots/ability-*.png.
//     node tools/abilitycheck.mjs
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const downloads = join(root, 'build', 'downloads');
mkdirSync(shots, { recursive: true });
rmSync(downloads, { recursive: true, force: true });
mkdirSync(downloads, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9505, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
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
  if (!x && !y) {
    check(`${name} on screen`, false, 'no such widget');
    return;
  }
  await browser.click(x, y);
  await sleep(250);
};
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `ability-${name}.png`));
};
const got = (name) => existsSync(join(downloads, name));
const wait = async (names) => {
  for (let i = 0; i < 50 && !names.every(got); i++) await sleep(200);
};

try {
  await browser.viewport(1400, 900);
  await browser.downloadsTo(downloads);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  // the asset browser closed (it comes open, along the bottom): the view as these steps knew it
  await browser.waitFor('(document.body.dataset.layout ?? "") !== ""', { what: 'the panels laid out' });
  if ((await data('layout')).includes('assets=bottom')) {
    await press('uiAssets');
    await sleep(400);
  }
  await cmd('view:20,12,12,0,45');

  // 1. a new ability, its power changed, a level-up and something it becomes
  await press('uiAbilities');
  const before = Number(await data('abilities'));
  await press('uiNewAbilityId');
  await browser.type('ember_fang');
  await press('uiNewAbility');
  await expect('a new ability picked', 'ability', 'ember_fang');
  await expect('one more ability', 'abilities', before + 1);
  await expect('one changed', 'abilitiesdirty', 1);
  const [px, py] = await ui('uiAbilityPower');
  await browser.drag([[px, py], [px + 10, py], [px + 25, py], [px + 40, py]]);
  await sleep(200);
  await press('uiAddLevel3');
  await press('uiAddEvolution');
  await shot('1-ability');

  // 2. its timeline: a motion, an FX (made here), an input and a sound
  await press('uiEditTimeline');
  await expect('the timeline shown', 'timeline', 'ember_fang');
  await press('uiAbilities'); // the window closed: the panel and the map in the clear
  await press('uiTimelineAdd_motion');
  await press('uiTimelineAdd_fx');
  await expect('two clips', 'clips', 2);
  await expect('the fx clip picked', 'tlselected', 1);
  await press('uiNewFxId');
  await browser.type('spark');
  const fx0 = Number(await data('fxcount'));
  await press('uiMakeFx');
  await expect('an FX record made', 'fxcount', fx0 + 1);
  await press('uiTimelineAdd_input');
  await press('uiTimelineAdd_sound');
  await expect('four clips', 'clips', 4);
  await shot('2-timeline');

  // 3. the FX editor: a sphere added, a stroke painted, copied to the next frame, undone
  const [tx, ty] = await ui('uiTimelineTracks');
  const [scale, left] = (await data('tlscale')).split(',').map(Number);
  await browser.click(left + scale * 0.05 + 4, ty + 18 + 22 * 3 + 11); // the FX track, at the clip
  await sleep(250);
  await expect('the fx clip picked again', 'tlselected', 1);
  await press('uiEditFx');
  await expect('the FX edited', 'fxedit', 'spark');
  await expect('  with its billboard', 'fxmeshes', 1);
  await press('uiFxAdd_sphere');
  await expect('  a sphere added', 'fxmeshes', 2);
  await press('uiFxMesh0');
  for (let i = 0; i < 40 && !(await data('fxcanvas')); i++) await sleep(100);
  const [cx, cy, cw, ch] = (await data('fxcanvas')).split(',').map(Number);
  check('the Drawing window', cw > 100, `${cw} x ${ch}`);
  await expect('frame 0, blank', 'fxpainted', 0);
  await browser.drag([[cx + cw * 0.3, cy + ch * 0.3], [cx + cw * 0.4, cy + ch * 0.4], [cx + cw * 0.55, cy + ch * 0.55], [cx + cw * 0.7, cy + ch * 0.7]]);
  await sleep(300);
  const painted = Number(await data('fxpainted'));
  check('a stroke painted', painted > 100, `${painted} pixels`);
  await shot('3-drawing');
  await press('uiFrameNext');
  await expect('frame 1', 'fxframe', 1);
  await expect('  blank', 'fxpainted', 0);
  await press('uiCopyPrevious');
  await expect('  the frame before copied onto it', 'fxpainted', painted);
  await press('uiDrawUndo');
  await expect('  and undone', 'fxpainted', 0);
  await press('uiFramePrev');
  await expect('back on frame 0, the stroke kept', 'fxpainted', painted);
  // the billboard dragged on the map: its height kept
  await cmd('point:20.5,0.8,12.5');
  await sleep(200);
  const [bx, by] = await ui('point');
  await browser.drag([[bx, by], [bx + 20, by], [bx + 40, by + 5], [bx + 60, by + 10]]);
  await sleep(300);
  await shot('4-fx');

  // 4. the flipbooks exported: the sheets drawn and the records downloaded
  await press('uiFxExport');
  await wait(['spark.json', 'spark_0.png', 'spark_0_x64.png', 'spark_1_x64.png']);
  check('the flipbooks and the FX record downloaded', got('spark.json') && got('spark_0.png') && got('spark_0_x64.png') && got('spark_1_x64.png'), ['spark.json', 'spark_0.png', 'spark_0_x64.png', 'spark_1_x64.png'].map((n) => `${n}: ${got(n) ? 'yes' : 'no'}`).join(', '));
  if (got('spark.json')) {
    const fx = JSON.parse(readFileSync(join(downloads, 'spark.json'), 'utf8'));
    check('  two meshes, their flipbooks named', fx.meshes?.length === 2 && fx.meshes[0].sheet === 'fx/spark_0_x64.png' && fx.meshes[0].shape === 'billboard' && fx.meshes[1].shape === 'sphere', JSON.stringify(fx.meshes?.map((m) => [m.shape, m.sheet, m.at])));
    check('  the billboard moved by the drag', fx.meshes?.[0]?.at?.[0] !== 0 && fx.meshes?.[0]?.at?.[1] === 0.8, JSON.stringify(fx.meshes?.[0]?.at));
  }
  await press('uiFxDone');
  await expect('the FX editor closed', 'fxedit', '');

  // 5. the timeline plays
  await press('uiTimelinePlay');
  await expect('playing', 'tlplaying', 1);
  await sleep(700);
  check('the playhead moved', Number(await data('tlt')) > 0.3, `${await data('tlt')} s`);
  await shot('5-playing');
  await press('uiTimelinePlay');

  // 6. saved: the ability's record, read back
  await browser.key('s', { ctrl: true });
  await wait(['ember_fang.json']);
  check('the ability downloaded', got('ember_fang.json'));
  if (got('ember_fang.json')) {
    const m = JSON.parse(readFileSync(join(downloads, 'ember_fang.json'), 'utf8'));
    check('  its power changed by the drag', m.power !== 40, `power ${m.power}`);
    check('  a level-up at level 3', m.levels?.some((l) => l.level === 3), JSON.stringify(m.levels));
    check('  something it becomes, at level 3', m.evolutions?.[0]?.at_level === 3, JSON.stringify(m.evolutions));
    const kinds = (m.timeline?.clips ?? []).map((c) => c.kind);
    check('  the timeline\'s four clips', kinds.join(',') === 'motion,fx,input,sound' && m.timeline.clips[1].what === 'spark', `${kinds.join(',')} · fx ${m.timeline?.clips?.[1]?.what}`);
  }
  await expect('nothing left to save', 'abilitiesdirty', 0);
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
