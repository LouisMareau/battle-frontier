// Battle Frontier's service worker: what makes the site installable as an app, and lets it
// start without a network once it has been opened. The network comes first (a new version
// is used as soon as it's deployed); the cache answers when the network doesn't. The
// content (records, models, pictures, sounds) is kept as it's fetched; of the program
// itself (game-<hash>.js and _bg.wasm), only this version's is kept.
const CACHE = 'battle-frontier';

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET' || new URL(request.url).origin !== self.location.origin) return;
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy)).catch(() => {});
        }
        return response;
      })
      .catch(() => caches.match(request, { ignoreSearch: true }).then((hit) => hit || Response.error()))
  );
});

// The page says which program files it runs on: older versions' are dropped
const program = (url) => /\/game-[0-9a-f]+(_bg\.wasm|\.js)$/.test(new URL(url).pathname);
self.addEventListener('message', (event) => {
  const urls = event.data && event.data.keep;
  if (!Array.isArray(urls)) return;
  event.waitUntil(
    caches.open(CACHE).then(async (cache) => {
      const kept = new Set(urls);
      for (const request of await cache.keys()) if (program(request.url) && !kept.has(request.url)) await cache.delete(request);
    })
  );
});
