// Model requests in the browser, driven in a headless Chrome against a stand-in for
// Anthropic's API (nothing is asked of Claude, and no one's key is used): the Requests button
// greyed out with no Claude sign-in; a key typed in the Claude sign-in, turned away when it's
// wrong, kept in this browser when it's taken; a picture dropped on the Requests window makes a
// request, kept in the browser; Start runs it "in the cloud": the agent and its environment
// made (named, with the recipe's version; the web's tools off, the network Blender's and the
// package registries'), the request's files uploaded and mounted, the session made with the
// commands that make it and its cap, its events followed (Claude's words and tool calls in the
// console, the scripts' [request] lines moving the bar, the request done), its outputs back
// (its render into its folder, the prop and its model into the asset browser), the uploads and
// the outputs taken off the key's files; the page opened again, the request is still there,
// done; a second Start finds the agent and its environment again, makes neither. The kinds as
// tabs: a kind's window, an instruction added to it (a keyword and how), a variety with one of
// its own, a kind of the user's own made as one of the kit's, all of it kept in this browser.
// Screenshots go to screenshots/requests-*.png.
//     node tools/requestcheck.mjs
import { copyFileSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Browser } from './cdp.mjs';
import { serve } from './serve.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const shots = join(root, 'screenshots');
const work = join(root, 'build', 'requestcheck');
mkdirSync(shots, { recursive: true });
rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const picture = join(work, 'creature front.png');
copyFileSync(join(root, 'crates', 'game', 'assets', 'icon-192.png'), picture);
const KEY = 'sk-ant-api03-test-key-for-the-check-WXYZ';
const ID = 'creature_front';

