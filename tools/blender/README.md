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
  remodelled.
- **Maps**: pick one, "Export and import the map": the project writes it as a `.glb` and
  Blender opens it (ground, props and trees as linked copies, buildings, each its own
  object). This needs Rust's `cargo`; without it, use the map editor's "To Blender" button
  and import from the exports below.
- **Exports**: anything the game exported (`build/exports/`: maps, buildings, props) can be
  imported from the list.

**From Blender into the project**

Select the objects that make your asset and click **Add to the project**. Give it an id
(lowercase, `_`), a name, a category, whether it blocks the way, and for windows the sill
height. The add-on:

- exports the selection as `content/assets/models/<id>.glb`,
- measures it: its footprint in cells (doors and windows fit openings as wide as their
  footprint) and its height,
- writes `content/data/props/<id>.json` laid out exactly as `content format` does, keeping
  what an existing record already had (its stand-in shapes, for instance).

Then `cargo run -p engine-content --bin content -- check`, and the prop is in the editor's
Place list (and the Build tool's lists, for building pieces).

## Conventions

The same as the engine's, so nothing needs converting:

- one Blender unit is one cell (a metre), and a level (one floor) is 2 units high;
- the asset stands on Z = 0 with the middle of its footprint at the origin ("Stand it on
  the origin" does that for you when exporting);
- its front faces the front view (-Y in Blender, +Z in glTF and in the engine);
- colours: materials' base colour, vertex colours and base colour textures are all kept.

## Without the add-on

- `cargo run -p engine-world --bin export-glb -- prop ID | props | map ID | building MAP ID`
  writes `.glb` files to `build/exports/`; the map editor's "To Blender" buttons do the same
  (or download them, in the browser).
- `cargo run -p engine-world --bin export-glb -- check FILE` reads any `.glb` the way the
  engine does and says what's in it.
- A model made elsewhere: put it in `content/assets/models/` and set `"model":
  "models/<file>.glb"` in the prop's record.

## Testing the add-on

```bash
blender --background --factory-startup --python-exit-code 1 --python tools/blender/test_addon.py -- . build/blender-project
```

It works on a scratch copy (`build/blender-project`), never on `content/`, except that the
project exports a map to `build/exports/`.
