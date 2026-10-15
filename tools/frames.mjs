// Frame times of one encounter, played by the AI on both sides, in a headless Chrome on
// the machine's GPU: frames per second and the worst frame gap, second by second, and
// every frame over 25 ms with what the game was doing.
//
//     trunk build --release && node tools/frames.mjs [encounter] [seconds=60] [gpu=1]
// (the first triple trainer battle of the content when no encounter is given)
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { pick } from './encounters.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const [preset = pick('trainer', { size: 3 }) ?? pick('trainer'), ...rest] = process.argv.slice(2).filter((a) => !a.includes('='));
const opt = Object.fromEntries(process.argv.slice(2).filter((a) => a.includes('=')).map((a) => a.split('=')));
const seconds = Number(opt.seconds ?? 60);
const gpuArgs = opt.gpu === '0' ? [] : ['--ignore-gpu-blocklist', '--enable-gpu-rasterization', '--use-angle=d3d11', '--enable-unsafe-webgpu'];
const id = preset;
if (!id) throw new Error('no encounter to play: the content has none');
void rest;

const server = await serve(0);
const browser = await Browser.launch({ width: 1400, height: 900, port: 9478, profiles: join(root, 'build', 'chrome'), args: gpuArgs });
const data = (key) => browser.evaluate(`document.body.dataset.${key} ?? ''`);
try {
  await browser.viewport(1400, 900);
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  console.log('graphics:', await data('backend'));
  await browser.evaluate(`(document.body.dataset.cmd = "auto:1;text:fast;time:day;start:${id}", true)`);
  await browser.waitFor('document.body.dataset.state === "battle"', { timeout: 60000, what: 'the battle' });
  const fps = [], gaps = [];
  for (let i = 0; i < seconds; i++) {
    await browser.wait(1000);
    fps.push(Number(await data('fps')));
    gaps.push(Number(await data('maxgap')));
    if ((await data('state')) !== 'battle') {
      await browser.evaluate(`(document.body.dataset.cmd = "start:${id}", true)`); // next battle, same encounter
    }
  }
  const sorted = [...fps].sort((a, b) => a - b);
  console.log(JSON.stringify({ preset: id, seconds, fpsMin: sorted[0], fpsMedian: sorted[Math.floor(sorted.length / 2)], worstGapMs: Math.max(...gaps), secondsWithAGapOver20ms: gaps.filter((g) => g > 20).length }));
  console.log('per second, worst gap (ms):', gaps.join(' '));
  console.log('frames over 25 ms:', (await data('hitches')) || 'none');
} finally {
  if (browser.errors.length) console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  await browser.close();
  server.close();
}