// ---------------------------------------------------------------- the stand-in for Anthropic
const api = { agents: [], environments: [], files: new Map(), sessions: [], made: { agents: 0, environments: 0 }, deleted: [], keys: new Set(), streams: 0 };
let fileN = 0;
const body = (req) => new Promise((r) => {
  const parts = [];
  req.on('data', (c) => parts.push(c));
  req.on('end', () => r(Buffer.concat(parts)));
});
const answer = (res, status, value) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(value));
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const model = readFileSync(join(root, 'content', 'assets', 'models', 'stone_well.glb'));
const render = readFileSync(join(root, 'crates', 'game', 'assets', 'icon-192.png'));
const output = (name, bytes) => {
  const id = `file_out${++fileN}`;
  api.files.set(id, { name, bytes, scope: 'sesn_test1' });
  return id;
};
// what the session "makes", as the sandbox's scripts would print it
const run = async (res) => {
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
  const send = (e) => res.write(`event: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`);
  let n = 0;
  const ev = (e) => send({ id: `sevt_${++n}`, processed_at: new Date().toISOString(), ...e });
  const now = Math.round(Date.now() / 1000);
  ev({ type: 'session.status_running' });
  ev({ type: 'agent.message', content: [{ type: 'text', text: 'Setting the sandbox up.' }] });
  ev({ type: 'agent.tool_use', name: 'bash', input: { command: `cd /workspace/bf && python3 tools/blender/run_claude.py ${ID} --prompt` } });
  ev({ type: 'agent.tool_result', content: [{ type: 'text', text: [
    `[request] ${JSON.stringify({ id: ID, status: 'in_progress', progress: { step: 'reading the pictures', done: 0.05, started: now, minutes: 8 } })}`,
    `[request-log] ${JSON.stringify({ id: ID, text: 'In the cloud: Claude in a sandbox of Anthropic\'s' })}`,
    `Your instructions for ${ID}: read /tmp/bf-prompt-${ID}.txt whole`] .join('\n') }], is_error: false });
  await sleep(1500);
  ev({ type: 'agent.tool_use', name: 'bash', input: { command: `python tools/blender/run_request.py ${ID}` } });
  ev({ type: 'agent.tool_result', content: [{ type: 'text', text: `[request] ${JSON.stringify({ id: ID, progress: { step: 'building', done: 0.5, started: now, minutes: 8 } })}\nrendered preview_front.png` }], is_error: false });
  await sleep(2500);
  ev({ type: 'agent.tool_result', content: [{ type: 'text', text: `[request] ${JSON.stringify({ id: ID, status: 'done', prop: ID, previews: ['preview_front.png'], progress: { step: 'done', done: 1, started: now, minutes: 1 } })}` }], is_error: false });
  ev({ type: 'agent.message', content: [{ type: 'text', text: 'Made a creature in front of the camera.' }] });
  ev({ type: 'session.usage', list_cost: { amount: '212', currency: 'USD' } });
  // its outputs, as cloud_run.py outputs writes them
  const files = [
    { name: '000_preview_front.png', path: `private/requests/${ID}/preview_front.png`, bytes: render },
    { name: `001_${ID}.json`, path: `temporary/data/props/${ID}.json`, bytes: Buffer.from(JSON.stringify({ id: ID, name: 'Creature front', category: 'character', footprint: [1, 1], height: 1.0, blocks: true, model: `models/${ID}.glb`, folders: ['Creatures'] }, null, 2)) },
    { name: `002_${ID}.glb`, path: `temporary/assets/models/${ID}.glb`, bytes: model },
  ];
  for (const f of files) output(f.name, f.bytes);
  output('manifest.json', Buffer.from(JSON.stringify({ request: ID, files: files.map(({ name, path }) => ({ name, path })) })));
  ev({ type: 'session.status_idle', stop_reason: { type: 'end_turn' } });
  res.end();
};
const handle = async (req, res, path) => {
  if (!path.startsWith('claude/')) return false;
  const url = new URL(req.url, 'http://x');
  const at = path.slice('claude/'.length);
  const key = req.headers['x-api-key'];
  api.keys.add(key);
  if (key !== KEY) return answer(res, 401, { type: 'error', error: { type: 'authentication_error', message: 'invalid x-api-key' } }), true;
  if (at !== 'v1/models' && req.headers['anthropic-beta'] !== 'managed-agents-2026-04-01') return answer(res, 400, { error: { message: 'no beta' } }), true;
  if (at === 'v1/models') return answer(res, 200, { data: [{ id: 'claude-opus-5-5' }], has_more: false }), true;
  for (const kind of ['agents', 'environments']) {
    if (at === `v1/${kind}` && req.method === 'GET') return answer(res, 200, { data: api[kind], next_page: null }), true;
    if (at === `v1/${kind}` && req.method === 'POST') {
      const made = { id: `${kind === 'agents' ? 'agent' : 'env'}_test${api[kind].length + 1}`, ...JSON.parse(await body(req)) };
      api[kind].push(made);
      api.made[kind]++;
      return answer(res, 200, made), true;
    }
  }
  if (at === 'v1/files' && req.method === 'POST') {
    const bytes = await body(req);
    const name = /filename="([^"]+)"/.exec(bytes.toString('latin1'))?.[1] ?? '?';
    const id = `file_in${++fileN}`;
    api.files.set(id, { name, bytes, scope: null, form: bytes.toString('latin1').slice(0, 200) });
    return answer(res, 200, { id, filename: name }), true;
  }
  if (at === 'v1/files' && req.method === 'GET') {
    const scope = url.searchParams.get('scope_id');
    const data = [...api.files.entries()].filter(([, f]) => f.scope === scope).map(([id, f]) => ({ id, filename: f.name, size_bytes: f.bytes.length }));
    return answer(res, 200, { data, has_more: false }), true;
  }
  let m = /^v1\/files\/(file_[a-z0-9]+)(\/content)?$/.exec(at);
  if (m && req.method === 'DELETE') {
    api.deleted.push(m[1]);
    api.files.delete(m[1]);
    return answer(res, 200, { id: m[1], type: 'file_deleted' }), true;
  }
  if (m && m[2]) {
    const f = api.files.get(m[1]);
    if (!f) return answer(res, 404, { error: { message: 'no such file' } }), true;
    res.writeHead(200, { 'content-type': 'application/octet-stream' });
    res.end(f.bytes);
    return true;
  }
  if (at === 'v1/sessions' && req.method === 'POST') {
    const s = { id: 'sesn_test1', ...JSON.parse(await body(req)) };
    api.sessions.push(s);
    return answer(res, 200, { id: s.id, status: 'running' }), true;
  }
  m = /^v1\/sessions\/(sesn_[a-z0-9]+)\/events(\/stream)?$/.exec(at);
  if (m && m[2]) {
    api.streams++;
    await run(res);
    return true;
  }
  if (m) return answer(res, 200, { data: [], next_page: null }), true;
  return answer(res, 404, { error: { message: `no ${req.method} ${at} here` } }), true;
};

