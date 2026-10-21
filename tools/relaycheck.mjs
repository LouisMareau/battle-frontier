// The site's relay for the model requests' calls to Anthropic (functions/claude/[[path]].js),
// run as Cloudflare runs it but with Anthropic's address answered here: only the agents',
// environments', sessions', files' and models' calls go on, only from the site's own pages,
// with only the API's own headers (the user's key among them, nothing else: no cookie), the
// query and the body as they came; the answer comes back as it is (streamed), with only its
// type, its request id and when to try again.
//     node tools/relaycheck.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
// (as a module whatever Node makes of a .js file: Cloudflare's Functions are modules)
const source = readFileSync(`${root}functions/claude/[[path]].js`, 'utf8');
const relay = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
let failed = 0;
const check = (what, ok, detail = '') => {
  if (!ok) failed++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${detail ? `: ${detail}` : ''}`);
};
// Anthropic, answered here: what it was asked
let asked = null;
globalThis.fetch = async (url, init) => {
  asked = { url, method: init.method, headers: Object.fromEntries(new Headers(init.headers)), body: init.body ? Buffer.from(init.body).toString() : null };
  const stream = new ReadableStream({
    start(c) {
      c.enqueue(new TextEncoder().encode('data: {"type":"session.status_running"}\n\n'));
      c.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream', 'request-id': 'req_1', 'set-cookie': 'x=1', 'x-internal': 'y' } });
};
const site = 'https://battle-frontier.pages.dev';
const call = (path, { method = 'GET', headers = {}, body } = {}) => {
  asked = null;
  const url = `${site}/claude/${path}`;
  const request = new Request(url, { method, headers: { origin: site, ...headers }, body });
  return relay.onRequest({ request, params: { path: new URL(url).pathname.slice('/claude/'.length).split('/') } });
};

const key = { 'x-api-key': 'sk-ant-users-own', 'anthropic-version': '2023-06-01', 'anthropic-beta': 'managed-agents-2026-04-01', cookie: 'session=abc', 'x-forwarded-for': '1.2.3.4' };
let r = await call('v1/sessions/sesn_abc123/events/stream?beta=true', { headers: key });
check('a session\'s stream goes on to Anthropic, its query with it', r.status === 200 && asked?.url === 'https://api.anthropic.com/v1/sessions/sesn_abc123/events/stream?beta=true', asked?.url);
check('  with the API\'s own headers, the user\'s key among them, nothing else', asked?.headers['x-api-key'] === 'sk-ant-users-own' && asked?.headers['anthropic-beta'] === 'managed-agents-2026-04-01'
  && !('cookie' in (asked?.headers ?? {})) && !('x-forwarded-for' in (asked?.headers ?? {})), JSON.stringify(asked?.headers));
check('  the answer as it is, streamed', (await r.text()).startsWith('data: {"type":"session.status_running"}'));
check('  with only its type and its request id', r.headers.get('content-type') === 'text/event-stream' && r.headers.get('request-id') === 'req_1' && !r.headers.get('set-cookie') && !r.headers.get('x-internal') && r.headers.get('cache-control') === 'no-store');
r = await call('v1/files', { method: 'POST', headers: { ...key, 'content-type': 'multipart/form-data; boundary=b0' }, body: '--b0\r\nfile\r\n--b0--\r\n' });
check('a file uploaded: its body and its type as they came', r.status === 200 && asked?.method === 'POST' && asked?.body === '--b0\r\nfile\r\n--b0--\r\n' && asked?.headers['content-type'] === 'multipart/form-data; boundary=b0');
for (const path of ['v1/models', 'v1/agents', 'v1/agents/agent_x1', 'v1/environments/env_x1', 'v1/sessions', 'v1/files/file_x1/content']) {
  r = await call(path, { headers: key });
  check(`  ${path} goes on`, r.status === 200 && asked !== null);
}
for (const path of ['v1/messages', 'v1/complete', 'v1/organizations/me', 'v1/agents/agent_x1/versions', 'v1/files/../messages', 'v1/vaults']) {
  r = await call(path, { headers: key });
  check(`  ${path}: not passed on`, r.status === 404 && asked === null, String(r.status));
}
r = await call('v1/files/file_x1', { method: 'PUT', headers: key });
check('another method: not passed on', r.status === 405 && asked === null);
r = await relay.onRequest({ request: new Request(`${site}/claude/v1/agents`, { headers: { ...key, origin: 'https://elsewhere.example' } }), params: { path: ['v1', 'agents'] } });
check('a page of another site: turned away', r.status === 403 && asked === null);
console.log(failed ? `${failed} failed` : 'all good');
process.exit(failed ? 1 : 0);
