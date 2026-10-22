// A pull request reviewed in the built site's map editor. GitHub is a stand-in on the page's
// own address (`github:stand-in`), the collaborator signed in beforehand (their sign-in stored
// as the editor keeps it; their permission: write) and a Claude key stored too. The open pull
// requests are listed; #7 (a terrain added, one changed, a prop added, one taken out, code and
// docs changed) is fetched whole (its files at its head, the records at its base) and opened in
// an editor of its own: read-only (Save saves nothing), the panels' buttons dotted with what
// each holds, the terrain changed marked in the Ground tool and the one added listed, the
// Assets panel's "Changed in the PR" folder showing the prop added and naming the one taken
// out, the Review window listing every change with the code's diffs, and Review by Claude
// (Anthropic at a stand-in behind the site's relay path) shown in its window. The review left,
// the editor is back as it was: the pull request's prop gone, its terrain gone. No error.
// Screenshots to screenshots/review-*.png. Exits 1 when something fails.
//     node tools/reviewcheck.mjs
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

// the pull request: its files at its head and (the records) at its base
const HEAD = 'h7h7h7h7h7h7h7h7h7h7h7h7h7h7h7h7h7h7h7h7';
const BASE = 'b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0';
const pull = { number: 7, title: 'Lava and a lamp', user: { login: 'someone' }, head: { ref: 'lava', sha: HEAD }, base: { sha: BASE }, html_url: 'https://github.com/LouisMareau/battle-frontier/pull/7', changed_files: 7, draft: false };
const text = (v) => JSON.stringify(v, null, 2) + '\n';
const lava = { id: 'lava', name: 'Lava', pattern: 'custom', walk: 'block', color: '#ff5a10', color2: '#ffb030', side: '#2a2220', priority: 1, look: { scale: 1.5, sharpness: 0.4, grain: 0.1, glow: 2.2, pulse: 0.25, layers: [{ shape: 'stones', color: '#2b2320', scale: 1.1, amount: 0.62, softness: 0.25, relief: 0.7 }] } };
const grassBefore = { id: 'grass', name: 'Grass', pattern: 'grass', walk: 'walk', color: '#4a8a3a', color2: '#2e6a28', priority: 1 };
const grassAfter = { ...grassBefore, priority: 2, color2: '#2a6a30' };
const lamp = { id: 'review_lamp', name: 'Review lamp', category: 'decoration', footprint: [1, 1], height: 1.2, parts: [{ shape: 'box', size: [0.3, 1.2, 0.3], at: [0, 0, 0], color: '#aa8844' }] };
const crateBefore = { id: 'crate', name: 'Crate', category: 'decoration', footprint: [1, 1], height: 0.8, parts: [] };
const files = [
  { filename: 'content/data/terrains/lava.json', status: 'added', additions: 20, deletions: 0 },
  { filename: 'content/data/terrains/grass.json', status: 'modified', additions: 2, deletions: 2 },
  { filename: 'content/data/props/review_lamp.json', status: 'added', additions: 12, deletions: 0 },
  { filename: 'content/data/props/crate.json', status: 'removed', additions: 0, deletions: 9 },
  { filename: 'crates/game/src/mapedit/terrains.rs', status: 'modified', additions: 14, deletions: 2, patch: '@@ -10,3 +10,15 @@\n fn presets() {\n-    old\n+    new: a lava lake\n+    fn lake() {}\n' },
  { filename: 'tools/lavacheck.mjs', status: 'added', additions: 40, deletions: 0, patch: '@@ -0,0 +1,3 @@\n+// the lava checked\n+import x from y;\n+check(1);\n' },
  { filename: 'README.md', status: 'modified', additions: 3, deletions: 0, patch: '@@ -1 +1,4 @@\n # Battle Frontier\n+\n+Lava.\n' },
];
const at = (path, ref) => {
  const head = { 'content/data/terrains/lava.json': lava, 'content/data/terrains/grass.json': grassAfter, 'content/data/props/review_lamp.json': lamp };
  const base = { 'content/data/terrains/grass.json': grassBefore, 'content/data/props/crate.json': crateBefore };
  return (ref === HEAD ? head : base)[path];
};
const asked = [];
const reply = (res, status, body) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(body));
  return true;
};
// GitHub's stand-in (the API under stand-in/api) and Anthropic's (the site's relay path)
const handle = async (req, res, path) => {
  if (path === 'claude/v1/messages') {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
    asked.push({ at: path, key: req.headers['x-api-key'] ?? '', model: body.model, prompt: body.messages?.[0]?.content ?? '' });
    return reply(res, 200, { content: [{ type: 'thinking', thinking: 'reading' }, { type: 'text', text: '## Content\n- **terrains**: `lava` added (a lake, glowing); `grass` reaches further (priority 1 -> 2)\n- **props**: `review_lamp` added; `crate` taken out\n## Editor and engine code\n- crates/game/src/mapedit/terrains.rs: a lava lake preset (`lake()`)\n## Tools, checks and the site\n- tools/lavacheck.mjs: the lava checked\n## Risks and what to look at\n- Maps placing `crate` lose it\n## Verdict\nReady, once the crate is accounted for.' }] });
  }
  if (!path.startsWith('stand-in/')) return false;
  const q = new URL(req.url, 'http://x').searchParams;
  const rel = path.slice('stand-in/'.length);
  asked.push({ at: rel, ref: q.get('ref') ?? '', auth: req.headers.authorization ?? '' });
  const repo = 'api/repos/LouisMareau/battle-frontier';
  if (rel === 'api/user') return reply(res, 200, { login: 'player', id: 42 });
  if (rel === `${repo}/collaborators/player/permission`) return reply(res, 200, { permission: 'write', role_name: 'write' });
  if (rel === `${repo}/pulls`) return reply(res, 200, [pull, { ...pull, number: 8, title: 'A draft', draft: true, changed_files: 1 }]);
  if (rel === `${repo}/pulls/7`) return reply(res, 200, pull);
  if (rel === `${repo}/pulls/7/files`) return reply(res, 200, files);
  if (rel.startsWith(`${repo}/contents/`)) {
    const file = at(rel.slice(`${repo}/contents/`.length), q.get('ref'));
    if (!file) return reply(res, 404, { message: 'Not Found' });
    return reply(res, 200, { encoding: 'base64', sha: 'x', content: Buffer.from(text(file)).toString('base64') });
  }
  return reply(res, 404, { message: 'Not Found' });
};

