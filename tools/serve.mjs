// Serves the built site (build/dist, made by `trunk build --release`) and, next to it,
// the temporary space (temporary/, placeholders that are never committed or deployed),
// so the browser build plays with them locally:
//     node tools/serve.mjs [port]  -> http://localhost:8080/
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

// another built site than build/dist when DIST says so (`trunk build --release --dist DIR`):
// a check can run on its own build while another one is made
const root = process.env.DIST ? resolve(process.env.DIST) + sep : fileURLToPath(new URL('../build/dist/', import.meta.url));
const temporary = fileURLToPath(new URL('../temporary/', import.meta.url));
const TYPES = {
  '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json',
  '.png': 'image/png', '.css': 'text/css', '.wav': 'audio/wav',
};

/// `rewrite(path, body)`: a check's chance to change what's served (a Buffer back, or the same).
/// `delay(path)`: milliseconds to hold a file back before serving it (a model that comes late).
/// `handle(req, res, path)`: a check's own answer to a request (a stand-in for a service the
/// page calls, any method): true when it answered, the files served otherwise.
export function serve(port = 8080, { rewrite, delay, handle } = {}) {
  const server = createServer(async (req, res) => {
    if (handle && (await handle(req, res, new URL(req.url, 'http://x').pathname.replace(/^\/+/, '')))) return;
    const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^([/\\])+/, '');
    const local = /^temporary[\\/]/.test(path); // normalize() gives backslashes on Windows
    const base = local ? temporary : root;
    const file = local ? join(temporary, path.slice('temporary/'.length)) : join(root, path || 'index.html');
    if (!file.startsWith(base)) { res.writeHead(403).end(); return; }
    try {
      let body = await readFile(file);
      const web = path.split('\\').join('/');
      if (rewrite) body = rewrite(web, body);
      const hold = delay ? delay(web) : 0;
      if (hold > 0) await new Promise((r) => setTimeout(r, hold));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(body);
    } catch {
      res.writeHead(404).end('not found');
    }
  });
  return new Promise((resolve) => server.listen(port, () => resolve(server)));
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const port = Number(process.argv[2] || 8080);
  await serve(port);
  console.log(`http://localhost:${port}/`);
}