// ---------------------------------------------------------------- the check
const server = await serve(0, { handle });
// a profile of its own, empty: nothing kept from another run
const profile = join(root, 'build', 'chrome-requestcheck');
rmSync(profile, { recursive: true, force: true });
const browser = await Browser.launch({ width: 1400, height: 900, port: 9493, profiles: profile, args: ['--ignore-gpu-blocklist', '--use-angle=d3d11', '--enable-unsafe-webgpu', '--mute-audio'] });
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
const ui = async (name) => (await data(name)).split(',').map(Number);
const press = async (name) => {
  const [x, y] = await ui(name);
  await browser.click(x, y);
  await sleep(250);
};
const shot = async (name) => {
  await sleep(400);
  await browser.screenshot(join(shots, `requests-${name}.png`));
};
const editor = async () => {
  await browser.goto(`http://127.0.0.1:${server.address().port}/index.html`);
  await browser.waitFor('document.body.dataset.state === "menu"', { timeout: 90000, what: 'the menu' });
  await cmd('map:test_meadow');
  await browser.waitFor('document.body.dataset.state === "maps"', { what: 'the map editor' });
  await sleep(1200);
  if (await data('uiWelcomeOk')) await press('uiWelcomeOk');
};
const request = async () => (await data(`request_${ID}`)).split('|');