const server = await serve(0, { handle });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9533, profiles: join(root, 'build', 'chrome-review'), args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
const url = `http://127.0.0.1:${server.address().port}/index.html`;
const data = (key) => browser.evaluate(`document.body.dataset[${JSON.stringify(key)}] ?? ''`);
const cmd = async (c) => {
  await browser.evaluate(`(document.body.dataset.cmd = ${JSON.stringify(c)}, true)`);
  await sleep(300);
};
const press = async (name, wait = 400) => {
  const [x, y] = (await data(name)).split(',').map(Number);
  if (!(x > 0 && y > 0)) throw new Error(`${name} isn't on screen`);
  await browser.click(x, y);
  await sleep(wait);
};
const shot = (name) => browser.screenshot(join(shots, `review-${name}.png`));

try {
  await browser.viewport(1400, 900);
  // signed in with GitHub and Claude beforehand, as the editor keeps them
  await browser.send('Page.addScriptToEvaluateOnNewDocument', { source: "localStorage.setItem('github_signin', JSON.stringify({ login: 'player', token: 'gho_check' })); localStorage.setItem('claude_api_key', 'sk-ant-check');" });
  await browser.goto(url);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('maps');
  await browser.waitFor('document.body.dataset.state === "maps" && document.body.dataset.loading === "0"', { timeout: 60000, what: 'the editor' });
  await sleep(1500);
  await browser.waitFor('(document.body.dataset.layout ?? "") !== ""', { what: 'the panels laid out' });
  const map0 = await data('map');
  const props0 = Number(await data('assets'));
  await cmd('github:stand-in');
  await sleep(300);
  check('signed in as the collaborator', (await data('signedin')) === 'player', await data('signedin'));

  // the pull requests listed, #7 fetched whole and opened
  await cmd('review:7');
  await browser.waitFor('(document.body.dataset.reviewpulls ?? "").startsWith("write|")', { timeout: 30000, what: 'the pull requests listed' });
  check('the pull requests open, listed with the collaborator\'s permission', (await data('reviewpulls')) === 'write|7:Lava and a lamp;8:A draft', await data('reviewpulls'));
  await browser.waitFor('(document.body.dataset.reviewopened ?? "").startsWith("7:")', { timeout: 60000, what: '#7 fetched' });
  check('  #7 fetched: its files at its head, its records at its base', (await data('reviewopened')) === '7:7 files' && asked.some((a) => a.at.endsWith('contents/content/data/terrains/grass.json') && a.ref === BASE) && asked.some((a) => a.at.endsWith('contents/content/data/terrains/lava.json') && a.ref === HEAD), `${await data('reviewopened')}; ${asked.map((a) => a.at).join(' ')}`);
  check('  as the collaborator, never the code', asked.every((a) => !a.at || a.at.startsWith('claude') || a.auth === 'Bearer gho_check') && !asked.some((a) => a.at.includes('terrains.rs')));
  await shot('1-fetched');
  await press('uiReviewEnter', 2500);
  await browser.waitFor('document.body.dataset.reviewing === "7"', { timeout: 30000, what: 'the review open' });
  await browser.waitFor('document.body.dataset.loading === "0"', { timeout: 30000, what: 'the review\'s map' });
  await sleep(1500);

  // read-only, every panel's changes on its button
  const dots = await data('reviewdots');
  check('the panels\' buttons dotted with what each holds', dots.includes('toolbox:') && dots.includes('assets:') && !dots.includes('post'), dots);
  await press('uiSave');
  check('  read-only: Save saves nothing', (await data('status')).includes('nothing is saved'), await data('status'));
  await browser.click(700, 889);
  await browser.key('2');
  await browser.waitFor('document.body.dataset.tool === "ground"', { what: 'the Ground tool' });
  check('  the terrain changed marked in the Ground tool, the one added listed', (await data('uiChanged_terrains_grass')) !== '' && (await data('uiTerrain_lava')) !== '' && (await data('uiChanged_terrains_lava')) !== '', `${await data('uiChanged_terrains_grass')} / ${await data('uiTerrain_lava')}`);
  if (!(await data('layout')).includes('assets=')) await press('uiAssets', 600);
  await press('uiFolder_Changed', 600);
  check('  the Assets panel\'s "Changed in the PR" folder: the prop added', (await data('shown')) === 'review_lamp', await data('shown'));
  await shot('2-marked');

  // the Review window: every change listed; Review by Claude
  await press('uiReview', 800);
  await shot('3-changes');
  await press('uiReviewClaude', 600);
  await browser.waitFor('document.body.dataset.reviewclaude === "answered"', { timeout: 30000, what: 'Claude\'s review' });
  await sleep(600);
  const claude = asked.find((a) => a.at === 'claude/v1/messages');
  check('Review by Claude: asked on the user\'s key, with the changes and the diffs, answered in its window', claude?.key === 'sk-ant-check' && claude.prompt.includes('terrains: 1 added, 1 modified') && claude.prompt.includes('crates/game/src/mapedit/terrains.rs') && claude.prompt.includes('+    fn lake() {}') && claude.prompt.includes('## Verdict'), claude ? claude.prompt.slice(0, 300) : 'not asked');
  await shot('4-claude');
  await press('uiReviewClaudeClose', 400);
  await press('uiReviewClose', 400);

  // the review left: the editor back as it was
  await press('uiReviewLeave', 2000);
  await browser.waitFor('document.body.dataset.reviewing === ""', { timeout: 30000, what: 'the review left' });
  await sleep(1000);
  if (!(await data('layout')).includes('assets=')) await press('uiAssets', 600);
  check('the review left: the editor back on its map, the pull request\'s prop gone', (await data('map')) === map0 && Number(await data('assets')) === props0, `${await data('map')} (was ${map0}), ${await data('assets')} props (were ${props0})`);
  await browser.key('2');
  await sleep(400);
  // (a mark's hook keeps its last place: what's drawn says what's there)
  check('  its terrain gone, nothing marked', !(await data('terrainsshown')).split(';').includes('lava') && (await data('reviewdots')) === '', `${await data('terrainsshown')} / ${await data('reviewdots')}`);
  await shot('5-back');
  check('no error in the page', browser.errors.length === 0, browser.errors.slice(0, 3).join(' | '));
} catch (e) {
  check('ran to its end', false, e.message);
  await shot('fail').catch(() => {});
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
