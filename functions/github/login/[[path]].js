// The editor's sign-in with GitHub from the site (Publish, crates/game/src/mapedit/publish.rs):
// GitHub's device flow, its two calls passed on to github.com from the site's own address,
// as github.com's sign-in sends no CORS headers (its API does: the page calls that itself).
// Only these two, POSTed forms answered in JSON; nothing is kept here, and no secret is used
// (the device flow needs none).
//     POST /github/login/device/code          -> https://github.com/login/device/code
//     POST /github/login/oauth/access_token   -> https://github.com/login/oauth/access_token
const PASSED = new Set(['device/code', 'oauth/access_token']);

export async function onRequestPost({ request, params }) {
  const path = [].concat(params.path || []).join('/');
  if (!PASSED.has(path)) return new Response('Not found', { status: 404 });
  const body = await request.text();
  if (body.length > 4096) return new Response('Too long', { status: 413 });
  // github.com turns these calls away at times (429: the addresses they go out from are shared
  // with many other sites). Asking for a code is asked again, a little later each time, before
  // the refusal is passed on; whether the code's been typed is passed on as it comes (the
  // editor asks that again itself, less and less often, and gives up after a few in a row)
  const tries = path === 'device/code' ? 3 : 1;
  let answer;
  for (let attempt = 1; ; attempt++) {
    answer = await fetch(`https://github.com/login/${path}`, {
      method: 'POST',
      headers: { Accept: 'application/json', 'Content-Type': 'application/x-www-form-urlencoded', 'User-Agent': 'battle-frontier-site' },
      body,
    });
    if (!(answer.status === 429 || answer.status >= 500) || attempt >= tries) break;
    await new Promise((resolve) => setTimeout(resolve, 400 * attempt));
  }
  return new Response(await answer.text(), {
    status: answer.status,
    headers: { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' },
  });
}
