// A battle playing an ability's timeline, in a headless Chrome: every placeholder move in
// the served temporary bundle is given the same timeline (a lunge, a tap input with a
// window of its own, a camera shake, an FX whose record is served too), nothing written;
// the gym battle is started as check.mjs does and the first move used; then the game must
// report the timeline playing (data-playing) and a tap read with the timeline's window
// (data-pending), not the config's. Exits 1 when something fails.
//     node tools/timelinebattle.mjs
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { pick } from './encounters.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
mkdirSync(shots, { recursive: true });
const WINDOW = 150;
const timeline = {
  seconds: 1.6,
  clips: [
    { kind: 'motion', who: 'user', at: 0.0, seconds: 0.9, what: 'lunge' },
    { kind: 'input', who: 'target', at: 0.3, seconds: 0.5, what: 'tap', value: WINDOW },
    { kind: 'fx', who: 'target', at: 0.7, seconds: 0.5, what: 'zap' },
    { kind: 'camera', who: 'user', at: 0.8, seconds: 0.3, what: 'shake', value: 0.3 },
    { kind: 'motion', who: 'target', at: 0.8, seconds: 0.3, what: 'shake' },
  ],
};
const zap = {
  id: 'zap', name: 'Zap', fps: 12, frames: 4, size: 32, export_size: 32,
  meshes: [
    { shape: 'billboard', at: [0, 0.8, 0.6], size: [1, 1, 1], turn: [0, 0, 0], source: 'fx/zap_0.png', columns: 4 },
    { shape: 'ring', at: [0, 0.05, 0], size: [1.5, 1, 1.5], turn: [0, 0, 0], source: 'fx/zap_1.png', columns: 4 },
  ],
};
let moves = 0;
const rewrite = (path, body) => {
  if (path !== 'temporary/bundle.json') return body;
  const bundle = JSON.parse(body.toString('utf8'));
  for (const [key, record] of Object.entries(bundle.files)) {
    if (key.startsWith('moves/')) {
      record.timeline = timeline;
      moves++;
    }
  }
  bundle.files['fx/zap.json'] = zap;
  return Buffer.from(JSON.stringify(bundle));
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = await serve(0, { rewrite });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9509, profiles: join(root, 'build', 'chrome'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu'] });
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

try {
  await browser.viewport(1400, 900);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  check('the placeholder moves given the timeline', moves > 0, `${moves} moves`);
  const gym = pick('gym');
  check('a gym encounter to play', Boolean(gym), gym);
  await browser.click(700, 450); // the canvas has the keys
  await cmd(`text:fast;time:day;start:${gym}`);
  await browser.waitFor('document.body.dataset.menu === "open"', { timeout: 60000, what: 'the battle menu' });
  await sleep(300);
  // Fight, the first move, its target
  await browser.key('ArrowDown');
  await browser.key('z');
  await sleep(250);
  await browser.key('z');
  await sleep(250);
  await browser.key('z');
  // the timeline plays, and the read is the timeline's
  let pending = '';
  let playing = '';
  for (let i = 0; i < 60 && !(pending.startsWith('tap:') && playing); i++) {
    await sleep(50);
    pending = (await data('pending')) || pending;
    playing = (await data('playing')) || playing;
  }
  check('the timeline playing', playing.includes(':'), playing);
  const [kind, beat, window] = pending.split(':');
  check('a tap read from its input clip', kind === 'tap', pending);
  check(`  with the clip's window of ${WINDOW} ms, not the usual one`, Number(window) === WINDOW, `${window} ms`);
  check('  its beat ahead', Number(beat) > 0, beat);
  await sleep(500);
  await browser.screenshot(join(shots, 'timelinebattle-1-playing.png'));
  // the ability lands and the battle goes on
  await browser.waitFor('document.body.dataset.pending === ""', { timeout: 10000, what: 'the read over' });
  for (let i = 0; i < 80 && (await data('playing')); i++) await sleep(50);
  check('the timeline over', (await data('playing')) === '', await data('playing'));
  check('the battle went on', (await data('state')) === 'battle' && Number(await data('round')) >= 1, `round ${await data('round')}`);
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
