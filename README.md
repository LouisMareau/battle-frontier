# Battle Frontier

An original creature-catching RPG and its engine, in Rust: one program for the browser
(WebAssembly, on WebGPU or WebGL 2) and for the desktop, holding both the game and the
tools to build it (a content browser and a map editor so far). "Battle Frontier" is a
working name. Battles are its own: timed hits and blocks on a classic RPG menu, MP, moves
that level up and change shape, locks on big attacks, rounds ordered by Speed, battles that
grow up to 6v6, staged in 3D on the overworld map, day and night.

Everything the game is made of is data, in `content/`. Until it has its own creatures and
moves, the author develops with local placeholders in `temporary/`, a space that is never
committed (see below): without them the game has no battles yet, and the editors work.

## Running it

Needs Rust (stable, with the `wasm32-unknown-unknown` target), [Trunk](https://trunkrs.dev)
for the web build, Node 22+ and a Chrome for the checks. Build output goes to `build/` (set in `.cargo/config.toml`, with 8 parallel jobs).

```bash
cargo run -p engine-content --bin content -- check   # what's in content/ and temporary/, every broken reference
cargo test -p engine-core              # the battle rules on their own fixtures: 15 tests, incl. 60 simulated battles and every mechanic
cargo test -p engine-world             # maps: records round-trip, tree areas
cargo run -p engine-game --release     # the desktop game (run from this folder, or set GAME_ROOT)
trunk build --release                  # the site in build/dist/ (content/ only, never temporary/)
node tools/serve.mjs                   # serves the site at http://localhost:8080, with temporary/ next to it
node tools/check.mjs [seconds=60]      # plays battles in a headless Chrome (needs creatures and encounters), frame times -> screenshots/
node tools/frames.mjs [encounter]      # frame times of one encounter, second by second
node tools/perf.mjs [tile=4] [size=3840x2160]   # how fast it draws, uncapped: menu, map views, editing, walking, a battle
node tools/mapcheck.mjs [map=ID]       # drives the map editor (paint, levels, props, trees, undo, download)
node tools/buildcheck.mjs              # drives the Build tool (footprint, doors, floors, confirm, walk in, turn, move)
node tools/zonecheck.mjs               # creatures roaming, the debug view, a zone painted and linked to a treeline
node tools/assetcheck.mjs              # the asset browser: folders, a row dragged, a .glb dropped in, unstackable, saves
node tools/worldcheck.mjs              # the World view: a piece dragged into place, the world saved, a walk across a seam into a chunked map
cargo run -p engine-content --bin content -- chunk MAP    # a map stored as chunk files (unchunk: rows again)
cargo run -p engine-world --bin export-glb -- map test_meadow   # a map (or prop ID, props, building MAP ID) as .glb for Blender
node tools/shot.mjs "map:test_meadow"  # one screenshot of a state, from commands
```

The desktop build takes `--preset ID` (start that encounter), `--auto` (the AI plays your
side), `--time morning|day|night`, `--levels 1-5` (every move at that level), `--maps [ID]` (the
map editor), `--exit-after SECONDS`, `--stats` (frames per second, the worst frame
gap and the time each frame keeps the processor busy, printed every second) and `--no-vsync`
(draws as fast as it can, not at the screen's rate: with `--stats`, what the machine could
do). The browser build takes the same as
`<body data-cmd="start:ID;auto:1">` (also `editor`, `maps`, `map:ID`, `back`, `scale:50-100`
and `shadows:off|low|medium|high`) and
writes `data-state`, `data-round`, `data-fps`, `data-maxgap`, `data-cpu`, `data-cpupeak`,
`data-draws`, `data-tris`, `data-shadowtris`, `data-culled`, `data-pending` (the timing
read in progress), and in the map editor `data-map`, `data-mapsize`, `data-tool`, `data-level`,
`data-props`, `data-areas`, `data-dirty`, `data-buildings`, `data-build`, `data-draft`,
`data-selection`, `data-walker`, `data-zones`, `data-links`, `data-roamers`, `data-roaming`,
`data-pool`, `data-rooms` (a building's inside and outside while it's designed), `data-status`,
`data-assets`, `data-folder`, `data-shown`, `data-dropped`, `data-models`, `data-chunked`,
`data-loading`, `data-squares` (the ground's squares held / in the map), `data-world`,
`data-pieces` and the places of the buttons the checks press (`data-ui-*`) on `<body>`;
`view:x,z,distance,yaw,pitch` points the editor's camera and `point:x,y,z` reports where a
world point is on the screen (`data-point`): that's what the checks drive.

## Content

Two spaces with the same layout, `data/<collection>/<id>.json` (one record per file) and
`assets/` (the pictures and sounds the records name):

| Space | What | In git |
|---|---|---|
| `content/` | The game's own content. What the public game ships. | Yes |
| `temporary/` | Local placeholders, imported by a private tool that isn't part of the repository: creatures, moves, abilities, items, types, pictures and battle maps to develop with until the game has its own. | Never (`.gitignore`) |
| `private/` | The author's local tools and prototypes. | Never (`.gitignore`) |

The collections: `types`, `creatures`, `moves`, `abilities`, `items`, `natures`,
`growth_rates`, `egg_groups`, `regions`, `encounters`, `parties`, and for the world
`settings`, `terrains`, `props`, `maps`, `spawns`; their fields are in `crates/content/src/model.rs`.
`content format` rewrites `content/`'s records in the layout the editor saves (short lists
on one line, one map row or placed prop per line), so every change reads as a small diff. Ids are lowercase slugs and records refer to each other by
id. A record in `content/` replaces the temporary one with the same id, which is how
placeholders get replaced one by one; the content browser (menu: "Browse the content")
shows every record with its space and flags each one that's still temporary.

Mechanics are named by `effect` keys the engine knows, never by ids or names: abilities
and moves carry an `effect`, types list the statuses they're immune to, and the few rules
about particular types (weather and water/fire, ground against electric paralysis...) find those
types by id and simply don't apply if the content has none. A brand new ability can use an
existing mechanic under its own name.

The browser loads each space as one `bundle.json` (`content bundle content`, which Trunk
runs before every web build; the import writes temporary/'s). A deployed site only has
`content/`; locally, `tools/serve.mjs` adds `temporary/` next to it.

## Maps and the map editor

A map is a grid. Each cell has a terrain and the level its ground stands at; a level is
the main character's height plus some room (`settings/world.json`: 1.5 + 0.5 cells), so
floors of buildings will stack on the same steps. Cells are stored as rows of characters
(`terrain` with a `legend`, `levels` as 0-9 then a-z, `areas` for tree areas), props as a
list (`prop`, `at` in cells, `level`, `turn`, `scale`).

A big map is stored as **chunk files** instead: the record keeps its lists and says
`"chunk": 64` with a `terrains` list, and the cells live in `data/maps/<id>/<x>_<z>.json`,
one file per 64 × 64 square, as rows of numbers (the terrain's index in that list, the level
as a number, so neither has a limit) with the tree areas and zones the same way. A square with
no file is blank, so a large, mostly empty world costs little; an edit touches few files. The
editor reads either form and saves the one the map asks for ("Chunk files" in the inspector,
or in the New map box); `content chunk MAP` and `unchunk MAP` convert; `content check` reads
every chunk file. On the GPU a map is held square by square whatever its storage: the squares
within reach of the camera are built (a few per frame, nearest first) and the far ones dropped,
and an edit rebuilds only the squares it touched, so a 320 × 240 map edits and draws like a
small one.

- **Terrains** are painted per cell; the border between two is drawn by the ground shader
  (the four nearest cells vote with noise and priority), so edges come out organic and
  corners round without border tiles. Patterns (grass, meadow, tall grass, dirt, sand,
  cobbles, rock, water) are procedural for now. Raised cells get cliff faces.
- **Props** are the asset database: footprint in cells, height, whether they block, whether
  they are `unstackable` (nothing overlaps them and they overlap nothing: stairs, beds,
  shelves, wells...), the asset browser's `folders` they're in, a `light` they give off (a
  spot: colour, intensity, range, cone, where it sits and points from the prop's origin,
  at night only or always; lamp posts and wall lamps have one), and a stand-in model made of
  simple shapes (`parts`) until a real model (glTF, `model`) comes. Trees and flora can't be
  scaled; the rest can be scaled down.
- **Tree areas** are cells the player can't cross. The engine fills them with trees from
  the area's settings (which trees, spacing, size range, density, a tree in every edge
  cell, seed): the same trees every time.
- **Buildings** have a footprint of cells (any shape, in one piece), floors one level each
  (each with its flooring, never empty), walls along the outline in a style (plaster,
  planks, bricks, stone, logs), doors and windows in them (an opening is as wide, in cells,
  as its asset: a 2-cell span only offers 2-cell doors), what stands inside each floor,
  pillars on points of the outline, a roof (gabled on a rectangle unless `flat_roof` asks
  for a flat one; flat with a parapet on any other shape), things hung on its walls
  (`anchors`, outside or `inside`) and what stands outside next to it (`outside`). Stairs
  on a floor open the floor above over them (nothing stands there) or, on the top floor, a
  flat roof; under a gabled roof they're refused. Walls block and doors let the player
  through; a building flattens the ground under it and clears trees.

- **Worlds** stitch maps together: a world record (`worlds/<id>.json`) places maps as
  pieces on one grid of cells (`"pieces": [{ "map": ..., "at": [x, z] }]`). Where two pieces
  touch they share a seam; walking off a map's edge goes on in the piece there, where the
  levels match on both sides (the content check notes seams whose levels differ; it refuses
  pieces that overlap). Interiors stay separate maps.
- **Zones** are areas creatures roam (`zone_cells` rows, `zones`), and **links** send
  creatures out of a treeline (edge cells of a tree area) to a zone: they walk out of the
  trees, roam the zone for a while, then walk back in. Creatures move freely, not cell by
  cell, around whatever blocks, and stay inside their zone while roaming (`crates/world/src/spawn.rs`).
  Who lives in each zone, by time of day, is the map's **spawns** record
  (`spawns/<map>.json`), kept apart from the map because it names creatures: the editor
  saves it in `temporary/` while it names placeholders, in `content/` once it only names
  the game's own creatures (the check refuses a `content/` record naming a placeholder).

The editor (menu: "Edit maps", or `--maps`): the tools and the level (PageUp/PageDown) are
at the top of the left panel, the tool's settings under them; the right panel inspects what's
selected (a prop, a building), or the map itself (name, size, what's on it); the bar at the
bottom shows the cell under the pointer and the last message.
**Ground** paints terrain and, with "set the level", raises or lowers cells to it
(right-click picks a cell's terrain and level); **Place** puts a prop at the level (Q/E turn
it, Shift places it off the grid); **Select** moves (drag), turns, scales and deletes props;
**Trees** paints tree areas and edits their settings; **Build** draws a building's
footprint, then designs it floor by floor: the floors above are cut away and the rest of the
map is darkened, and nothing lands outside the building. *Structure* is flooring, walls and
roof (a flat roof for a rectangle, if you want one), doors and windows (drag along an outside
wall, then pick an asset that wide) and pillars;
*Inside* places what the `Buildings/Inside` folder holds on the floor being designed (on its
inner walls for things that hang); *Outside* asks for a wall first (a court's walls count),
then hangs what `Buildings/Outside` holds on it or stands it on the ground up to two cells in
front of it. Click a placed thing to select it: Q/E turn it, Delete (or its "remove" button)
removes it; unstackable props never overlap anything. Then "Confirm the building". A
building on the map is selected with Select: Edit (the same modes), turn (Q/E), move (drag),
delete. **Walk** puts a stand-in character on the map: the
arrows move it cell by cell, and the building it enters is cut away above it. **Zones**
paints zones (and who lives there: morning, day, night) or, in "Link treelines", takes a
drag from a forest's edge onto a zone. "Creatures" lets them roam the map; "Debug" shows
every zone, tree area, link arrow and creature path. Right-drag pans, middle-drag turns
the view, the wheel zooms; Morning/Day/Night preview the light, and the lamps light up by
night (Lamps lights them whatever the time); Ctrl+Z/Ctrl+Y undo and redo. **World** opens the World view across the window: the world this map is part of as
pieces on a grid (the map being edited lit), seams green where the levels match and crimson
where they don't; drag a map in from the list, drag pieces into place (they snap to cells),
double-click a piece to edit that map, "New world" starts one with this map, "Save world"
writes `content/data/worlds/<id>.json` (a download in the browser). With a world, Walk
crosses from one map into the next at a matching seam (the map must be saved first). Save writes `content/data/maps/<id>.json` on the desktop; the browser downloads the
file. Maps made here are the game's own content.

**Assets** (in the top bar, or drop a file on the window) is the asset browser: every prop
of both spaces as a detailed list, with its GAME or TEMPORARY tag, category, size, model or
stand-in, what it does (blocks, unstackable, on walls) and folders, shown by folder
(`Buildings/Inside`, `Buildings/Outside`, `Nature`, `Town`...: a prop's `folders`, or its
category's). Drag a row onto a folder to put the prop there too (a chip's × takes it out);
click a row to use it (the Place tool, or a building's Inside or Outside list when it's in
that folder); "light" on a row opens the prop's light (colour, intensity, range, cone, where
it sits and points) or gives it one; drop `.glb` files on a folder and each becomes a prop there, its footprint and
height measured from the model: written at once on the desktop (`content/`, or `temporary/`
with the box ticked), kept until Save in the browser (its record and model download). On the
desktop, `data/props` and `assets/models` of both spaces are watched: a record or model
changed there (the Blender add-on's Add to project, a re-export) shows within a second.

## Blender

Props, buildings and maps go to Blender, and models made in Blender come into the game, as
glTF (`.glb`). A prop with a `model` (a `.glb` under its space's `assets/models/`) is drawn
with it, textures included; one without keeps its stand-in made of shapes. The map editor
exports the map ("To Blender"), the selected prop (Place) and a building (its inspector) to
`build/exports/` (a download in the browser); so does `export-glb`. The Blender add-on in
`tools/blender/` finds the project on the computer, imports its props (models, or stand-ins
rebuilt from their shapes), exports and imports maps, and adds a selection to the project as
a prop: the `.glb` and its record, measured and laid out as the project's own (see
`tools/blender/README.md`). Same frame everywhere: one unit is a cell (a metre), the asset
on the ground at the origin, its front to Blender's front view.

## Layout

| Crate | What |
|---|---|
| `crates/content` (`content`) | The content model (`model.rs`: records, maps as rows or as chunk files, worlds and their seams; buildings and their checks in `building.rs`), loading spaces from disk or bundles, the game's own over temporary records, validation of every reference, bundles. The `content` tool: `check`, `bundle`. |
| `crates/core` (`rules`) | The battle rules, no rendering, no platform code: `battle.rs` (rounds by Speed, actions, damage, locks, battle sizes), `moves.rs` (upgrade tracks, Focus/Chain/Storm, MP cost, level texts), `mon.rs`, `ai.rs`, `config.rs` (every tuning number), `dex.rs` (the content made fast to look up: indexes, type chart and roles, ability effects). A port of the prototype's `src/engine/*.js`; `tests/battle.rs` is its `tests/battle.test.js`. |
| `crates/world` (`world`) | Maps as the engine works on them: the record decoded into a grid and back (`lib.rs`), procedural tree areas (`trees.rs`), which cells block and where walls are, buildings' geometry (`buildings.rs`), roaming creatures (`spawn.rs`), glTF models in and out (`gltf_in.rs`, `gltf_out.rs`, the `export-glb` tool), and the geometry (`mesh.rs`: cell tops, cliff faces, tall grass, props' stand-in models from their parts). |
| `crates/render` (`render`) | The renderer on wgpu 30: battle maps (`map.rs`: ground, boxes, cutouts, the battle spot), world maps (cell tops through the ground shader, which draws terrain borders from a texture of the cells; vertex-coloured cliffs, props and trees), billboards, discs, rings, rectangles, glows, rain, the sun's shadow map, day/night light and fog (`scene.rs`, `shader.wgsl`). Draws into its own target, then blits into the host's pass. Uniform buffers only, so WebGL 2 works as well as WebGPU. |
| `crates/game` | The program (eframe 0.36, as GBMapping): `app.rs` (menu, loading, battle, results, frame stats, test hooks), `editor.rs` (the content browser), `mapedit.rs` (the map editor; `mapedit/build.rs` the Build tool, `mapedit/rooms.rs` inside and outside a building, `mapedit/assets.rs` the asset browser, dropped files and the disk watch, `mapedit/stream.rs` the map on the GPU square by square, `mapedit/worlds.rs` the World view, `mapedit/walk.rs` walking (across seams too), `mapedit/zones.rs` zones, links and roaming creatures, `mapedit/models.rs` props' models and exports to Blender), `director.rs` (turn flow, event playback, the animation + timing state machine), `stage.rs` (battlers, motions, effects, camera, light), `hud.rs` and `menu.rs` (egui), `input.rs` (A/B/d-pad/L/R; tap, hold-release and rhythm reads), `presets.rs` (battles from encounters), `platform.rs` (clock, files, hooks). |
| `tools/` | `serve.mjs`, `check.mjs`, `frames.mjs`, `perf.mjs`, `encounters.mjs` (which encounters the checks play, by kind), `mapcheck.mjs`, `buildcheck.mjs`, `zonecheck.mjs`, `assetcheck.mjs`, `worldcheck.mjs`, `blender/` (the Blender add-on and its test), `shot.mjs`, `cdp.mjs` (headless Chrome). |

## Frame pacing

Nothing in a battle steps per frame: the rules are event-driven, animations and timing
windows run on a millisecond clock (`performance.now()` in the browser), so the battle
looks and plays the same at 60 or 120 Hz. Timing presses are stamped with the time of the
frame that sees them (at most one frame late, against windows of ±110 ms or more). The
renderer reuses its buffers (the sprite buffer only grows), so a frame allocates very little.
The overworld, when it comes, will step its movement at a fixed 60 Hz. Every pipeline draws
once offscreen at startup, so a browser that compiles shaders on first use doesn't do it
mid-battle.

Measured on 2026-10-14 (RTX 3080), a triple battle played by the AI:

| Where | Frames per second | Worst frame, per second |
|---|---|---|
| Desktop build (DX12/Vulkan, 144 Hz screen), 20 s | 144-145 | 7-10 ms (one frame = 6.9 ms) |
| Browser, headless Chrome on WebGPU, 60 s (`tools/frames.mjs`) | 56-58 (headless Chrome ticks at ~57 Hz) | 18-19 ms; one frame over 25 ms (25 ms, a battle restarting) |

The only long frame on either is the first one, while the battle loads.

## Performance

The aim is 120 frames per second on a 120 Hz screen, 60 at least on modest machines. `F3`
shows the details in the game (time per frame, draw calls and triangles in the scene and the
shadow, what was left out of sight, the graphics settings). In the menu, **Graphics** sets
the 3D scene's resolution (100, 75 or 50% of the screen's, smoothed up) and the shadows (off,
low, medium, high: a 1024 to 4096 shadow map) for machines that can't keep up.

What keeps it fast:

- World meshes (ground, cliffs, props, trees, buildings) are built square by square (64 cells,
  only the squares within reach of the camera, a few per frame; the far ones are dropped) and
  cut into 16 x 16 cell pieces with their bounds; a piece out of the camera's sight isn't drawn,
  nor one out of the sun's in the shadow pass. A big map costs what's in reach, not its size,
  and an edit rebuilds the squares it touched: on a 320 x 240 map a prop placed costs 6 ms.
- Nearest first, and props without cut-outs on a shader with no `discard`: the GPU drops what's
  hidden before shading it. The ground (the costliest shader) is drawn last, under the rest.
- The ground shader skips the border votes wherever a cell and its neighbours have one
  terrain: most of the ground.
- The map editor sends props, trees and buildings to the GPU as separate layers: placing a
  prop doesn't send the forest again.
- Test hooks on `<body>` are written only when they change.
- Lamps are spot lights in the shader, the 32 nearest the camera each frame (no shadows from
  them); a light carried by a moving creature fits the same way, since the list is per frame.

Shadows are looked up a little off each surface, along its normal (1.5 shadow texels), with
a margin of half a texel: both in world units, so shadows start at their casters' feet at any
zoom (a fixed margin in the light's depth used to part them by up to a world unit). The sun's
view moves in whole shadow texels, so shadow edges stay still when the camera moves, and its
coverage grows in steps when the camera zooms out.

`tools/perf.mjs` measures it uncapped (Chrome without vsync or its 60 FPS limit): frames per
second are then what the machine could do. `tile=N` plays a map made of N x N copies of the
test meadow (added to the served bundle, nothing written); `profile=NAME` samples where the
time goes. Measured on 2026-10-15 (RTX 3080, WebGPU), before and after the work above:

| 3840 x 2160, the meadow x 16 (160 x 120 cells, 1M triangles) | Before | After |
|---|---|---|
| Close view | 1009 fps (0.99 ms) | 1610 fps (0.62 ms) |
| Mid view | 555 fps (1.8 ms) | 1080 fps (0.93 ms) |
| The whole map in view | 471 fps (2.1 ms) | 727 fps (1.4 ms) |
| Walking (close camera) | not measured | 895 fps (1.1 ms) |
| A prop placed in the map editor: worst frame | 57 ms | 7.4 ms |

At 1400 x 900 on the meadow itself, every scene runs at ~2400 fps uncapped (~0.4 ms a frame),
a battle at ~3800. A 320 x 240 map (`tile=8`, 1.8M triangles in view) draws whole at 4K at
379 fps, and a prop placed on it costs 6 ms (`only=editing`).

## Not yet

- The content browser only reads; the map editor saves maps (other records next).
- Maps: creatures roam but don't battle yet (walking into one does nothing), no stairs or ramps between
  levels (walking stays on one level, so upper floors aren't reachable on foot), no
  interiors as separate maps, no lights at night; most props are still stand-in shapes
  (a sample made in Blender: `stone_well`).
- Battle items (Potion, Capture Ball...) are still a fixed list in `crates/core`, not content.
- No overworld yet (walking, map loading and connections, scripts).
- Sprites are battle pictures on billboards. The renderer can take 3D models later
  (glTF), the battle code doesn't care.
- No gamepad or touch yet (keyboard and mouse), no sound.

## Checks and deployment

Every pull request and push to `main` runs `.github/workflows/checks.yml` on GitHub: the
tests, clippy with warnings as errors, `content check`, the site built by
`tools/cloudflare-build.sh`, a check that the site holds nothing from `temporary/`, a scan
for other games' brand names (`tools/check-names.py`; the names are kept as hashes in
`tools/forbidden-names.sha256`, so they never appear in the repository),
and, on pull requests, a `Signed-off-by` line on every commit (`tools/check-signoff.sh`).
Nothing is deployed there and no secret is used, so pull requests from forks are safe.

Cloudflare Pages deploys the site through its Git integration: framework "None", build
command `bash tools/cloudflare-build.sh` (it installs Rust and Trunk when missing), output
directory `build/dist`, production branch `main`; the repository's other branches get
preview addresses.

## License

The code (everything outside `content/`) is under the MIT license or the Apache License
2.0, at your choice (`LICENSE-MIT`, `LICENSE-APACHE`). The game's content (`content/`:
records, maps, props, and pictures and sounds as they come) is under Creative Commons
Attribution-ShareAlike 4.0 (`content/LICENSE`). Contributions are welcome as pull requests;
see `CONTRIBUTING.md` (original work only, commits signed off).
