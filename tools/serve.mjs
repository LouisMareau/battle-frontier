// Serves the built site (build/dist, made by `trunk build --release`) and, next to it,
// the temporary space (temporary/, placeholders that are never committed or deployed),
// so the browser build plays with them locally:
//     node tools/serve.mjs [port]  -> http://localhost:8080/
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../build/dist/', import.meta.url));
const temporary = fileURLToPath(new URL('../temporary/', import.meta.url));
const TYPES = {
  '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json',
  '.png': 'image/png', '.css': 'text/css', '.wav': 'audio/wav',
};

/// `rewrite(path, body)`: a check's chance to change what's served (a Buffer back, or the same).
export function serve(port = 8080, { rewrite } = {}) {
  const server = createServer(async (req, res) => {
    const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^([/\\])+/, '');
    const local = /^temporary[\\/]/.test(path); // normalize() gives backslashes on Windows
    const base = local ? temporary : root;
    const file = local ? join(temporary, path.slice('temporary/'.length)) : join(root, path || 'index.html');
    if (!file.startsWith(base)) { res.writeHead(403).end(); return; }
    try {
      let body = await readFile(file);
      if (rewrite) body = rewrite(path.split('\\').join('/'), body);
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
