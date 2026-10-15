# content/: the game's own content

Everything the game is made of, as data: one JSON file per record, reviewed and merged
like code. This is what the public game ships.

- `data/<collection>/<id>.json`: the records. Collections: `types`, `creatures`, `moves`,
  `abilities`, `items`, `natures`, `growth_rates`, `egg_groups`, `regions`, `encounters`,
  `parties`. Fields: `crates/content/src/model.rs` (the source of truth).
- `assets/`: the pictures and sounds the records name. Paths in records are relative to
  this folder (`"front": "creatures/emberkit/front.png"` is `content/assets/creatures/emberkit/front.png`).
- `bundle.json`: generated for the browser (`cargo run -p engine-content --bin content -- bundle content`,
  run by Trunk before each web build); not committed.

Rules:

- Ids are lowercase slugs (`a-z`, `0-9`, `_`), the file name is the id, and records
  refer to each other by id.
- Mechanics are named by `effect` keys the engine knows (moves, abilities), never by a
  record's id or name: a new ability can reuse a mechanic under its own name.
- Only original work goes here: no names, texts, pictures or sounds taken from other
  games. Local placeholders live in `temporary/`, which is never committed; a record here
  with the same id replaces the placeholder.

Check everything resolves: `cargo run -p engine-content --bin content -- check`.

Big maps are chunk files: `data/maps/<id>.json` says `"chunk": 64` and lists its
`terrains`, and `data/maps/<id>/<x>_<z>.json` holds each 64 × 64 square's cells as rows of
numbers (terrain index, level, tree area, zone); a square with no file is blank.
`content chunk MAP` converts a map, `unchunk MAP` converts it back. Worlds (`data/worlds/`)
place maps as pieces on one grid of cells; pieces may touch (a seam, walked across where the
levels match) but never overlap.

Props' real models are glTF files (`.glb`) in `assets/models/`, named by the prop's `model`
field; the Blender add-on (`tools/blender/`) writes both. A prop without one is drawn from its
`parts`, a stand-in made of simple shapes. A prop's `light` is a spot light it gives off
(`kind` "spot", `color`, `intensity`, `range` in cells, `cone` in degrees, `at` and `aim` from
its origin before it's turned, `night`: lit by night only). A prop's `unstackable` means nothing overlaps it
and it overlaps nothing (stairs, beds, shelves); `folders` lists the asset browser's folders
it's in (`Buildings/Inside` and `Buildings/Outside` are what the Build tool offers inside and
outside a building), its category's folder when left out.

Everything in this folder is licensed under Creative Commons Attribution-ShareAlike 4.0
(`LICENSE` here); the code outside it is under MIT or Apache-2.0.
