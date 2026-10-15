# temporary/: local placeholders, never committed

Everything in this folder except this file is ignored by git and must never be pushed.

It holds placeholders the author imports locally with a private tool (not part of the
repository): creatures, moves, abilities, types, items, pictures, battle maps and
encounters to build and test with before the game has its own content. Every record here
is meant to be replaced by an original one in `content/` (same layout). Where both spaces
have a record with the same id, the one in `content/` wins, and the editor flags
everything still temporary.

Layout (same as `content/`):

- `data/<collection>/<id>.json`: one record per file (see `crates/content/src/model.rs`)
- `assets/`: the pictures and sounds the records name (paths in records are relative to it)
- `bundle.json`: every record in one file, for the browser

Local only: `node tools/serve.mjs` serves this folder next to the built site; a deployed
site has none of it.