try {
  await browser.viewport(1400, 900);
  await editor();

  // 1. no Claude sign-in: the Requests button greyed out, nothing opens
  check('no sign-in yet', (await data('claude')) === 'out', await data('claude'));
  await press('uiRequests');
  check('  the Requests button greyed out: it opens nothing', (await data('requests')) === '', await data('requests'));
  await shot('1-no-sign-in');

  // 2. the Claude sign-in: a wrong key turned away, the right one taken and kept
  await press('uiClaudeSignIn');
  check('the Claude sign-in opens', (await data('claudewindow')) === 'open');
  await press('uiClaudeKey');
  await browser.type('sk-ant-wrong');
  await press('uiClaudeUse');
  await browser.waitFor('document.body.dataset.status?.includes("turned this key away")', { what: 'the wrong key turned away' });
  check('  a wrong key turned away', (await data('claude')) === 'out');
  await press('uiClaudeKey');
  await browser.key('a', { ctrl: true });
  await browser.type(KEY);
  await shot('2-key');
  await press('uiClaudeUse');
  await browser.waitFor('document.body.dataset.claude === "in"', { what: 'the key taken' });
  check('  the right one taken', (await data('status')).includes('your API key (sk-ant-…WXYZ, kept in this browser)'), await data('status'));
  check('  kept in this browser, and only there', (await browser.evaluate('localStorage.getItem("claude_api_key")')) === KEY);
  await press('uiClaudeClose');

  // 3. the Requests window: a picture dropped on it makes a request, kept in the browser
  await press('uiRequests');
  check('the Requests window opens now', (await data('requests')) === 'open', await data('requests'));
  await browser.drop([picture], 900, 400);
  await browser.waitFor(`(document.body.dataset.request_${ID} ?? "").startsWith("waiting")`, { what: 'the request made' });
  check('  a picture dropped on it makes a request', (await request())[0] === 'waiting', (await request()).join(' '));
  await shot('3-request');
  // its picture clicked: the viewer (no download), closed again
  await press('uiRequestPicture_0');
  await sleep(600);
  check('  its picture clicked opens the viewer', (await data('picture')) === 'creature_front.png' && (await data('uiPictureShown')) !== '', `picture "${await data('picture')}"`);
  await shot('3b-picture');
  await press('uiPictureClose');
  await sleep(300);
  check('  closed again', (await data('picture')) === '', `picture "${await data('picture')}"`);

  // 4. Start: the run in the cloud, followed to its end
  const assets0 = Number(await data('assets'));
  await press(`uiRequestStart_${ID}`);
  await browser.waitFor(`(document.body.dataset.request_${ID} ?? "").split("|")[1] === "0.50"`, { timeout: 30000, what: 'the bar half way (the scripts\' line)' });
  check('Start: in progress, the bar moved by the scripts\' lines', (await request())[0] === 'in_progress' && (await request())[2] === 'building', (await request()).join(' | '));
  await shot('4-running');
  await browser.waitFor(`(document.body.dataset.request_${ID} ?? "").startsWith("done")`, { timeout: 30000, what: 'the request done' });
  await browser.waitFor('document.body.dataset.requestlog?.includes("files back from the cloud")', { timeout: 20000, what: 'its outputs back' });
  check('  done', (await request())[0] === 'done', (await request()).join(' | '));
  const log = await data('requestlog');
  check('  the console: what Claude said, the cost, the outputs back', log.includes('Claude: Made a creature') && log.includes('$2.12') && log.includes('3 files back from the cloud'), log);
  check('  its cost kept with it, on its card', (await data(`requestcost_${ID}`)) === '$2.12', `cost "${await data(`requestcost_${ID}`)}"`);
  // the agent and its sandbox, made once, as the recipe makes them
  const agent = api.agents[0] ?? {};
  check('the agent made: its name, its prompt the recipe\'s, the web\'s tools off, nothing waits for a yes', api.made.agents === 1 && agent.name === 'Battle Frontier model requests'
    && agent.system?.includes('cloud_run.py setup ID') && agent.tools?.[0]?.configs?.every((c) => c.enabled === false)
    && agent.tools?.[0]?.default_config?.permission_policy?.type === 'always_allow' && /^[0-9a-f]{8}$/.test(agent.metadata?.recipe ?? ''), JSON.stringify(agent).slice(0, 200));
  const env = api.environments[0] ?? {};
  check('  its environment: Blender\'s packages, its network the registries and Blender\'s downloads', api.made.environments === 1 && env.config?.packages?.apt?.includes('xvfb')
    && env.config?.networking?.type === 'limited' && JSON.stringify(env.config?.networking?.allowed_hosts) === '["download.blender.org"]', JSON.stringify(env.config));
  const s = api.sessions[0] ?? {};
  const mounts = (s.resources ?? []).map((r) => r.mount_path).sort();
  check('the session: the request\'s files mounted under uploads', mounts.includes(`/request/request.json`) && mounts.includes(`/request/${ID}.png`), mounts.join(' '));
  const text = s.initial_events?.[0]?.content?.[0]?.text ?? '';
  check('  its first message: the checkout, the setup, run_claude.py --prompt and --settle, the outputs', text.includes('git clone --quiet --filter=blob:none --no-checkout https://github.com/LouisMareau/battle-frontier.git')
    && text.includes(`cloud_run.py setup ${ID}`) && text.includes(`run_claude.py ${ID} --prompt`) && text.includes(`run_claude.py ${ID} --settle`) && text.includes(`cloud_run.py outputs ${ID}`), text.slice(0, 300));
  check('  its model and its cap', s.agent?.type === 'agent_with_overrides' && s.agent?.model?.id === 'claude-opus-5-5' && s.budget?.max_list_cost?.amount === '1500', JSON.stringify([s.agent, s.budget]));
  check('  only the user\'s key was sent', api.keys.size === 2 && api.keys.has(KEY), [...api.keys].join(' '));
  check('the uploads and the outputs taken off the key\'s files', api.files.size === 0 && api.deleted.length === 6, `${api.files.size} left, ${api.deleted.length} deleted`);
  // the prop it made: in the asset browser, its model loaded
  await browser.waitFor(`Number(document.body.dataset.assets) === ${assets0 + 1}`, { timeout: 10000, what: 'the prop in the asset browser' });
  check('the prop it made is in the asset browser', Number(await data('assets')) === assets0 + 1, `${await data('assets')} props`);
  await press(`uiRequestFold_${ID}`);
  await shot('5-done');

  // 5. the page opened again: the request kept in the browser, done (the window as it was left:
  // open)
  await editor();
  await sleep(500);
  if ((await data('requests')) !== 'open') await press('uiRequests');
  await browser.waitFor(`(document.body.dataset.request_${ID} ?? "").startsWith("done")`, { timeout: 10000, what: 'the request kept' });
  check('the page opened again: the request still there, done', (await request())[0] === 'done');
  check('  signed in still', (await data('claude')) === 'in');
  await shot('6-again');

  // 6. made again (Start on a done request): the agent and its environment found, not made
  await press(`uiRequestStart_${ID}`);
  await browser.waitFor(`(document.body.dataset.request_${ID} ?? "").startsWith("done") && document.body.dataset.requestlog?.includes("files back")`, { timeout: 40000, what: 'the second run done' });
  check('a second run: the agent and its environment kept, none made again', api.made.agents === 1 && api.made.environments === 1 && api.sessions.length === 2, `${api.made.agents} agents, ${api.made.environments} environments, ${api.sessions.length} sessions`);
  const again = (api.sessions[1]?.resources ?? []).map((r) => r.mount_path).sort();
  check('  given what the first made: its render, and its prop and model (kept in the browser) for the project',
    again.includes(`/request/preview_front.png`) && again.includes(`/project/temporary/data/props/${ID}.json`) && again.includes(`/project/temporary/assets/models/${ID}.glb`), again.join(' '));

  // 7. the kinds as tabs: the Tree tab (none of its requests listed); its window: an instruction
  // added (a keyword and how), a variety (Pine) with one of its own; a kind of the user's own
  // (Bush, made as environment from its tab: environment's instructions to start); all of it
  // kept in this browser once the page is opened again
  const typeInto = async (name, text) => {
    await press(name);
    await browser.key('a', { ctrl: true });
    await browser.type(text);
    await sleep(150);
  };
  await press('uiRequestTab_tree');
  check('the kinds as tabs: Tree shown, none of its requests listed', (await data('requesttab')) === 'tree' && (await data('requestlisted')) === '', `${await data('requesttab')} · listed "${await data('requestlisted')}"`);
  await press('uiRequestKindEdit');
  await sleep(300);
  check('  its window open', (await data('requestkind')) === 'tree', await data('requestkind'));
  await browser.waitFor('(document.body.dataset.uiKindFeatureAdd ?? "") !== "" && (document.body.dataset.uiKindDone ?? "") !== ""', { what: "the kind window's buttons" });
  await sleep(300);
  await press('uiKindFeatureAdd');
  await sleep(300);
  await typeInto('uiKindFeatureKey_4', 'bark');
  await typeInto('uiKindFeatureText_4', 'rough vertical ridges in the mesh, darker in the grooves');
  await typeInto('uiKindVarietyName', 'Pine');
  await press('uiKindVarietyAdd');
  await sleep(300);
  await press('uiKindFeatureAdd');
  await sleep(300);
  await typeInto('uiKindFeatureKey_0', 'foliage');
  await typeInto('uiKindFeatureText_0', 'needles in drooping tiers of cards');
  await sleep(1000);
  const kinds = await data('requestkinds');
  check('  an instruction added to the tree, a variety with one of its own', kinds.includes('tree:trunk,foliage,colour,wind,bark[pine:foliage]'), kinds);
  await shot('7-kind');
  await press('uiKindDone');
  await press('uiRequestTab_environment');
  await press('uiRequestKindAdd');
  await sleep(300);
  await browser.waitFor('(document.body.dataset.uiKindNewName ?? "") !== ""', { what: 'the new kind dialog' });
  await typeInto('uiKindNewName', 'Bush');
  await press('uiKindNewOk');
  await sleep(1000);
  check("a kind of the user's own: Bush, made as environment, its instructions environment's to start, shown", (await data('requesttab')) === 'bush' && (await data('requestkinds')).includes('bush<environment>:wind'), `${await data('requesttab')} · ${await data('requestkinds')}`);
  await shot('7-bush');
  await press('uiKindDone');
  await press('uiRequestTab_all');
  check('  All: every request listed again', (await data('requestlisted')).split(',').includes(ID), await data('requestlisted'));
  await editor();
  await sleep(500);
  if ((await data('requests')) !== 'open') await press('uiRequests');
  await sleep(600);
  const kept = await data('requestkinds');
  check('  all of it kept in this browser, the page opened again', kept.includes('bark[pine:foliage]') && kept.includes('bush<environment>:wind'), kept);
  if (browser.errors.length) {
    failed++;
    console.log('PAGE ERRORS:\n' + browser.errors.join('\n'));
  }
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `${failed} failed` : 'all good');
