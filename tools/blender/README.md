# Blender and Battle Frontier

Everything goes both ways through glTF (`.glb`), which Blender reads and writes itself.

## The add-on

`battle_frontier.py` is a Blender add-on (Blender 3.1 and newer).

**Install it:** Edit > Preferences > Add-ons > Install... (Blender 4.2 and newer: the menu
at the top right, "Install from Disk..."), pick `battle_frontier.py`, tick it. The panel is
in the 3D view's sidebar (press N), tab **Battle Frontier**.

**The project is found by itself:** from the `BATTLE_FRONTIER_ROOT` environment variable,
from the folder the add-on runs in, or by looking a few folders deep in your home folder and
drives. You can also set it in the add-on's preferences, or click "Find the project".

**From the project into Blender**

- **Props**: pick one, "Import the prop". A prop with its own model comes in from its
  `.glb`; a stand-in comes in rebuilt from its shapes, one material per colour, ready to be
  remodelled. A lamp's light comes in as a light object on it.
- **Every prop**: "Export and import every prop" brings the whole set in one go, a row per
  category, each prop with its cells and its light, each object knowing which prop it is
  (select one, remodel it, "Add to the project" with "Replace" sends it back). This needs
  Rust's `cargo`; without it, the asset browser's "All props to Blender" button writes the
  same `props.glb`, to import from the exports below.
- **Maps**: pick one, "Export and import the map": the project writes it as a `.glb` and
  Blender opens it (ground, props and trees as linked copies, buildings, each its own
  object). This needs `cargo` too; without it, use the map editor's "To Blender" button
  and import from the exports below.
- **Exports**: anything the game exported (`build/exports/`: maps, buildings, props) can be
  imported from the list.

**From Blender into the project**

Select the objects that make your asset (its meshes, its rig, its lights and speakers) and
click **Add to the project**. Give it an id (lowercase, `_`), a name, a category, whether
it blocks the way, and for windows the sill height. The add-on:

- exports the selection as `content/assets/models/<id>.glb`: meshes, materials with their
  textures, vertex colours, rigs (armatures and skinning), animations and shape keys, all
  as Blender's own glTF exporter writes them (untick "Apply modifiers" to keep shape keys),
  and its lights; never a camera (the game has one camera, its own),
- measures it: its footprint in cells (doors and windows fit openings as wide as their
  footprint) and its height,
- writes `content/data/props/<id>.json` laid out exactly as `content format` does, keeping
  what an existing record already had (its stand-in shapes, for instance), with:
  - its **light**, from the first spot or point light selected: 1000 W in Blender is an
    intensity of 1, the light's colour, its spot size is the cone, its custom distance the
    range (else the record's, or 6 cells), where it sits and points from the prop's origin;
  - its **sound**, from the first speaker selected: the sound file is copied to
    `content/assets/sounds/<id>_<file>`, with the speaker's volume and maximum distance:
    the prop plays it where it stands, looping, fading with distance.

Then `cargo run -p engine-content --bin content -- check`, and the prop is in the editor's
Place list (and the Build tool's lists, for building pieces).

**What the engine does with it:** draws the triangles, the materials' base colour and base
colour texture and the vertex colours; lights its light; plays its sound; and, when it has
a rig, poses it with its animation named "idle" (else its first) in a loop, with shadows
(`tools/blender/make_flag.py` makes a sample: a flag pole whose flag waves). The rest of a
material (roughness, normal maps, emission...), shape keys and the other animations stay in
the file untouched, for Blender and for later.

## Conventions

The same as the engine's, so nothing needs converting:

- one Blender unit is one cell (a metre), and a level (one floor) is 2 units high;
- the asset stands on Z = 0 with the middle of its footprint at the origin ("Stand it on
  the origin" does that for you when exporting);
- its front faces the front view (-Y in Blender, +Z in glTF and in the engine);
- colours: materials' base colour, vertex colours and base colour textures are all kept;
- lights: a spot or point light object on the prop, pointing down its -Z as Blender's do;
  1000 W is an intensity of 1. The add-on reads and writes them in Blender's own watts
  (the glTF "Raw" lighting mode of Blender 3.6 and newer, which is all Blender 3.1 to 3.5
  ever does). A `.glb` exported by Blender 3.6 or newer in its default "Standard" mode
  carries physical candela, 54 times as much; the engine notices (an intensity past 10)
  and brings it back. Blender's own File > Import with "Standard" units shows the game's
  lamps 54 times too dim: pick "Raw" there, or import through the add-on.

## Without the add-on

- `cargo run -p engine-world --bin export-glb -- prop ID | props | map ID | building MAP ID`
  writes `.glb` files to `build/exports/` (`props`: each prop's own file and all of them
  laid out in `props.glb`); the map editor's "To Blender" buttons do the same (or download
  them, in the browser).
- `cargo run -p engine-world --bin export-glb -- check FILE` reads any `.glb` the way the
  engine does and says what's in it: triangles, textures, size, lights, and what it drops
  (cameras) or keeps unread (rigs, animations).
- A model made elsewhere: put it in `content/assets/models/` and set `"model":
  "models/<file>.glb"` in the prop's record; or drop the `.glb` on the editor's asset
  browser, which writes the record (its size measured, its first light read from the file).

## Testing the add-on

```bash
blender --background --factory-startup --python-exit-code 1 --python tools/blender/test_addon.py -- . build/blender-project
```

It works on a scratch copy (`build/blender-project`), never on `content/`, except that the
project exports a map to `build/exports/`.
