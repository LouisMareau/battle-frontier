// The editor's model requests in the browser (crates/game/src/mapedit/claude.rs, requests/):
// they run on the user's own Claude API key, in a sandbox of Anthropic's. Anthropic's API
// answers a page for some of its calls but turns it away for its agents', sessions' and files'
// (they send no CORS headers), so the page's calls come to the site's own address and go on
// from here to api.anthropic.com as they came. The key they carry is the user's: passed on as
// it is and never kept, logged or used for anything else (the site has no key of its own).
// Only these calls, from the site's own pages:
//     /claude/v1/models                                the key checked
//     /claude/v1/agents[/ID], /claude/v1/environments[/ID]
//                                                      the agent and its sandbox, made once a key
//     /claude/v1/sessions[/ID[/events[/stream]]]       a request's run: started, followed, stopped
//     /claude/v1/files[/ID[/content]]                  its pictures in, what it made out
//     /claude/v1/messages                              Review by Claude (a pull request's changes)
const API = 'https://api.anthropic.com/';
const PASSED = [
  /^v1\/models$/,
  /^v1\/agents(\/agent_[A-Za-z0-9_-]+)?$/,
  /^v1\/environments(\/env_[A-Za-z0-9_-]+)?$/,
  /^v1\/sessions(\/sesn_[A-Za-z0-9_-]+(\/events(\/stream)?)?)?$/,
  /^v1\/files(\/file_[A-Za-z0-9_-]+(\/content)?)?$/,
  /^v1\/messages$/,
];
// what's passed on of the page's request, and of Anthropic's answer
const ASKED = ['x-api-key', 'anthropic-version', 'anthropic-beta', 'content-type', 'accept'];
const ANSWERED = ['content-type', 'request-id', 'retry-after'];
// a request's pictures and files at most, in one call
const MOST = 64 * 1024 * 1024;

export async function onRequest({ request, params }) {
  const path = [].concat(params.path || []).join('/');
  if (!PASSED.some((p) => p.test(path))) return new Response('Not found', { status: 404 });
  if (!['GET', 'POST', 'DELETE'].includes(request.method)) return new Response('Not allowed', { status: 405 });
  // the site's own pages only (a page elsewhere has Anthropic's own address)
  const here = new URL(request.url);
  const origin = request.headers.get('origin');
  if (origin && origin !== here.origin) return new Response('Not from this site', { status: 403 });
  if (Number(request.headers.get('content-length') || 0) > MOST) return new Response('Too big', { status: 413 });
  const headers = new Headers();
  for (const h of ASKED) {
    const v = request.headers.get(h);
    if (v) headers.set(h, v);
  }
  const body = request.method === 'POST' ? await request.arrayBuffer() : undefined;
  if (body && body.byteLength > MOST) return new Response('Too big', { status: 413 });
  const answer = await fetch(API + path + here.search, { method: request.method, headers, body });
  const out = new Headers({ 'cache-control': 'no-store' });
  for (const h of ANSWERED) {
    const v = answer.headers.get(h);
    if (v) out.set(h, v);
  }
  // as it comes (a session's events stream for as long as it runs)
  return new Response(answer.body, { status: answer.status, headers: out });
}
