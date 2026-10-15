// The encounters the checks play, found in the content by what they are (so the scripts
// name none): both spaces' records, the game's own over placeholders with the same id.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));

export function encounters() {
  const all = new Map();
  for (const space of ['temporary', 'content']) {
    const dir = join(root, space, 'data', 'encounters');
    if (!existsSync(dir)) continue;
    for (const f of readdirSync(dir).filter((f) => f.endsWith('.json'))) {
      const e = JSON.parse(readFileSync(join(dir, f), 'utf8'));
      all.set(e.id, e);
    }
  }
  return [...all.values()].sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

/// The first encounter of a kind ("gym", "wild", "trainer"), the nth one if asked, with a
/// battle size if asked.
export function pick(kind, { nth = 0, size } = {}) {
  const list = encounters().filter((e) => e.kind === kind && (size === undefined || e.size === size));
  return list[nth]?.id;
}
