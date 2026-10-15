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
field; the Blender add-on (`tools/blender/`) writes both, and a `.glb` dropped on the
editor's asset browser gets its record written. The file keeps everything Blender exports
(materials, rigs, animations); the engine draws its triangles, base colours, textures and
vertex colours, and a rigged model plays its "idle" animation (else its first) in a loop.
A prop without one is drawn from its `parts`, a stand-in made of simple shapes. A prop's `light` is a light it gives off (`kind` "spot", a cone, or
"point", all around; `color`, `intensity` (1 is 1000 W in Blender), `range` in cells, a
spot's `cone` in degrees, `at` and `aim` from its origin before it's turned, `night`: lit by
night only): in Blender, a spot or point light object on the prop. A prop's `sound` is a
speaker on it (`file` under `assets/`, `volume`, `range` in cells, `at`): it loops where
the prop stands, fading with distance. A creature's `light` is the same, carried from its
feet with +z the way it faces (a tail light: `"at": [0, 0.5, -0.4]`, `"aim": [0, -0.3,
1]`); its `sound` is a loop it makes as it goes about, and its `cry` the one-shot it gives
when a battle starts. A prop's `unstackable` means nothing overlaps it
and it overlaps nothing (stairs, beds, shelves); `folders` lists the asset browser's folders
it's in (`Buildings/Inside` and `Buildings/Outside` are what the Build tool offers inside and
outside a building), its category's folder when left out.

A map's `actors` are the people, creatures and things with a life of their own: `id`,
`name`, `look` (`creature:<id>`, `sprite:<picture under assets/>` or `prop:<id>`), `at`,
`level`, `facing` (degrees clockwise from south), `moves` (`still`, `wander` within `area`
(x, z, width, height) or `path` along `path`'s cells), `speed` in cells a second, and its
`events`. The map's `events` and an actor's each have an `id`, a `trigger` (`interact`:
spoken to, `enter`: stepped on, `start`: the map starting), an `area` of cells (a map
event's), `once`, `if` and `unless` (flags), and a `script`: one action a line, in plain
words: `say [Who:] text`, `ask text -> label | text -> label`, `label name`, `goto name`,
`if flag goto name`, `unless flag goto name`, `set flag [value]`, `add flag value`, `clear
flag`, `move who x,z [x,z ...]`, `face who n|s|e|w|player|<actor>`, `wait seconds`, `show
who`, `hide who`, `sound file [volume]`, `camera x,z|who`, `warp map x,z`, `battle
encounter`, `end` (`who` is an actor's id or `player`; `crates/content/src/script.rs`). The
check reads every script: an action that isn't one, a label or an actor that isn't there,
a map or an encounter that doesn't exist. A map's `ambience` lists the sounds of the
place: `file` under `assets/`, `volume`, `when` (`always`, `day`, `night`, `morning`), and
for a sound of one spot its `area` and a `fade` in cells past its edge.

A placed prop (`props` on a map, on a building's floors and outside it) stays on its cells:
`at` is their centre, and `offset` (x, z from it, within its cells) is where it stands
within them.

`templates/<id>.json` keeps things to put down again anywhere: `name`, `size` (cells along
x and z), and `props`, `buildings`, `actors` and `events` as a map has them, placed from the
template's north-west corner with levels from its lowest ground. Its scripts may name
actors it doesn't hold (they're on the maps it's put down on); everything else is checked
like a map's.

Everything in this folder is licensed under Creative Commons Attribution-ShareAlike 4.0
(`LICENSE` here); the code outside it is under MIT or Apache-2.0.
