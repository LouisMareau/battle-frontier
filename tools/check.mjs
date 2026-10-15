// Plays the built game (build/dist) in a headless Chrome and reports frame times:
// a gym battle driven with the keyboard (menus, targeting, a send the leader matches),
// with an in-page "perfect player" hitting the timing windows, then AI battles on each
// region's map. Screenshots go to screenshots/.
//
//     trunk build --release && node tools/check.mjs [seconds=60] [gpu=1] [w=1400 h=900]
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { pick } from './encounters.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const opt = Object.fromEntries(process.argv.slice(2).filter((a) => a.includes('=')).map((a) => a.split('=')));
const width = Number(opt.w ?? 1400), height = Number(opt.h ?? 900);
const seconds = Number(opt.seconds ?? 60);
const shots = join(root, 'screenshots');
mkdirSync(shots, { recursive: true });

// a real GPU when the machine lets headless Chrome have one, else its software renderer
const gpuArgs = opt.gpu === '0' ? [] : ['--ignore-gpu-blocklist', '--enable-gpu-rasterization', '--use-angle=d3d11', '--enable-unsafe-webgpu'];
const server = await serve(0);
const port = server.address().port;
const browser = await Browser.launch({ width, height, port: 9477, profiles: join(root, 'build', 'chrome'), args: gpuArgs });
const js = (code) => browser.evaluate(code);
const data = (key) => js(`document.body.dataset.${key} ?? ''`);
const shot = async (name) => { await browser.screenshot(join(shots, `${name}.png`)); console.log('shot', name); };
const cmd = (c) => js(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
const step = (m) => console.log('-', m);

/** Frame stats once a second for `secs`: the app writes data-fps and data-maxgap. */
async function sample(secs, label) {
  const fps = [], gaps = [];
  for (let i = 0; i < secs; i++) {
    await browser.wait(1000);
    fps.push(Number(await data('fps')));
    gaps.push(Number(await data('maxgap')));
  }
  const sorted = [...fps].sort((a, b) => a - b);
  const result = { label, seconds: secs, fpsMin: sorted[0], fpsMedian: sorted[Math.floor(sorted.length / 2)], worstGapMs: Math.max(...gaps), gapsOver20: gaps.filter((g) => g > 20).length, perSecondWorst: gaps };
  console.log('  frames:', JSON.stringify(result));
  return result;
}

try {
  await browser.viewport(width, height);
  await browser.goto(`http://127.0.0.1:${port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  const gpu = await js(`(async () => {
    const c = document.createElement('canvas'); const gl = c.getContext('webgl2');
    const info = gl && gl.getExtension('WEBGL_debug_renderer_info');
    const webgl = gl ? (info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER)) : 'no WebGL 2';
    let webgpu = 'no WebGPU';
    if (navigator.gpu) { const a = await navigator.gpu.requestAdapter(); webgpu = a ? (a.info ? (a.info.vendor + ' ' + a.info.architecture + ' ' + a.info.description) : 'adapter') : 'no adapter'; }
    return { webgl, webgpu };
  })()`);
  console.log('graphics:', JSON.stringify(gpu), '· the game uses:', await data('backend'));
  await shot('00-menu');
  // the content browser, temporary records flagged
  await cmd('editor');
  await browser.waitFor('document.body.dataset.state === "editor"', { timeout: 10000, what: 'the content browser' });
  await browser.wait(1200);
  await shot('00b-content');
  await cmd('back');
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 10000, what: 'the menu again' });

  // the perfect player: presses A on every beat the game publishes in data-pending
  await js(`(() => {
    const canvas = document.getElementById('game');
    const key = (type) => canvas.dispatchEvent(new KeyboardEvent(type, { key: 'z', code: 'KeyZ', bubbles: true }));
    const tap = (at) => setTimeout(() => { key('keydown'); setTimeout(() => key('keyup'), 30); }, Math.max(0, at - performance.now() - 4));
    let last = '';
    setInterval(() => {
      const p = document.body.dataset.pending || '';
      if (!window.__perfect || !p || p === last) return;
      last = p;
      const [kind, a, b, c] = p.split(':');
      if (kind === 'tap') tap(Number(a));
      if (kind === 'rhythm') a.split(',').map(Number).forEach(tap);
      if (kind === 'charge') {
        const fill = Number(b), win = Number(c);
        setTimeout(() => { key('keydown'); setTimeout(() => key('keyup'), fill + win / 2); }, 80);
      }
    }, 4);
    window.__perfect = true;
    return true;
  })()`);
  await browser.click(width / 2, height / 2); // focus the canvas

  const gym = pick('gym');
  if (!gym) throw new Error('no gym encounter in the content: the battle checks need one');
  step(`gym battle: ${gym}, played with the keyboard`);
  await cmd(`text:fast;time:day;start:${gym}`);
  await browser.waitFor('document.body.dataset.menu === "open"', { timeout: 60000, what: 'the battle menu' });
  await browser.wait(300);
  await shot('01-battle-menu');
  await browser.key('ArrowDown');
  await browser.key('z');
  await browser.wait(250);
  await shot('02-moves');
  await browser.key('z');
  await browser.wait(250);
  await shot('03-target');
  await browser.key('z');
  await browser.wait(700);
  await shot('04-charge');

  // keep playing: send a second creature once (the leader matches), then attack
  let sent = false;
  const started = Date.now();
  const frames = [];
  const play = async () => {
    while (Date.now() - started < 600000) {
      const state = await data('state');
      if (state !== 'battle') return state;
      if ((await data('menu')) === 'open') {
        await browser.wait(150);
        if (!sent) {
          sent = true;
          for (const k of ['ArrowDown', 'ArrowDown', 'z', 'ArrowDown', 'z', 'z']) { await browser.key(k); await browser.wait(90); }
          await browser.wait(1500);
          const sizes = await js('[document.body.dataset.round]');
          console.log('  sent a second creature at round', sizes[0]);
        } else {
          for (const k of ['z', 'z']) { await browser.key(k); await browser.wait(90); }
        }
      }
      await browser.wait(120);
    }
    return 'timeout';
  };
  const playing = play();
  frames.push(await sample(Math.min(seconds, 30), 'gym battle, keyboard + perfect timing'));
  await shot('05-gym');
  const end = await playing;
  console.log('  battle ended:', end, await data('outcome'));
  await browser.wait(800);
  await shot('06-results');
  await cmd('back');
  await browser.wait(300);

  const triple = pick('trainer', { size: 3 });
  const runs = [[pick('wild'), 'night', '07-wild-night'], [pick('wild', { nth: 1 }), 'morning', '08-wild-morning'], [triple, 'day', '09-triple']].filter(([id]) => id);
  for (const [id, time, name] of runs) {
    step(`${id} (${time}), AI on both sides`);
    await cmd(`auto:1;time:${time};start:${id}`);
    await browser.waitFor('document.body.dataset.state === "battle"', { timeout: 60000 });
    if (id === triple) frames.push(await sample(seconds, `triple battle (${id}), AI`));
    await browser.waitFor('Number(document.body.dataset.round || 0) >= 3 || document.body.dataset.state !== "battle"', { timeout: 180000 }).catch(() => {});
    await shot(name);
    await browser.waitFor('document.body.dataset.state === "results"', { timeout: 600000, what: `the end of ${id}` });
    console.log('  outcome:', await data('outcome'));
    await cmd('back');
    await browser.wait(400);
  }
  console.log('FRAMES', JSON.stringify(frames));
  console.log('HITCHES (frames over 25 ms):', await data('hitches'));
} finally {
  if (browser.errors.length) console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  const errs = browser.console.filter((l) => l.startsWith('error') || l.includes('panicked'));
  if (errs.length) console.log('CONSOLE ERRORS:\n' + errs.slice(0, 20).join('\n'));
  await browser.close();
  server.close();
}
