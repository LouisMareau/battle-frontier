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

**Play it in the browser: [battle-frontier.pages.dev](https://battle-frontier.pages.dev)**
(the latest of `main`, deployed by Cloudflare Pages). It installs as an app: in Chrome or
Edge, the install button at the end of the address bar (or the menu's "Cast, save and
share" > "Install page as app"). Installed, it opens in its own window, and once opened it
starts without a network too.

The game opens on its main menu, over the world: the map new games start on, its creatures
and people about, the camera drifting slowly over it, lit at the time it is where you are.
**Continue** (the saved story, where you left it), **New game** (asked first when it would
replace a saved story), **Options** and **Exit** (in the browser, a tab the page may not close
says so), in white Roboto: an item hovered grows a little and steps right; one picked steps
right while the others slide left and fade (every tween 0.3 s at most). The **Options** are in
tabs, each in categories that a click on their name closes and opens (a closed one is a
single row for the game's buttons, A opening it): **Game** (Battles: text speed, timing
windows and rings; World: the time of day), **Graphics** (Quality: resolution, shadows;
Post-processing: **Pixelation**, the world in big square pixels in 15-bit colour, its **Pixel
density** from 0, the fewest (about as many as the GBC's 160 by 144 screen had, with the
camera 16 cells off), to 1, the screen's own (none), the pixels bigger as the camera comes
closer and smaller as it pulls back, so what's further off keeps its shape (the overworld's
camera stands back further than that, so its pixels are smaller there); **LCD screen**, the colours as the GBA's or the
GBC's screen showed them, with the thin gaps between its pixels when the pixelation is on:
the colour transforms of the Pipit emulator, "gba-color" for the GBA, Gambatte's for the GBC;
the editor keeps the screen's own look), **Controls** (Buttons and D-pad: what each button
answers to on the device in use: the keyboard's keys, or a controller's buttons while one
is in use; one clicked, or A on it, takes the next one pressed, "+" adds one; while one
waits, Delete or the controller's Select takes it off, Esc or its Start keeps it; a
right-click takes one off too) and **Audio** (Volume: master, effects, ambience); they're kept between runs, in the browser's storage or
`build/save/options.json`. The mouse works the menus, and so does a controller (or the
keyboard), all of them, with nothing left for the mouse: in the Options, L and R go through
the tabs, the d-pad through the rows and their values (a ring round what it's on), A presses.
The Editor's icon is the one exception: the editor is made for a mouse.

**Controllers** work everywhere the game reads its buttons (the menus, the Options, the
overworld, the battles), read by [gilrs](https://gitlab.com/gilrs-project/gilrs): the
system's own way on the desktop (Windows.Gaming.Input on Windows), the Gamepad API in the
browser, where a controller shows once one of its buttons is pressed. As they come: A is the
lower face button (A, Cross), B the right one (B, Circle), the d-pad or the left stick moves,
LB and RB are L and R, Start (Menu, Options, Plus) puts the ring on the overworld's buttons as
Esc does. The game
listens to one device at a time: a controller connected (or there at the start) takes over,
and whichever device is used while the game's window is active takes over from the one
before (a key, a click or the mouse moved for the keyboard; a button or the stick for a
controller; nothing counts while the window isn't the one in use). A controller come or gone
says so at the foot of the screen; with one in use, the mouse pointer hides, and the hints
name its buttons as printed on it (A, B, X, Y; Cross, Circle, Square, Triangle; or A on the
right and B at the bottom, told by its maker's USB number).

The time of day is the real one, where you are, hour by hour: the sun crosses the sky, dawn
and dusk colour it, the lamps come on at dusk. The **Editor**'s icon (a white pencil, bottom right) opens the tools
the game is made with; the first time in a run it says what they are and how what you make
gets into the game: a pull request (see `CONTRIBUTING.md`), until you tick "Don't show this
again". The game's icons are SVG (`crates/game/icons/`); its text is Roboto (Light, Medium
for headings, Bold for titles).

## Running it

Needs Rust (stable, with the `wasm32-unknown-unknown` target), [Trunk](https://trunkrs.dev)
for the web build, Node 22+ and a Chrome for the checks. Build output goes to `build/` (set in `.cargo/config.toml`, with 8 parallel jobs).

```bash
cargo run -p engine-content --bin content -- check   # what's in content/ and temporary/, every broken reference
cargo test -p engine-core              # the battle rules on their own fixtures: 15 tests, incl. 60 simulated battles and every mechanic
cargo test -p engine-world             # maps: records round-trip, tree areas
cargo run -p engine-game --release     # the desktop game (run from this folder, or set GAME_ROOT)
cargo run -p engine-game --release -- --explore [ID]   # straight into the overworld (the saved map, or ID)
trunk build --release                  # the site in build/dist/ (content/ only, never temporary/)
node tools/serve.mjs                   # serves the site at http://localhost:8080, with temporary/ next to it
node tools/check.mjs [seconds=60]      # plays battles in a headless Chrome (needs creatures and encounters), frame times -> screenshots/
node tools/frames.mjs [encounter]      # frame times of one encounter, second by second
node tools/perf.mjs [tile=4] [size=3840x2160]   # how fast it draws, uncapped: menu, map views, editing (a prop, strokes of each kind, an undo), walking, a battle
node tools/mapcheck.mjs [map=ID]       # drives the map editor (paint, levels, props, trees, undo, download)
node tools/buildcheck.mjs              # drives the Build tool (footprint, doors, floors, confirm, walk in, turn, move)
node tools/tilecheck.mjs               # cell by cell: flooring, wall styles, a hole upstairs (walked up to)
node tools/floorcheck.mjs              # floors laid with a set of tiles: a house's floor painted all over with the flagstones, every two cells side by side agreeing (each long slab whole), laid again another way, a slab put by hand staying as put, a long half put mirrored across the house's middle with the set laid round both; the grass out of doors laid so too (the Ground tool's Floors tab), a cell taken away, halves put with quarter turns round a corner picked on the map and with both mirrors, taken away all round and undone in one step; the tiles, turns, mirrored and hand-put ones in the download
node tools/rampcheck.mjs               # ramps: one dragged along a step's top, refused two levels down, two rows deep or where nothing's a level down, walked up, down and along (not off its sides, nor up the cliff beside), picked, smoothed, removed and back, saved; stone stairs from the asset browser refused off a ramp and laid along one (every tread stone, flush at its foot and top), a mud slide on one rising west, both saved; gone when a cell of it is painted to another level, the stairs with it
node tools/levelcheck.mjs              # painting levels: the brush on the cell under the pointer, on its own ground, whatever the level picked (raised from the lowland, sunk from a hill's top), a stroke held still painting its own cells and no more, ground raised to meet a hill's top joining it with no seam, saved
node tools/zonecheck.mjs               # creatures roaming, the debug view, a zone painted and linked to a treeline
node tools/menucheck.mjs               # the main menu: Continue, New game (and its question), the Options kept, the Editor's welcome and Menu, where new games start ("Start new games here", picked on the map: clicked, dragged to face a way, turned, marked, in the World view), Publish in the browser against a stand-in GitHub (its sign-in starting with a token, signed in by a code instead, the start saved sent as a pull request from a fork, signed out; a code GitHub keeps turning away given up on, a token pasted), and what the editor saves kept in the browser (nothing downloaded: a cell painted saved by itself, all of it there when the page opens again, listed in "Saved in this browser", one zip of it, then forgotten)
node tools/padcheck.mjs                # controllers (stand-ins for the Gamepad API): taking over and handing back, the menu, the Options, the overworld
node tools/hudcheck.mjs                # the overworld's HUD: its groups, Esc, the Team's list, always a main, the last battles, View all's board of cards (its order, sorts, a type, a card's pin, a creature's details and moves, one lost for good), a controller; the pixelation (as dense as asked, with the camera's zoom: hook `pixelzoom`) and LCD, an Options category closed and opened
node tools/watercheck.mjs              # the wind and the water: inland foam on the shore the wind blows to, the sea rougher onshore than offshore, the tall grass and trees swaying (still when calm), a leaf made two-sided fluttering as one (its back never through its top), the clouds' shadows drifting the wind's way, changing shape, unbroken across a cliff, the Post-processing window's Wind
node tools/layoutcheck.mjs             # the editor's panels: as they first come, floated and docked again, closed and opened, pinged, the toggles' names folded, all kept after a reload; what's selected (props, the buildings, the map) fitting the Inspector
node tools/assetcheck.mjs              # the asset browser: folders, a row dragged, a .glb dropped in, unstackable, saves; the Requests window's note
python tools/blender/test_run_claude.py # the Requests window's runs against a stand-in for Claude Code: done, failed (each way), cancelled; the lines checked first; made again with the user's comments; split into a request per drawing; the user's bones
cargo test -p engine-game -- --ignored shot   # the desktop-only windows drawn on the GPU, no window opened (build/shots/requests-bones.png; SHOT_PICTURE: the picture)
cargo test -p engine-game -- --ignored real_build   # the Builds panel's build for real: a debug desktop build into build/local/test, then one cancelled as it compiles (the folder as it was)
node tools/worldcheck.mjs              # the World view: a piece dragged into place, the world saved, a walk across a seam into a chunked map
cargo run -p engine-content --bin content -- chunk MAP    # a map stored as chunk files (unchunk: rows again)
cargo run -p engine-world --bin export-glb -- map test_meadow   # a map (or prop ID, props, building MAP ID) as .glb for Blender
node tools/shot.mjs "map:test_meadow"  # one screenshot of a state, from commands
```

The desktop build takes `--preset ID` (start that encounter), `--auto` (the AI plays your
side), `--time HH:MM` (or morning, day, night: a fixed time of day, for tests), `--levels 1-5` (every move at that level), `--maps [ID]` (the
map editor), `--requests` (the map editor with its Requests window open), `--mute` (no sound at all),
`--exit-after SECONDS`, `--stats` (frames per second, the worst frame
gap and the time each frame keeps the processor busy, printed every second) and `--no-vsync`
(draws as fast as it can, not at the screen's rate: with `--stats`, what the machine could
do). The release build has no console: when it stops on an error, the reason is added to
`build/crash.log` in the project, timed. The browser build takes the same as
`<body data-cmd="start:ID;auto:1">` (also `editor`, `maps`, `map:ID`, `battles` (the test
battles), `explore[:ID]`, `forget`, `back`, `scale:50-100` and `shadows:off|low|medium|high`) and
writes `data-state`, `data-round`, `data-fps`, `data-maxgap`, `data-cpu`, `data-cpupeak`,
`data-draws`, `data-tris`, `data-shadowtris`, `data-culled`, `data-pending` (the timing
read in progress), and in the map editor `data-map`, `data-mapsize`, `data-tool`, `data-level`,
`data-props`, `data-areas`, `data-dirty`, `data-buildings`, `data-build`, `data-draft`,
`data-selection`, `data-walker`, `data-zones`, `data-links`, `data-roamers`, `data-roaming`,
`data-pool`, `data-rooms` (a building's inside and outside while it's designed), `data-status`,
`data-assets`, `data-folder`, `data-shown`, `data-dropped`, `data-models`, `data-requests`, `data-chunked`,
`data-loading`, `data-squares` (the ground's squares held / in the map), `data-world`,
`data-pieces`, on the main menu `data-continue`, `data-options`, `data-confirmnew` and
`data-keptoptions`, in the editor `data-welcome`, `data-start` and `data-startfacing` (where new games start),
`data-startmark` (where it's marked on the map open), `data-startpick` and `data-worldstart`,
and the places of the buttons the checks press (`data-ui-*`) on `<body>`;
`view:x,z,distance,yaw,pitch` points the editor's camera and `point:x,y,z` reports where a
world point is on the screen (`data-point`): that's what the checks drive.

## Content

Two spaces with the same layout, `data/<collection>/<id>.json` (one record per file) and
`assets/` (the pictures and sounds the records name):

| Space | What | In git |
|---|---|---|
| `content/` | The game's own content. What the public game ships. | Yes |
| `temporary/` | Local placeholders, imported by a private tool that isn't part of the repository: creatures, moves, abilities, items, types, pictures and battle maps to develop with until the game has its own. | Never (`.gitignore`) |
| `private/` | The author's local tools and prototypes, and the map editor's model requests (`private/requests/`: pictures, often other artists' work). | Never (`.gitignore`) |

The collections: `types`, `creatures`, `moves`, `abilities`, `items`, `natures`,
`growth_rates`, `egg_groups`, `regions`, `encounters`, `parties`, and for the world
`settings`, `terrains`, `props`, `maps`, `spawns`; their fields are in `crates/content/src/model.rs`.
`content format` rewrites `content/`'s records in the layout the editor saves (short lists
on one line, one map row or placed prop per line), so every change reads as a small diff. Ids are lowercase slugs and records refer to each other by
id. A record in `content/` replaces the temporary one with the same id, which is how
placeholders get replaced one by one; the content browser (the editor's Menu: "Content")
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

A map is a grid. Each cell has a terrain and the level its ground stands at; a level is one
cell high: half a building's floor, which is the main character's height plus some room
(`settings/world.json`: 1.5 + 0.5 cells), so floors of buildings stack on every other step. Cells are stored as rows of characters
(`terrain` with a `legend`, `levels` as 0-9 then a-z, `areas` for tree areas), props as a
list (`prop`, `at` in cells, `level`, `turn`, `scale`, `offset`), the flooring out of doors as
`floors` (each cell's `tile` and `turn`, `flip` for one mirrored, `by_hand` for one put by
hand). A placed prop stays on
its cells once it's put down (`at` is their centre): it can stand anywhere within them
(`offset`), as far as its model leaves room; one that fills its cells, and every building,
stays exactly where it was put. A **ramp** (`ramps`: `at` its north-west cell, `size` along x
and z, `up` the way it rises, n, e, s or w, `rough` 0 to 1, `seed`) is a row of cells along
the top of a step in the ground, one cell deep and as long as you like, whose ground goes
down from their level to the ground past its foot, which must be a level down (one level
only: 2 down to 1, never 2 down to 0; the content check says so, and refuses one more than a
cell deep, on water, or over a building or another ramp). Its cells keep their level (the
upper one) and their terrain; the engine shapes their ground in place of the cliff, a little
uneven, each ramp its own way (bumps, its slope rounder in places and straighter in others
along it; none at its foot and top, which meet the ground flush), ragged rock showing down
its middle, and walls up its sides to the step's top either side; the cliffs and the ground
beside it meet its ground and its walls with nothing open between them (a higher cell's
outer corner over a ramp comes down square onto it rather than cut round).

A map is 2048 cells a side at most (a cell is about a metre: two kilometres); past 256 a side,
it's kept in **chunk files** (the editor switches to them, and the content check says so of a
record in rows). A big map is stored as **chunk files** instead: the record keeps its lists and says
`"chunk": 64` with a `terrains` list, and the cells live in `data/maps/<id>/<x>_<z>.json`,
one file per 64 × 64 square, as rows of numbers (the terrain's index in that list, the level
as a number, so neither has a limit) with the tree areas and zones the same way. A square with
no file is blank, so a large, mostly empty world costs little; an edit touches few files. The
editor reads either form and saves the one the map asks for ("Chunk files" in the inspector,
or in the New map box); `content chunk MAP` and `unchunk MAP` convert; `content check` reads
every chunk file. On the GPU a map is held square by square whatever its storage: the squares
within reach of the camera are built (a few per frame, nearest first) and the far ones dropped,
and an edit rebuilds only the squares it touched and works out again only what it can reach,
so even a 2000 × 1500 map takes any stroke or undo in under 26 ms a frame (Performance, big
maps).

- **Terrains** are painted per cell; the border between two is drawn by the ground shader
  (the four nearest cells vote with noise and priority), so edges come out organic and
  corners round without border tiles. Patterns (grass, meadow, tall grass, dirt, sand,
  cobbles, rock, water) are procedural for now. Raised cells get cliff faces of rock (layered
  slabs, cracks, lumps; in the terrain's side colour), every edge bevelled: the top edge
  goes down, the ground at the foot comes up into it (not water), and an outer corner
  standing on one level all round is cut off down its height.
- **Water** is a terrain's `water`: `inland` (lakes, ponds, rivers; what's drawn as water is
  inland unless it says otherwise) or `oceanic` (the Sea terrain). Inland water ripples the
  way the wind blows (barely when it's calm; from a cell or so out, clear of the shore's own
  waves) and lands small waves, their foam, on the shore the wind blows to; the sea's waves are rough by default and always roll in to the land,
  shorter and steeper as it shoals, breaking in foam near the shore, rougher when the wind
  blows to the land and calmer when it blows out to sea. Both are lighter in the shallows,
  and the sun glints on them. Where water meets a `beach` (sand is one) it washes up and back:
  up quickly, back slowly, each stretch of the beach in its own time, as far as the sea's
  waves or the wind pushing inland water onto that shore say, the sand it reached wet and
  darker (fading up the beach, no edge to it), foam along its edge. The renderer keeps a field of how far each cell is from the
  shore, which way the land is from there (smoothed), and which water a beach is near on its
  own level (`crates/render/src/shore.rs`), made again when terrain is painted; the water's
  edge runs along it, wavering.
- **The wind** is the world's (`settings/world.json`'s `wind`: the way it blows to, in
  degrees clockwise from north up the map, its strength from calm to a gale, how much of it
  comes in gusts; the editor's Post-processing window), and moves only what's painted for it:
  a model's "wind" colour layer (see Blender: R sway, G flutter, B phase), a
  stand-in's `wind` (its `sway` and `flutter`, rising with height: the asset inspector's
  Wind sliders; the trees, bushes and flowers that come with it have one), the tall grass.
  It leans downwind, sways about that and flutters, the taller the further, in gusts that go
  by in broad patches (a wood moves as one, a wave going through it), shadows included, on
  the GPU (`swayed` in the vertex shaders). The water ripples and rolls with it, and the
  clouds' shadows drift its way. How far each of these has gone is added up frame by frame,
  so a change of wind (its way, its strength) turns them smoothly, nothing jumping.
- **Abilities** (`moves`) hold what an ability does in battle, its level-ups, what it
  becomes and its timeline; **FX** records hold hand-drawn flipbooks on simple meshes
  (sheets under `assets/fx/`), made in the editor's FX editor.
- **Props** are the asset database: footprint in cells, height, whether they block, whether
  they are `unstackable` (nothing overlaps them and they overlap nothing: stairs, beds,
  shelves, wells...), the asset browser's `folders` they're in, a `light` they give off (a
  spot: colour, intensity, range, cone, where it sits and points from the prop's origin,
  at night only or always; lamp posts and wall lamps have one), and a stand-in model made of
  simple shapes (`parts`) until a real model (glTF, `model`) comes. Trees and flora can't be
  scaled; the rest can be scaled down. A prop (a character is one) or a creature can be
  `toon`: drawn with the toon look, which `settings/world.json`'s `toon` describes (the
  editor's Post-processing window). A prop of category `railing` is a piece of a railing
  style (`railing`: its `style`, a `span` along one cell edge, modelled from x -0.5 to 0.5,
  or a `post` on a point where spans meet or end; a span's two `ends` say what each needs of
  the point there, a post `takes` some of those words and stands `at` some kinds of point:
  `end`, `wall`, `line`, `corner`, `tee`, `cross`; a `weight` for how often). A railing is
  put together from its style's pieces by wave function collapse (`world::railings`): each
  span and point starts with every piece that fits it, the one with the fewest left is
  decided by weight from a seed (the same railing every time), and what no longer agrees
  with its neighbours goes. The styles that come with it: wood (balusters, now and then a
  panel between posts), iron (bars, now and then a ring) and stone (a balustrade with piers);
  a style is new as soon as one span says it, and the asset browser edits a piece's
  `railing` in its inspector. A prop of category `floor` (a cell's flooring, modelled a cell
  wide) may be a tile of a set of them (`floor`: its `set`, its four `edges`' words going
  round from the north, -z, then east, south and west, as modelled; a `weight`; `turns`: it
  may be laid a quarter, a half or three quarters turned): a set is laid by wave function
  collapse (`world::floors`), every two cells side by side agreeing on the edge between them
  (the same word on both), so a pattern runs on across cells (a long slab over two, a border
  round a medallion, planks running on). Past the cells laid only what may end there faces
  out: with an edge called `rim` in the set, rims (and they face nothing else: a border all
  round, round a hole too); else the words a tile has on all four edges (the joints between
  plain stones: never half a slab); else anything. The flagstones come with it (small
  stones, a slab, a long slab whose two halves meet along `long`). A prop of category `ramp` lies along ramps (stairs, a mud
  slide): its `ramp` says how (`along`: `stretch`, one stretched along the ramp, or `repeat`,
  copies side by side, one per its width; `rise`: how high its model rises, a level when it
  was made, 1 cell; `ends`: how much of either end keeps its size when stretched, a
  parapet, a post). Its model is made one cell deep, as wide as its footprint, rising along
  -z from its foot (z 0.5, on the ground) to its top (z -0.5); on a ramp it's turned up the
  ramp's way, made a level high, its foot on the ground at the ramp's foot and its top on the
  ground at its top, and laid from one end of the ramp to the other. One goes on a ramp; the
  ramp's ground under it goes straight from foot to top (a hair under that line), and it's
  walked on, never in the way. The samples: `stone_stairs` (stretched, its low walls kept)
  and `mud_slide` (a `wedge`: a box whose top slopes down from its back to its front).
- **Tree areas** are cells the player can't cross. The engine fills them with trees from
  the area's settings (which trees, spacing, size range, density, trees all along the edge,
  seed): the same trees every time, and painting a few cells changes only the trees near
  them (each spot tried has its own dice). Each tree keeps the room its crown takes: two
  trunks stand at least 0.6 of their crowns' radii together apart (0.45 along the edge, so it
  reads as a wall), and the area's spacing at least, so real-size trees make a canopy and
  bushes fill in between. The sample trees are real size (a cell is about a metre): a pine
  13 cells tall with a crown 4.6 across, a round tree 10 tall and 6.4 across; an area's size
  range scales them (0.85 to 1.2: 8 to 16 cells).
- **Buildings** have a footprint of cells (any shape, in one piece), floors two levels each
  (each with its flooring, never empty: a floor of a set's tile is laid with the set, afresh
  from a seed of the building's and the floor's, round what was laid by hand; any cell may
  have a flooring of its own, a set's tile with its `turn`, and above
  the ground floor a cell may be a hole, open down to the floor below, with a railing of a
  style along its edge or none: an inner balcony), walls along the outline in a style
  (plaster, planks, bricks, stone, logs; any segment of a floor's walls may have a style of
  its own, outside and inside apart: an inside face is as its outside unless it has its own
  or the building has one for all its insides, `inner_wall`, and its own is kept whatever its
  outside becomes; `tiles`, `holes`, `walls`, `inner` on a floor), doors and windows in them (an opening is as wide, in cells,
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
  pieces that overlap). Interiors are walked into in place (the floors above fade away, and
  the walls facing the camera go down to their foot).
- **Templates** (`templates/<id>.json`) are things on a map kept to be put down again
  anywhere as they are: props (on their cells and within them), buildings with what's
  inside and outside them, actors with their events, events with their scripts, from the
  template's north-west corner and its lowest ground. Select's "Capture an area" takes
  everything wholly inside the cells dragged; "Make a template" on a selected prop,
  building or actor takes that one. Drag one from Select's list onto the map (any map) and a
  copy goes down there: on the ground there, ids made unique on that map (Bo comes back as
  `bo_2`, his events too), the scripts' cells moved along and their actors renamed; it's
  refused off the map or onto another building. Ctrl+S saves the templates changed (kept
  in the browser); a template naming placeholders is kept in `temporary/`.
  Dragging a prop from the asset browser onto the map puts it down too.
- **Stairs** take the walker up: standing in front of a flight's bottom step and walking
  the way it rises, it climbs to the floor above (or out onto a flat roof), the building
  cut away above it; walking back down past the top step takes it down. Upper floors are
  walked within the building, around the stairwells and what blocks there.
- **Zones** are areas creatures roam (`zone_cells` rows, `zones`), and **links** send
  creatures out of a treeline (edge cells of a tree area) to a zone: they walk out of the
  trees, roam the zone for a while, then walk back in. Creatures move freely, not cell by
  cell, around whatever blocks, and stay inside their zone while roaming (`crates/world/src/spawn.rs`).
  Who lives in each zone, by time of day, is the map's **spawns** record
  (`spawns/<map>.json`), kept apart from the map because it names creatures: the editor
  saves it in `temporary/` while it names placeholders, in `content/` once it only names
  the game's own creatures (the check refuses a `content/` record naming a placeholder).
- **Actors** are the people, creatures and things on a map with a life of their own
  (`actors`): each looks like a creature, a picture or a prop, stands at a cell, and keeps
  still, wanders an area (a few steps, a rest, a look around) or walks a path round and
  round, a cell at a time on the ground the player walks, never through anyone. **Events**
  (`events`, and an actor's own) are what happens: spoken to (facing it, A), stepped on, or
  when the map starts; once only if asked, and only with a flag set or unset. Each has a
  **script**, one action a line, that reads like stage directions: `say Bo: Hello.`, `ask
  Yes -> yes | No -> no` with `label` and `goto`, `if`/`unless flag goto`, `set`/`add`/`clear
  flag`, `move bo 12,8 12,11` (the way found on the ground; `move player` too), `face bo
  player`, `wait 1`, `show`/`hide`, `sound sounds/chime.wav`, `camera 12,6` and `camera
  player`, `warp map x,z`, `battle encounter`, `end` (`crates/content/src/script.rs`; the
  content check reads every script and names what's wrong). A dialog box types the lines
  out (A: all of it, then on), choices pick with up and down; the walker and the actors'
  own ways wait while a script runs, so a script is a cinematic: people walking about, the
  camera gliding, lines said. Flags are what the story remembers (`world::events::Machine`
  runs scripts; the editor is their host, the game will be next). The test meadow has Bo
  (a talk with a choice and the camera going to the well), Pip wandering, a signpost to
  read and the well's cells to step into.
- **Sound**: a prop's `sound` loops where it stands (the stone well hums), a creature's
  loops as it roams and its `cry` plays when a battle starts, and a map's `ambience` is
  the sounds of the place (wind always, crickets by night), everywhere or within an area
  with a fade past its edge; scripts play one-shots. Everything is heard from the walker
  (or the view), fading with distance (`crates/game/src/audio.rs`: Web Audio in the
  browser, which plays nothing before the first click or key, rodio on the desktop). The
  sample sounds are made from scratch by `tools/make_sounds.py`.
- **Animation**: a prop whose model has a rig plays its "idle" animation (else its first)
  where it stands, skinned on the GPU with shadows (`crates/world/src/anim.rs` samples the
  pose, the renderer's skinned pipelines draw it; the flag pole's flag waves).
- **The overworld** (the main menu's New game or Continue, or `--explore [ID]`) is the game
  itself: the map walked full screen with its actors, events, sounds, lamps and animated
  props, no editing (the same code as the editor's Walk, without its panels:
  `MapEditor::play_ui`). Its camera stands back, 28 cells off and looking down at 58°, over
  real-size trees (the wheel takes it from 10 to 60), and what stands between it and the
  walker (trees, props) is seen through round them, dithered away more the nearer the middle of
  a cone from the camera to them, so they never go out of sight in a wood (the editor's Walk
  too). A new game starts where the world settings say (`start`: a map, a
  cell and a facing, one for the whole world). The editor's map panel (nothing selected),
  under New game, sets it: "Pick it on the map", then click the cell (or drag from it the
  way the player faces; Esc stops), or "Start new games here" (where the walker stands or
  the view looks); "Facing" turns it a quarter, "Go to it" opens its map on it. Set on any
  map it moves there; it's marked on its map (a ring, an arrow the way the player faces)
  and in the World view. The story is kept between runs, one slot: the map, where
  you stood and which way you faced, and the flags (the browser's storage;
  `build/save/story.json` on the desktop), saved as they change. Warps, seams and a script's
  battle work there (the battle plays, then it's back to the map).
- **The HUD** over the overworld (`play_hud.rs`): the map's name and the keys top left, and
  its buttons in layout groups (`layout.rs`: rows anchored to a corner of the screen that
  place their buttons themselves, one after another, so a new button is one more line in its
  group; each button frosted glass, the world behind it blurred, a white icon on it):
  **Team** bottom left (where the game's menus go), **Settings** (the Options) and **Exit**
  (back to the main menu, the story kept) top right. Esc no longer leaves: it (or a
  controller's Start) puts a ring on the buttons, the arrows or the d-pad move it from one to
  the next (across the groups too), A presses, B goes back; the world waits meanwhile.
  **Team** opens its list beside it: the five creatures that fought in the most of your last
  ten battles (the more recently, the sooner), your **main** first, pinned with a click on it
  (there's always one: the team's first in a new game; it's the one sent out first in a
  battle, the others after it in the team's order), then **View all**. The list closes with
  Team again, a click anywhere else (the other buttons too) or View all. The story keeps who
  fought in each battle and the main (`team.rs`).
  **View all** is a board of cards on frosted glass, every creature, five a row: the list's
  five first (the main, then who fought most), then the others; sorted another way (the Sort
  dropdown: most used, level, name, HP or number; Reversed, the other way round) or only some
  types shown (the Types dropdown: a checkbox each, as many at once as you like, All and None
  at its top over a line; the two fields in one box), the main stays first. Each card has the
  creature's picture (its pixels doubled), its name (its own, if a party gives it one: a
  member's `name`, 16 letters at most; battles call it so too) and level, its number (and its
  kind's name under a name of its own), its types, its status (or KO, or LOST), HP, MP and
  Exp in bars, and its kind's base stats. Out of battle every creature is whole for now (each
  battle starts it so, and none of that is kept after) and sits where its level's Exp begins
  (its growth rate's: Exp isn't earned yet). Hovered, a card shows its pin (grey; the main's
  is always there, white): pressed, that one is the main. A card clicked opens its details:
  the cards turn into thumbnails (the picture, the level, the main's pin), three a row down
  the left, one clicked showing its own, and the creature fills the right: its picture (three
  times its pixels; its model, idle, once creatures have models), what its card has, its
  lifeforce (a bar, with its number), a button to pin it, and its moves, a tab each, the one open with its type, kind,
  power, accuracy, MP, what it does and its five levels (those reached lit, the one it's at
  marked, the XP for the next). With the game's buttons the arrows or the d-pad go over the
  cards (or the thumbnails and the details' buttons) and the controls above them (and an open
  dropdown's choices), A presses, L and R go through the sorts, B closes a dropdown, then the
  details, then the board; the info icon by Close (the cross, top right) says so.
  **Lifeforce** is a gauge, 0 to the world's top (`settings/world.json`'s `lifeforce`, 100
  unless it says otherwise): a creature knocked out in a story's battle loses its level's
  worth (a level 15 one, 15), up to the world's cap (`lifeforce_ko_cap`, 20: from level 20
  on, 20 a knock-out); with none left it's lost for good (greyed, LOST): it isn't sent into battle and can't be the main, and only a very
  rare consumable (to come) could bring it back. Ways to get some back (resting at an inn)
  are to come too. With none of the team left to fight, a battle doesn't happen. The story
  keeps each one's lifeforce with the team, a plain number.

The editor (the main menu's Editor, or `--maps`) opens on the map it last had, else the one
games start on. Its top bar's **Menu** goes back to the main menu, to the **Content** (every
record, both spaces) and to the **Test battles** (every encounter by region, to try it, with
the party at its level or the AI on your side; a battle goes back to them), and shows the
welcome again ("About the editor"). The tools and the level (PageUp/PageDown) are
at the top of the **Toolbox** (on the left), the tool's settings under them; the **Inspector**
(on the right) shows what's selected (a prop, a building), or the map itself (name, size,
what's on it), its rows as wide as it is; the bar at the bottom shows the cell under the
pointer and the last message. Every panel but the top bar and that bottom bar docks (on the
left, on the right or along the bottom) or floats in a window of its own, and goes from one to
the other at any time: its header's float button, a window's dock button (it docks on the side
it's nearest), drag a docked one's edge to size it. The Toolbox, the Inspector and the Assets
come docked, the rest closed (the Requests, the Post-processing, the Abilities and the Builds in windows).
Each has its toggle at the top bar's right end, its icon and name (the chevron before them folds
the names away, leaving the icons); right-clicking one pings its panel, opening it if it's
closed: shaken a moment, rings round it (coming in from a docked one's edges, going out round a
window). The layout is kept as it was left, after a restart or a reload (the browser's storage,
or `build/save/` on the desktop).
**Ground** paints terrain and, with "set the level", raises or lowers cells to it: the cells
under the pointer as they stand, whatever the level picked, outlined where they'll go (a
stroke goes by the ground as it was when it began, not as it has raised or sunk it);
right-click picks a cell's terrain and level. On its **Ramps** tab, a drag along the top of
a step (the cells a level up, where the ground steps down one level) makes them a ramp down to
the ground below, one cell deep (a click, one of a cell; at a corner it goes down the side the
camera is on; a drag that runs on past the step's end is cut back to it), and says why not when
it can't be (two levels down, two rows deep, nothing a level down, water, a building); a click
picks one (how uneven it is, "Another look", Remove or Delete), and painting another level over
one of its cells takes it away (with what lay along it). On foot a ramp is walked up and down
and along, never off its sides; props, trees and creatures stand on its slope. Its **Floors**
tab lays flooring on the ground out of doors (a pavement, a plaza, old ruins), the brush's
cells on their own level (never on a ramp's slope), clearing their trees: a tile, or a set of
tiles laid to fit (each cell the tile that fits the cells round it, the stroke laid again all
together as it ends, "Lay it again" another way), or "None" to take it away. A tile put by
hand ("Mirrored" mirrors it, its east and west swapped, before it's turned) can be put again
as a **symmetry** takes it round a centre: mirrored left-right or top-bottom, both mirrors,
a half turn, quarter turns, or all eight (both mirrors, both diagonals and the quarter turns:
a medallion, a rose). Picking one with no centre yet has the next click pick it ("Pick it"
does again): the nearest middle of a cell, middle of an edge or corner, shown with its lines;
quarter turns need a cell's middle or a corner. Then each tile put (or taken away with "None")
is put again where the symmetry takes it, mirrored and turned to match, where it lands on the
map (shown under the pointer), one click one undo step; a set laid to fit isn't mirrored.
**Place** puts a prop at the level (Q/E turn
it, Shift puts it where the pointer is within its cells); what lies along ramps (category
`ramp`: stairs, a mud slide) goes on the ramp under the pointer instead, or the one next to
the cell it's on (its foot, past its top), turned up its way and laid from its foot to its top
and end to end, the slope it'll cover marked as you point (crimson where it can't go); it says
why not off a ramp, on one that has its own already, or where something that doesn't stack is
in the way. Select drags it onto another ramp; it isn't turned or scaled. **Select** moves a prop or a
building to other cells (drag it, where there's room: its position in the inspector only
shows), turns, scales and deletes props, and a prop's Tweak (in the inspector) moves it
within its cells instead (drag it, or the inspector's x and z),
and its inspector edits the prop's record too: name, category, cells and height, blocking,
its light (the same window as the asset browser's), its sound and the asset-browser folder
it's in ("Move to a folder"), with "Save prop" (or Ctrl+S) writing the record. "Edit in
Blender" opens Blender on it, the add-on set up and the prop imported; what you change
there is written back to the project by itself (a moment after each change, and on
Ctrl+S: the .blend lives in `build/blender/`), into the space it came from (a placeholder
stays in `temporary/`), its record's facts and folders kept, and the editor shows it within
a second. In the browser the button opens a `battlefrontier://edit/<prop>` link, which the
desktop game answers (its release build sets the link up in the user's registry each time
it starts, a debug build only with `game --register-links`; `reg delete
HKCU\Software\Classes\battlefrontier /f` takes it away): Blender opens the same way, on
the project the desktop game was started in;
**Trees** paints tree areas and edits their settings; **Build** draws a building's
footprint, then designs it floor by floor: the floors above are cut away and the rest of the
map is darkened, and nothing lands outside the building. *Structure* is flooring, walls and
roof (a flat roof for a rectangle, if you want one), doors and windows (drag along an outside
wall, then pick an asset that wide), pillars, and cell by cell: *Flooring* (click or drag
over the floor's cells: each takes the flooring picked, or its floor's own again; a set of
floor tiles is one of them, "Flagstones: a set of 4, laid to fit": each cell painted takes the
set's tile that fits the cells round it, the stroke laid again all together as it ends, and
"Lay it again" lays the floor's cells of the set another way. Picked, a set shows its tiles
too, each to put by hand ("· by hand": just where it's clicked, as it's turned, "Turned 90°"
turning it a quarter more each click), for a mosaic or a symmetric design of one's own: a tile
put by hand stays as it was put (`by_hand`), and the set's laying round it fits it. Any tile
of no set can be turned so too, and mirrored ("Mirrored"), and put again with a symmetry as
the Floors tab does (its centre the floor's middle unless one is picked: "The middle" goes back
to it), a hole and the floor's own too; above the
ground floor, "Empty: a hole" opens a cell down to the floor below, with the railing picked
under it (a style, or none; a hole painted again takes the one picked), never where
something stands; the walker can't step into one, nothing is placed over one) and *Walls*
(click or drag along the floor's walls: each segment's outside, or with "Inside" its inside,
takes the style picked, on this floor or on every floor at once; Style's "Inside" gives all
the inside faces one style, "As the outside" by default);
*Inside* places what the `Buildings/Inside` folder holds on the floor being designed (on its
inner walls for things that hang); *Outside* asks for a wall first (a court's walls count),
then hangs what `Buildings/Outside` holds on it or stands it on the ground up to two cells in
front of it. Click a placed thing to select it: Q/E turn it, Delete (or its "remove" button)
removes it; unstackable props never overlap anything. Then "Confirm the building". A
building on the map is selected with Select: Edit (the same modes), turn (Q/E), move (drag
it onto free cells), delete. **Walk** puts a stand-in character on the map: the
arrows move it cell by cell, and A speaks to what's in front (an actor, a sign: its events)
while cells stepped on set off theirs. Walking into a building, the floors above and the
roof fade away over half a second (a blended pass drawn after everything else, each pixel
once), and so do the walls facing the camera on that floor, down to their foot (a stub
showing where the room ends and where its doors are; their doors, windows and what hangs on
them go with them), so they hide none of the room; the rest of the map darkens and the view
comes a little closer; walking out brings them back the same way. Turning the camera inside,
the walls it faces then go down and the others rise back, fading (a wall goes once it faces
the camera well, rises once it hardly does: none flickers as the camera turns past).
**Events** (7) is the actors and events: add an actor (click a cell) and give it a look, its
ways (still, an area to wander, a path of cells) and events; add an event (drag cells) and
write its script in the inspector, which names what's wrong as you type and runs it on the
spot while walking; the story's flags are listed there, to forget. "Sounds" and "Animate"
in the top bar turn the sounds and the rigged props' animations off and on. **Zones**
paints zones (and who lives there: morning, day, night) or, in "Link treelines", takes a
drag from a forest's edge onto a zone. "Creatures" lets them roam the map; "Debug" shows
every zone, tree area, link arrow and creature path. Right-drag pans, middle-drag turns
the view, the wheel zooms; the top bar's time slider (0:00 to 23:59) previews the light at
any time of day (the game itself follows the real clock), and the lamps light up from dusk
(Lamps lights them whatever the time); Ctrl+Z/Ctrl+Y undo and redo.
**Post-processing** sets the toon look, for what's ticked toon (a prop's "toon" in the asset
browser or its panel, a creature's under the browser's Creatures): the sun's and the lamps'
light in steps (how many, how soft their edges), the shade turning a colour, a rim of light
round the edge and a glint; ink lines round it where it stands in front of what's behind it
(drawn outside it: colour, width, opacity, how far behind counts) and along folds sharper
than an angle; colour steps for the whole picture, toon or not (the world's light in
bands, the colours kept; the interface untouched); and the clouds' shadows drifting over
the world (how dark, how big, how fast, how much of the sky; cast along the sun's way from
the clouds' own height, so they run on unbroken up a cliff, faint by moonlight; slowly
changing shape as they go, layers of noise each drifting its own way); and the wind (the way it blows, a compass showing it, its strength and its
gusts). Changes show at once; Save (or Ctrl+S)
writes them into the world settings record, Revert drops them, Defaults starts over.
**Abilities** opens the Abilities window: every ability (a move record) of both spaces, one
picked to edit or a new one (new ones go to `temporary/` unless told otherwise): what it
does in battle (type, category, power, accuracy, PP, priority, target, the engine's effect
and its chance, contact, flags, the description), its level-ups (what it gains at move
levels 2 to 5, on top of the world's own growth), what it becomes (stronger abilities made
separately and named here, at a move level, with an item, or both), and "Edit the
timeline". The **Timeline** panel lays the ability's clips on tracks in time (the user's and
the target's animations, motions, FX, sounds, the player's inputs, the camera): add one at
the playhead, drag it along its track, set what it is and for how long; Play runs it on the
map with a stand-in user and target (any prop with a model, a rigged one playing its
animations, or a creature's picture; "Stand here" puts them where the view looks): motions
move them, FX frames play round them, sounds sound, input cues shrink onto their mark (Z is
judged against the window), the camera shakes or comes closer. An fx clip's "Edit the FX"
holds the timeline still and opens the **FX editor**: the FX's meshes (a billboard, a flat
quad, a sphere, a dome, a ring) placed round the actor, dragged on the map or set by hand,
each with a flipbook of hand-drawn frames; the **Drawing** window paints the frame shown
(brushes, colours, an eraser, undo, the frame before copied), the frames before and after
ghosted, and plays them; "Export the flipbooks" packs every mesh's frames at a smaller
size (what the game draws; the drawn frames stay as the source). Abilities and FX save with
Ctrl+S (the browser keeps them). In battle an ability with a timeline plays it instead of
the engine's own animation (`world::timeline` is the maths both read): the stage runs its
motions, FX, sounds and camera, and its first input clip is the timing read (a tap, cast or
block at the beat with the clip's window, a charge over the clip, a rhythm of its beats); a
foe's ability asks the player to block at that beat; the ability lands at the clip's beat
(or the first flash on the target, or the end). **World** opens the World view across the window: the world this map is part of as
pieces on a grid (the map being edited lit), seams green where the levels match and crimson
where they don't; drag a map in from the list, drag pieces into place (they snap to cells),
double-click a piece to edit that map, "New world" starts one with this map, "Save world"
writes `content/data/worlds/<id>.json` (kept in the browser). A map in a world is
never alone: the other pieces within the camera's reach are drawn where they lie, streamed
square by square like the open map, a little darker in the editor (the status bar names the
one under the pointer; a double-click edits it, the view staying put), their lamps lighting
across. At a seam on one level there's no cliff, and terrain borders and patterns run on
from one map into the next. Walk crosses into the next map at a matching seam without a
pause: it becomes the open map, the one before a map around it, nothing reloaded and
nothing moving on the screen (the map must be saved first). Save writes `content/data/maps/<id>.json` on the desktop; the browser keeps it
(below). Maps made here are the game's own content.

**Auto-save**, next to Save, saves what's changed a moment (1.5 s) after the last click, key
or drag, with nothing held and nothing being typed into (a name half typed isn't a new
file): the map and its creatures, the world, the props, the world settings, the abilities,
the FX and the templates, new ones too, as each has its place in the project. It's on in the
browser, and on the desktop once ticked there (it writes into the project's folders; the
choice is kept). **In the browser** nothing asks where to save: each file is kept in the
browser's own storage (IndexedDB, `crates/game/src/keep.rs`) at its place in the project
(`content/data/maps/<id>.json`, a map's squares, a model, an FX sheet; a template deleted is
kept as removed), laid over the site's content whenever the page opens (the game plays it
too), and dropped once the site has the same record. Publish sends it; the Menu's **Saved in
this browser** lists it, downloads it as one zip (each file at its place) or forgets it all
(the page then opens again on the site's own). With `?saves=downloads` in the address, or in
a browser that keeps nothing, saving downloads each file instead, as the checks that read
downloads ask (`data-keeping`, `data-kept`, `data-keptwindow` and `data-autosave` for the
checks; the `kept:<path>` page command puts a kept file's text in `data-keptfile`).

**Assets** (in the top bar, or drop a file on the window) is the asset browser: every prop
of both spaces as a detailed list, with its GAME or TEMPORARY tag, category, size, model or
stand-in, what it does (blocks, unstackable, on walls) and folders, shown by folder
(`Buildings/Inside`, `Buildings/Outside`, `Nature`, `Town`...: a prop's `folders`, or its
category's). Drag a row onto a folder to put the prop there too (a chip's × takes it out);
click a row to use it (the Place tool, or a building's Inside or Outside list when it's in
that folder); "light" on a row opens the prop's light (colour, intensity, range, cone, where
it sits and points) or gives it one; drop `.glb` files on a folder and each becomes a prop there, its footprint and
height measured from the model: written at once on the desktop (`content/`, or `temporary/`
with the box ticked), kept until saved in the browser (its record and model with it). On the
desktop, `data/props` and `assets/models` of both spaces are watched: a record or model
changed there (the Blender add-on's Add to project, a re-export) shows within a second.

**Publish** (in the top bar) sends what's been made to the game's repository as a pull
request, for its maintainers to look at: only those who can write to the repository merge one,
so nothing gets into the game unreviewed. **Sign in** comes first (Publish waits for it), with
a GitHub account: a code typed on github.com/login/device, or a token pasted (classic, with the
`public_repo` scope; "Make one on GitHub" opens GitHub's page for one, the scope ticked). The
desktop starts with the code, the browser with the token (below). The sign-in is kept, until **Sign out**, in the user's own settings folder
on the desktop (`%APPDATA%\BattleFrontier\github.json`; `~/.config/BattleFrontier/` elsewhere),
never in the project; in the browser in the site's storage. Publish lists every file of
`content/` changed, in groups (maps and their events, props, creatures, models...), each
ticked to send or not. On the desktop: the project's folder against the commit its checkout is
on (read from `.git`; else main, and then removed files are left unticked, as they may be new
there). In the browser: what was saved there (each record as the editor holds it, models
dropped on the page, a chunked map's squares) against the site's own (its bundle, its
squares), on the commit the site was built from (Cloudflare's `CF_PAGES_COMMIT_SHA`, built
in; else main); a record gone is left unticked. Then a title, a description, and a tick that
it's your own work or yours to send (the Developer Certificate of Origin: the commit is signed
off as you, as the checks ask of every commit). Sent: on the desktop the names check first
(`tools/check-names.py`, when Python is there), then your fork (made when you can't write to
the repository, and brought up to date), the files, a tree on the copy's, a commit on its
commit, a branch `publish/<title>-<time>`, and the pull request against main, its link shown
(`data-signedin`, `data-signing`, `data-signincode`, `data-signinerror`, `data-publish`, `data-publishfiles` and
`data-published` for the checks; `github:BASE` points it at a stand-in). GitHub's calls are the
same on both: on a thread of their own on the desktop, the page's fetch in the browser. The
API answers other sites, but github.com's sign-in doesn't, so in the browser its two calls go
through the site's own address: a Cloudflare Pages Function (`functions/github/login/`, only
those two, nothing kept; `_routes.json`, written by `tools/cloudflare-build.sh`, keeps every
other address plain files). github.com turns those calls away at times (429: it limits the
shared addresses Cloudflare's Functions go out from), so the Function asks for a code up to
three times; the editor asks whether it's been typed less and less often while turned away
(the waits doubling) and gives up after four refusals in a row, saying what to do instead; and
the browser's sign-in starts with a token, which goes from the page straight to GitHub's API.
Signing in by code goes through the repository's own OAuth App,
"Battle Frontier editor" (its owner's: on GitHub, Settings, Developer settings, OAuth Apps; its
"Enable Device Flow" ticked; no client secret, as the device flow needs none), its Client ID
(public) in `CLIENT_ID` (`crates/game/src/mapedit/publish.rs`); `GITHUB_CLIENT_ID` where the
editor runs or is built overrides it (a fork's own app, say). A token pasted signs in too. For merging to stay the maintainers', keep the repository's collaborators few
(a branch rule on main asking for a review stops anyone merging their own unreviewed).

**Requests** (in the top bar; the desktop only, the browser's window says why) is where the
pictures of a creature, a prop or a floor go for Claude to model in Blender: drop PNG, JPG or WebP files
on the window while it's open (models dropped meanwhile still go to the asset browser, and
pictures go to it only while it's open). The first drop starts a request named after the
first picture; the next ones add to the selected request ("New request" starts another). A
request has a name (its id follows it while it waits), a kind (creature, prop, or floor set:
a set of floor tiles, each a cell, laid to fit), its height in cells (not a floor set's), a style (smooth, low-poly, or accurate: as close to the pictures as it can be, whatever
it takes, with its model details), a creature's default pose (T-pose or as-is), how many shapes
to build it from (a range on a meter, or unlimited: as many as the design needs, up to about
100), notes, and "This design is mine or licensed" (off by default: the model of someone else's
design goes to `temporary/`, never `content/`). A creature's **Draw the bones…** opens one of its
pictures large to draw its bones over it, a colour per part (leg, arm, torso, head, ear, horn,
tail, wing, and misc for a piece you name yourself): a click adds each joint, chained from the
root out, and a right-click ends the line. Claude works out which part of the creature each
line is on when it draws its own lines, and rigs the creature on them. It's kept
in `private/requests/<id>/` (`request.json` and the pictures, a WebP as PNG for Blender), never
committed. The list shows each request's status (waiting, in progress, failed, done) and its
pictures, with a **Start** button on each (**Cancel** while it's being made; one at a time):
Start has Claude Code model it by itself, unattended (`tools/blender/run_claude.py`, with only
the tools the recipe needs; **Claude:** at the top picks the model). One in progress has a bar
with the time left (the top bar's button shows it too); it ends done by itself once the model
is in the project, or failed (why, under it) when Claude ends without it, stops with an error,
isn't logged in, or goes 30 minutes without a word. The **console** at the window's foot shows
the request's log (`log.txt`, every line timed, the times greyed): Claude reached and the model it runs, what was
sent, each step and tool call, the end; Copy puts it all on the clipboard. A done request shows
the renders of its model and the prop it became ("Use it" puts it in the Place tool), and
**Make it again**: a box for what to look for and what to change, and **Make it again with
these comments**, a run that starts from the model the last one made (its script, its lines,
what it said) and changes what you say, keeping the rest; the comments are kept with the
request, and the runs after it are told them too. A tree's cells are its trunk's foot (what
stands below a character's height), not its crown's. A picture with several drawings of one
kind of asset (trees, bushes, rocks, plants) makes as many: tick "Several of them: one asset
per drawing" and Start has each drawing made a request of its own (its crop, its name and
height, the request's settings); **Make them all** then has them made one after another. A
floor set's pictures (a pavement, old ruins, marble, a wood tiling) become a set of floor
tiles: Claude breaks the pattern into cells, each tile's edges saying what fits beside it,
lays the set out by the game's rules to compare it with the pictures, and adds each tile to
the set; its panel lists the tiles and their edges, and "Use it" opens the Ground tool's Floors
tab with the set (in a building, it's in the Build tool's Flooring). The
window resizes every way. Claude Code's command line must be logged in once: a request that failed for that has a "Log in to
Claude Code…" button, which opens it in a terminal (type `/login`). The recipe is
`tools/blender/requests.md`: a script per request builds the model from shapes in Blender,
renders it beside the pictures until it matches (reshaping it with any of Blender's tools:
edits by hand, modifiers, scripted sculpting), rigs a creature with an idle animation, and
adds it to the project; a Claude session asked to "make the waiting requests" follows it too.

**Builds** (in the top bar; the desktop only, the browser's panel says why) builds the game
into a folder without the terminal, as the commands above build it. **Platform**: Desktop
(`cargo build -p engine-game`: the program with `content/` beside it, so double-clicking it
plays; `temporary/` too when "Placeholders" is ticked, for your own play only, never to share)
or Web (`trunk build`: the site's files, for any web server; its **Address** is Trunk's
`--public-url`). **Build**: Release or Debug. The **Folder** (typed, Windows' picker with
"Choose…", or `build/local/desktop`): a whole path, never the project nor a folder holding it,
inside the project only under `build/local/`. Optional: how many compile at once, and the
**compile cache** (`build/target-builds`: a folder of its own, since the running editor's own
program is locked and a build beside it couldn't replace it; the first build compiles
everything, the next ones only what changed). **Start build** compiles in the project, then
copies into the folder, with a bar (cargo's own count, then the files) and the programs'
lines. **Cancel** stops it at any point: every program of it (the compilers cargo started
too), what it made in the folder taken away and what it replaced there put back, the folder
as it was (leaving or closing the editor cancels one going on too). A build leaves
`battle-frontier-build.json` in its folder, listing what it made: the next build into it
replaces those, and only those (a name the folder has of its own is refused, never written
over). The settings are kept (`build/save/editor_build.json`).

## Blender

Props, buildings and maps go to Blender, and models made in Blender come into the game, as
glTF (`.glb`). A prop with a `model` (a `.glb` under its space's `assets/models/`) is drawn
with it, textures included; one without keeps its stand-in made of shapes. The map editor
exports the map ("The map to Blender", in its panel), the selected prop (Place), every prop in one file laid out
by category (the asset browser's "All props to Blender") and a building (its inspector) to
`build/exports/` (a download in the browser); so does `export-glb`. The Blender add-on in
`tools/blender/` finds the project on the computer, imports its props (models, or stand-ins
rebuilt from their shapes, lamps with their lights) one at a time or all at once, exports
and imports maps, and adds a selection to the project as a prop: the `.glb` and its
record, measured and laid out as the project's own, its light read from a spot or point
light object on it and its sound from a speaker (see `tools/blender/README.md`); one made to
lie along ramps (category "Along ramps": one cell deep, rising along +Y from its foot at Y
-0.5 to its top at Y 0.5) stays where it was made and keeps its footprint. The file
keeps materials, rigs and animations as Blender writes them: the engine draws base colours
and textures, and a rigged model plays its "idle" animation (`crates/world/src/anim.rs`
poses it, the renderer skins it, shadows included); a camera never goes into a model, the
game has its own. What the wind moves is painted for it in Vertex Paint, on a colour layer
named "wind": its R how far a point sways (0 at a stem's or a trunk's foot, where hair grows;
1 at the tips), its G how much it flutters (leaves, petals, fur, a cloth's edge), its B its
phase (different values on different leaves or tufts, so they don't move together); the
game adds how high the point stands, so taller things bend further. The add-on's "Wind
paint from height" starts one (the sway rising from the foot, a phase for each loose part);
Add to the project writes it as the model's second colour layer and marks it (`bf_wind`),
which the engine reads (`export-glb check` says "painted for the wind") and the import gives
back as "wind". Unpainted, a model stays still. Same frame everywhere: one unit is a cell (a metre), the asset on the ground at
the origin, its front to Blender's front view. Models made from the editor's requests come
through the same add-on (`tools/blender/requests.md`, `request_kit.py`).

## Layout

| Crate | What |
|---|---|
| `crates/content` (`content`) | The content model (`model.rs`: records, maps as rows or as chunk files, worlds and their seams; buildings and their checks in `building.rs`; ramps and their rules in `ramp.rs`), loading spaces from disk or bundles, the game's own over temporary records, validation of every reference, bundles. The `content` tool: `check`, `bundle`. |
| `crates/core` (`rules`) | The battle rules, no rendering, no platform code: `battle.rs` (rounds by Speed, actions, damage, locks, battle sizes), `moves.rs` (upgrade tracks, Focus/Chain/Storm, MP cost, level texts), `mon.rs`, `ai.rs`, `config.rs` (every tuning number), `dex.rs` (the content made fast to look up: indexes, type chart and roles, ability effects). A port of the prototype's `src/engine/*.js`; `tests/battle.rs` is its `tests/battle.test.js`. |
| `crates/world` (`world`) | Maps as the engine works on them: the record decoded into a grid and back (`lib.rs`), procedural tree areas (`trees.rs`: each tree keeping its crown's room, each spot its own dice, placed again after an edit only where it can move them: `Woods`), which cells block, where walls are and who can step where (up and down ramps too), ramps' ground (`ramps.rs`: the slope down a level, the unevenness, the rock), buildings' geometry (`buildings.rs`; railings put together from their style's pieces, `railings.rs`; floors laid with a set's tiles to fit, `floors.rs`; cut away above a floor, the walls facing the camera there down to a stub), roaming creatures (`spawn.rs`), glTF models in and out (`gltf_in.rs`, `gltf_out.rs`, the `export-glb` tool), the geometry (`mesh.rs`: cell tops, bevelled where the ground steps down or up, lumpy cliff faces with their outer corners cut, tall grass, props' stand-in models from their parts; every vertex's wind: sway, flutter, phase, height above its foot), the FX meshes and sheets (`fx.rs`), and the timeline maths (`timeline.rs`: poses, frames, beats, the camera, the impact). |
| `crates/render` (`render`) | The renderer on wgpu 30: battle maps (`map.rs`: ground, boxes, cutouts, the battle spot), world maps (cell tops and cliff faces through the ground shader, which draws terrain borders from a texture of the cells and rock on the faces and bevels; vertex-coloured props and trees), billboards, discs, rings, rectangles, glows, rain, the sun's shadow map, day/night light and fog, the toon look (banded light for what's flagged toon; ink lines found in the blit from a second target the solid things write their looks to, each pixel's toon, distance and facing; colour steps over everything; the clouds' shadows, each pixel put back in the world from its distance, cast from the clouds' height and changing shape), the wind (what's painted for it swaying in the vertex shaders, shadows too; its gusts', swaying's and the water's motions added up frame by frame), the water (inland ripples and the sea's waves, foam, the swash up a beach, from the shore's field: `shore.rs`, worked out again round a change only), trees and props seen through round the walker (a dithered cone from the eye), the post-processing (`Retro`: the pixelation, the scene drawn small, its pixels whole squares of the screen's, as dense as asked and bigger or smaller with the camera's zoom, in 15-bit colour; an old handheld's LCD, its colours and the gaps between its pixels) and a small blurred copy of the picture for frosted glass (`frost_view`, which the game hands egui) (`scene.rs`, `shader.wgsl`). Draws into its own target (what's solid, then what's see-through), then blits into the host's pass. Uniform buffers only, so WebGL 2 works as well as WebGPU. |
| `crates/game` | The program (eframe 0.36, as GBMapping): `app.rs` (menu, loading, battle, results, frame stats, test hooks), `editor.rs` (the content browser), `mapedit.rs` (the map editor; `mapedit/build.rs` the Build tool, `mapedit/rooms.rs` inside and outside a building, `mapedit/assets.rs` the asset browser, dropped files and the disk watch, `mapedit/requests.rs` the model requests' window, its Bones window and their files, `mapedit/builds.rs` the Builds panel (the game built into a folder by cargo or Trunk, cancelled and taken back), `mapedit/welcome.rs` the editor's Menu, its welcome and where new games start, `mapedit/stream.rs` the maps on the GPU square by square, `mapedit/around.rs` the maps around the open one and stepping into them, `mapedit/dock.rs` the panels docked or floating, their toggles and the layout kept, `mapedit/post.rs` the Post-processing window, `mapedit/ramps.rs` the Ground tool's ramps, `mapedit/abilities.rs` the Abilities window, `mapedit/timeline.rs` the Timeline panel, the preview, the FX editor and the Drawing window, `mapedit/worlds.rs` the World view, `mapedit/walk.rs` walking (across seams too), `mapedit/zones.rs` zones, links and roaming creatures, `mapedit/actors.rs` the actors alive, `mapedit/events.rs` the story, the dialog box and the Events tool, `mapedit/sounds.rs` what's heard, `mapedit/templates.rs` templates captured and dropped, `mapedit/models.rs` props' models, their rigs posed and exports to Blender), `audio.rs` (the mixer: Web Audio and rodio), `director.rs` (turn flow, event playback, the animation + timing state machine), `stage.rs` (battlers, motions, effects, camera; the light through the day), `hud.rs` and `menu.rs` (egui), `input.rs` (A/B/d-pad/L/R and the player's keys and controller buttons for them; tap, hold-release and rhythm reads), `pads.rs` (controllers through gilrs, and which device is listened to), `layout.rs` (the HUD's layout groups, frosted buttons, the ring from one to the next), `play_hud.rs` (the overworld's HUD: the Team, its list, its board of cards and a creature's details, Settings, Exit), `team.rs` (who of the team fought in the last battles, the main, the order the team goes into a battle, the board's order, lifeforce), `presets.rs` (battles from encounters), `platform.rs` (clocks: the steady one and the local time of day; files, hooks), `title.rs` (the main menu over the world, its tweens, the fonts, the Options in tabs and their categories), `shot.rs` (the tests' pictures of windows, drawn on the GPU). |
| `tools/` | `serve.mjs`, `check.mjs`, `frames.mjs`, `perf.mjs`, `encounters.mjs` (which encounters the checks play, by kind), `mapcheck.mjs`, `buildcheck.mjs`, `tilecheck.mjs` (the Build tool cell by cell: flooring, walls, holes), `floorcheck.mjs` (floors laid with a set of tiles, in a house and out of doors, and tiles put by hand, mirrored and with a symmetry), `rampcheck.mjs` (ramps made, refused, walked, saved; stairs and a slide laid along them), `levelcheck.mjs` (the Ground brush painting levels on the cell under the pointer), `zonecheck.mjs`, `assetcheck.mjs`, `worldcheck.mjs`, `eventcheck.mjs`, `explorecheck.mjs` (the overworld: the story, the cottage walked into, its walls facing the camera down, another as the camera turns), `templatecheck.mjs`, `postcheck.mjs` (the toon look), `abilitycheck.mjs` (an ability, its timeline, an FX drawn and exported), `timelinebattle.mjs` (a battle playing an ability's timeline), `pwacheck.mjs` (the site installs as an app: `url=` for the deployed one), `menucheck.mjs` (the main menu, the Options, the Editor's welcome), `layoutcheck.mjs` (the editor's panels docked, floating, pinged and kept), `padcheck.mjs` (controllers, through stand-ins for the Gamepad API), `hudcheck.mjs` (the overworld's HUD, the Team's list and board of cards, the pixelation and the LCD), `watercheck.mjs` (the wind and the water: foam on the downwind shore, the sea rougher onshore, what sways, a two-sided leaf fluttering as one, the clouds' shadows going with the wind, the Wind section), `blender/` (the Blender add-on, its test, `make_flag.py` for the rigged sample), `make_sounds.py` (the sample sounds), `make_icons.py` (the app's icons), `shot.mjs`, `cdp.mjs` (headless Chrome). |

## Frame pacing

Nothing in a battle steps per frame: the rules are event-driven, animations and timing
windows run on a millisecond clock (`performance.now()` in the browser), so the battle
looks and plays the same at 60 or 120 Hz. Timing presses are stamped with the time of the
frame that sees them (at most one frame late, against windows of ±110 ms or more). The
renderer reuses its buffers (the sprite buffer only grows), so a frame allocates very little.
The overworld walks cell by cell at a fixed pace, whatever the screen's rate. Every pipeline draws
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
shadow, what was left out of sight, the graphics settings). In the Options, **Graphics** sets
the 3D scene's resolution (100, 75 or 50% of the screen's, smoothed up) and the shadows (off,
low, medium, high: a 1024 to 4096 shadow map) for machines that can't keep up (the
pixelation sets the resolution itself: the scene drawn that small costs less still).

What keeps it fast:

- World meshes (ground, cliffs, props, trees, buildings) are built square by square (64 cells,
  only the squares within reach of the camera, a few per frame; the far ones are dropped) and
  cut into 16 x 16 cell pieces with their bounds; a piece out of the camera's sight isn't drawn,
  nor one out of the sun's in the shadow pass (a square's box out of sight: all its pieces left
  out at once). A big map costs what's in reach, not its size, and an edit rebuilds the squares
  it touched: on a 320 x 240 map a prop placed costs 6 ms.
- An edit works out again only what it can reach, however big the map: the trees round it
  (`trees::Woods`), the ground's squares and the cells' texture round the cells it changed
  (only that box sent), the props' squares where the list changed, a building's digest when it
  changed, the lights and sounds when what gives them off did. An undo step's copy of the map
  is made in memory grown at once.
- An idle frame looks at what's near, not at all of a big map: the props within reach are
  posed (tested for reach before their model is looked up), the sounds within earshot heard,
  the maps around keep their lamps and blocked cells from when they came, and the checks'
  hook of what lies along ramps is worked out again only when the props or the ramps change.
- Nearest first, and props without cut-outs on a shader with no `discard`: the GPU drops what's
  hidden before shading it. The ground (the costliest shader) is drawn last, under the rest.
- The ground shader skips the border votes wherever a cell and its neighbours have one
  terrain: most of the ground.
- The map editor sends props, trees and buildings to the GPU as separate layers: placing a
  prop doesn't send the forest again.
- Test hooks on `<body>` are written only when they change.
- Lamps are spot lights in the shader, the 32 nearest the camera each frame (no shadows from
  them). Creatures carry lights the same way (`light` on a creature record, from its feet and
  the way it faces): battlers on the stage and creatures roaming the editor's maps. Billboards
  are lit by the lamps too, as if they faced them.

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

At 1400 x 900 on the meadow itself (2026-10-19), the menu runs at ~540 fps uncapped and the
map editor's views at ~420 to 470 (about 2.2 ms a frame, most of it the same on any map). A
320 x 240 map (`tile=8`, 1.8M triangles in view) draws whole at 4K at
379 fps, and a prop placed on it costs 6 ms (`only=editing`).

Big maps (2026-10-19, RTX 3080, real-size trees; the editing scene paints a stroke of eight
cells of each kind and undoes; the last column is the build before edits and idle frames
cost what they touch, measured the same day):

| `perf.mjs tile=N creatures=0`, 1400 x 900 | 1000 x 750 (`tile=25`: 12,000 props) | 2000 x 1500 (`tile=50`: 47,000 props, a million cells of woods) | 2000 x 1500, before |
|---|---|---|---|
| A close view (the camera 14 cells off) | 373 fps | 354 fps | 107 fps |
| The whole map in view | 151 fps | 158 fps | 83 fps |
| Walking | 405 fps | 360 fps | 101 fps |
| A prop placed: worst frame | 9 ms | 20 ms | 34 ms |
| A stroke of terrain, of water | 9 ms, 11 ms | 17 ms, 13 ms | 24 ms, 27 ms |
| A stroke a level up, of a wood's cells | 14 ms, 9 ms | 13 ms, 15 ms | 99 ms, 59 ms |
| An undo | 14 ms | 20 ms | 98 ms |

An edit costs what it touches, not the map's size. The trees are placed again only where it
can move them (`world::trees::Woods`: the spots on the cells that changed are tried again,
then, in the order of placing, those near a tree that came or went, until nothing more
changes: the same trees as placing them all, which took 35 ms for a million cells of woods).
What changed in the cells is found by looking the map over row by row, eight bytes at a time:
the ground's squares are hashed again round it and the cells' texture sent only there (the
shore's field worked out again round it). The props' squares are hashed again where the list
changed (what stayed the same at its start and its end is passed over), a building's digest
only when it changed (Debug-formatting all 5,000 took 23 ms an undo), and the lights and
sounds are placed again only when what gives them off changed. An undo step's copy of the map
is made after what undo lets go is freed, in memory grown at once: made a piece at a time, a
big map's copy grew the browser's memory in thousands of small steps (25 ms more). The tools'
marks over cells cover only what the camera sees. `streamcost` says what a change's costliest
frame of the last two seconds spent its time on, step by step (`rest`: the rest of the frame).

An idle frame costs what's in view too. On the biggest, half of a close view's frame went to
looking props up by their id with the standard library's hash (SipHash), each of the 47,000
every frame: for the checks' hook of what lies along ramps, for the props with a rigged model
to pose, for the sounds to hear. Now that hook is worked out again only when the props or the
ramps change (among the props on a ramp's cells), a prop's reach is tested before its model is
looked up, and only the sounds within earshot go further. The renderer tests a square's box
before its pieces; the maps around keep their lamps and blocked cells from when they came
(they were worked out every frame, and every step across a seam); and the walker finds the
building it's in without listing every building's cells. A close view on the 2000 x 1500 map
keeps the processor 2.7 ms a frame (9.4 before), the 40 x 30 meadow's 2.2.

## Not yet

- The content browser only reads; the map editor saves maps (other records next).
- Maps: creatures roam but don't battle yet (walking into one does nothing; a script's
  `battle` does), actors and events don't know a building's floors yet; on stairs along a
  ramp the walker follows the ramp's slope, not each step; most props are still stand-in
  shapes (samples made in Blender: `stone_well`, `flag_pole`).
- Battle items (Potion, Capture Ball...) are still a fixed list in `crates/core`, not content.
- The overworld (Explore) has no doors into interiors as separate maps: buildings are
  walked into in place; between levels outside, ramps only (no stairs); the save is one slot.
- Sprites are battle pictures on billboards. The renderer can take 3D models later
  (glTF), the battle code doesn't care.
- No touch controls yet (keyboard, mouse and controllers); no music. Controllers don't
  rumble.

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
Attribution-ShareAlike 4.0 (`content/LICENSE`). The game's font, Roboto
(`crates/game/fonts/`), is under the SIL Open Font License 1.1 (`crates/game/fonts/OFL.txt`).
Contributions are welcome as pull requests; see `CONTRIBUTING.md` (original work only,
commits signed off).
