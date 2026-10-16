# Model requests: the recipe

The map editor's **Requests** window (desktop) takes pictures of a creature or a prop dropped
on it. Each request is a folder `private/requests/<id>/`: its `request.json` and the pictures.
Its **Start** button has Claude Code follow this recipe for that request by itself
(`run_claude.py`, below), the window's console showing everything it does; and when the user
says **"make the waiting requests"** to a Claude session, the session follows it for each
waiting one. Either way the request is modelled in Blender, from shapes, as a clean stylized
base (one mesh, a material per colour region, a rig for creatures), and added to the project
as a prop.

Rules that hold all the way:

- The pictures are often other artists' work. They stay in `private/` (never committed), and
  the model of a design that isn't the user's goes to `temporary/` (never committed either).
  Only a request whose **"This design is mine or licensed"** box is ticked (`"mine": true`)
  goes to `content/`, the public game.
- Never commit anything from `private/` or `temporary/`. A model that went to `content/` is
  committed with the rest of the work, after `python private/import/audit.py .` and
  `python tools/check-names.py`.
- One request at a time, start to finish, so the editor's progress bar means something.

## request.json

```json
{
  "id": "round_stone_lantern",
  "name": "Round stone lantern",
  "kind": "prop",
  "height": 1.2,
  "style": "smooth",
  "notes": "Warm grey stone, the base a little darker...",
  "mine": false,
  "status": "waiting",
  "progress": { "step": "", "done": 0.0, "started": 0.0, "minutes": 0.0 },
  "images": ["round_stone_lantern.png"],
  "previews": [],
  "prop": "",
  "reply": ""
}
```

(`progress` is laid out over several lines in the file, as the project's records are.)

- `kind`: `creature` (rigged, with an idle animation) or `prop`; `height` in cells (a cell is
  a metre; the player is about 1.5); `style`: `smooth` or `low_poly`.
- `status`: `waiting`, `in_progress`, `done`, or `failed` (the run ended without the model,
  stopped with an error or went silent: `progress.step` says why, `log.txt` has the rest).
  `progress` is what the editor's bar shows (in
  the window, with the time left, and as a line along the top bar's Requests button): the
  step, how far along (0 to 1), when the modelling started (seconds since 1970) and how many
  minutes it should take; once done, how many it took.
- `previews`: the renders of the model, in the folder; `prop`: the prop it became; `reply`:
  what the modeller says back (what it guessed, what to look at).

The window and the files are written the same way (`content::to_text`'s layout), and either
side reads the file again before writing it, so the editor and a session can both change a
request at once. `tools/blender/model_requests.py` reads and writes them from plain Python:

```bash
python tools/blender/model_requests.py list
python tools/blender/model_requests.py start ID [--minutes N]
python tools/blender/model_requests.py step ID "what it's doing" [--done 0.4]
python tools/blender/model_requests.py wait ID [--note "why"]
python tools/blender/model_requests.py fail ID "why"
python tools/blender/model_requests.py note ID "what happened"
python tools/blender/model_requests.py new "Name" PICTURE... [--kind prop] [--height 1.2] [--style low_poly] [--notes "..."] [--mine]
```

(`new` makes a request without the editor, from pictures on disk. `note`, `wait --note` and
`fail` add a line to the request's `log.txt`, timed by the computer's clock.)

## From the editor: Start, Cancel and the console

Each request's row in the window's list, and the request's own panel, has a **Start** button
(Start again for a failed one, Make it again for a done one) and, while it's being made, a
**Cancel** button; one request is made at a time. **Claude:** at the top picks the model
(Claude Code's default, or opus, sonnet, haiku). Start runs

```bash
python tools/blender/run_claude.py ID [--model opus]
```

on its own (it goes on if the editor closes), which:

- finds Claude Code (`CLAUDE` names the program, else `claude` on the PATH, else
  `~/.local/bin/claude`) and runs it headless: `claude -p --output-format stream-json
  --verbose --permission-mode dontAsk --strict-mcp-config --no-session-persistence --settings
  private/requests/ID/claude-settings.json`, the prompt on its input: this recipe for this
  request, from step 3, unattended;
- allows it only what the recipe needs, without asking (`claude-settings.json`): reading the
  project, writing and editing in the request's folder, `python tools/blender/run_request.py
  ID` (Blender), `python tools/blender/model_requests.py ...`, the content check and `ls`.
  Anything else is refused, never asked: nobody is there to answer;
- writes everything to the request's `log.txt`, a line each, timed: the run starting, the
  program and its version, what was sent (the command, what's allowed, the prompt), "Claude
  reached" with the model it runs, each message, thought, tool call (→) and answer (←, × for
  an error), what was refused, the end (turns, time, cost at API prices) and Claude's last
  word; the raw stream goes to `run.jsonl`. The window's console shows that log (its last lines
  when folded, all of it open), Copy puts all of it on the clipboard;
- ends the request **done** when Claude has added the model (`--finish`), **failed** when
  Claude ends without it, stops with an error or isn't logged in, or says nothing for 30
  minutes (then it's stopped); **waiting** again on Cancel (the editor writes a file named
  `stop` in the request's folder). The editor marks a request failed itself when nothing has
  been written in its folder for 31 minutes (a run that was killed), and settles a Cancel with
  nothing running after a few seconds.

Claude Code must be logged in for this, once (the desktop app's own login isn't the command
line's): until then a run fails at once with "Claude Code isn't logged in", naming the
program's path (it's often not on the PATH: `~/.local/bin/claude.exe`), and the request's
panel has a **Log in to Claude Code…** button that opens it in a terminal, in the project:
type `/login` there, then Start again. `python tools/blender/test_run_claude.py` checks the whole of
it against a stand-in for Claude: a run that finishes, one that ends without the model, one
that crashes, one not logged in, one gone silent, one cancelled.

## The recipe

Blender runs through `python tools/blender/run_request.py ID [--finish]`: in the background
(no window opens), on the request's `model.py`, the program named by `BLENDER`, else the
newest under Program Files (`"C:/Program Files/Blender Foundation/Blender 3.1/blender.exe"`
here). Everything runs from the project folder.

1. **List them**: `python tools/blender/model_requests.py list`. Take the first waiting one.
2. **Start it**: `python tools/blender/model_requests.py start ID`. The editor's bar starts
   (the estimate: 12 minutes for a prop, 25 for a creature, a fifth less in low-poly; give
   `--minutes` when the pictures promise much more or less work).
3. **Read it**: `private/requests/ID/request.json`, then every picture (look at each one).
   Note what views there are (front, side, back, details), the proportions (the height
   split into the head, the body, the legs; the widest point; where things attach), the
   colour regions with a hex for each (take them from the pictures) and the few shapes that
   make the silhouette. The notes win over the pictures. What no picture shows, guess from
   the design's own logic, and say so in the reply.
4. **Write `private/requests/ID/model.py`** with the kit (below). Build at any scale (the
   pictures' pixels, say): `fit` gives it the request's height. Creatures: a metaball `blob`
   for each soft mass (body, head), `capsule`s for limbs and tails, `slab`s for ears, fins,
   crests and wings, `paint` volumes for markings (a belly, spots, a mask), `eyes`. Props:
   `lathe` for anything round, `box` and `cylinder` for hard shapes, `cuts` for openings,
   `paint` for patterns, `light` for a lamp. Big readable shapes: it's a base to refine, seen
   from the game's camera, not a sculpt.
5. **Render it**: `python tools/blender/run_request.py ID`. It writes `preview_front.png`, `preview_side.png`,
   `preview_three_quarter.png` (Workbench, the material colours) and `compare.png` (the
   pictures on top, the renders under them, the same height). Look at `compare.png`, and at
   a preview at full size when a detail matters.
6. **Compare and try again**, usually three to six times: the silhouette first (front and
   side outlines: proportions, the head against the body, where limbs attach), then the
   masses, then the colour regions and their borders, then the details that carry the
   character (the eyes' size and place, ears, a tail's curve). Edit `model.py`, render again.
   Stop when the renders read as the pictures' design at a glance; don't chase a sketch's
   wobbles.
7. **Creatures: rig it** in `model.py` after `fit`: `rig(body, chains)` from the model's own
   landmarks (in cells, the model on the origin facing -Y): `spine` (hips to chest), `neck`
   and `head`, a chain per limb named with its side (`leg.L`, `arm.R`), `tail`, `ear.L`...
   Then `idle(arm)`. Render again: `preview_idle.png` shows four moments of the idle (look
   for vertices torn by the wrong bone), and the rig's line says how many vertices the
   automatic weights missed (many: a bone is outside the mesh).
8. **Finish it**: `python tools/blender/run_request.py ID --finish`. The model is added with
   the add-on's `write_prop` (to `temporary/`, or `content/` when `mine`), as the prop
   named by the request's id (numbered when another prop has it); the engine's own reading
   of the file is printed (triangles, size, its light, its rig's joints and "idle"); the
   request is done, with its renders, the prop, the reply and the time it took. Put the
   reply in `model.py` (`finish(..., reply=...)`): what was guessed, what to look at.
9. **Check it**: `cargo run -p engine-content --bin content -- check`. The prop shows in the
   map editor's asset browser at once (with a TEMPORARY tag when it went there), and the
   Requests window shows the request done, with its renders and "Use it".
10. **Next**: back to 1 until none waits. Then tell the user what was made, with the replies.

A request left in progress (a session that stopped): the editor marks it failed after 31
minutes without news; Start again (or `model_requests.py wait ID`) puts it back in the queue.

## The kit: `request_kit.py`

`model.py` starts with:

```python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools" / "blender"))
import request_kit as kit  # noqa: E402

job = kit.start(__file__)   # the request, the scene emptied, the request in progress
```

`job.record` is the request; `job.creature`, `job.smooth`, `job.height`, `job.pictures()`.
Positions are in Blender units (one is a cell), Z up, the front toward -Y; rotations in
degrees about X, Y and Z. Each shape takes a colour `region` (a name).

| Shapes | |
|---|---|
| `sphere(at, radius, region, scale=, rotation=)` | |
| `ellipsoid(at, (rx, ry, rz), region, rotation=)` | |
| `capsule(a, b, radius, region, radius_b=)` | a limb from `a` to `b`, tapering to `radius_b` |
| `box(at, (sx, sy, sz), region, rotation=)` | centred on `at` |
| `cylinder(at, radius, height, region, top=, rotation=)` | standing on `at`; `top=0` is a cone |
| `torus(at, radius, thickness, region, rotation=)` | a ring lying flat |
| `lathe([(radius, z), ...], region)` | turned about the vertical, bottom to top, ends closed |
| `slab([(x, z), ...], thickness, region, at=, rotation=)` | a flat outline (front view), extruded |
| `blob([("ball", at, r), ("ellipsoid", at, r, (sx, sy, sz)), ("capsule", at, r, length)], region)` | metaballs melted together (a negative radius carves; the surface sits inside the radius, about 0.6 to 0.8 of it) |
| `mirrored(part, "x")` | a copy on the other side |

| Making it one model | |
|---|---|
| `fuse(parts, name, colours, smooth=, voxel=, faces=, cuts=[...], paint=[...])` | joined and voxel-remeshed into one clean watertight mesh, `cuts` carved out, each face given the material of the nearest part's region (then of the `paint` volume it's in), decimated to about `faces` triangles (smooth 6000, low-poly 800) sparing the borders between regions, smooth (weighted normals) or flat shaded |
| `colours` | region to `"#rrggbb"` or `{"color": ..., "rough": 0.65, "metal": 0, "glow": 0, "specular": 0.5}` |
| `eyes(body, [centres], radius, colour=, look=)` | glossy eyes with a glint, set into the body (group "eyes", which the rig gives the head) |
| `fit([objects], height)` | scaled to `height` cells, standing on the origin |
| `light(at, color, intensity, reach, kind="point")` | a lamp's light, which becomes the prop's (lit at night) |
| `rig(body, {chain: [points]}, parents=)` | an armature, the body skinned with automatic weights; a chain hangs from the bone nearest its first point |
| `idle(arm, seconds=2.0, amount=1.0)` | a looping "idle" action from the bones' names: spine breathes, neck and head nod and look, tail/ear/antenna sway, arms and fins swing, wings stir, legs stay |
| `previews(job, objects)`, `compare(job, files)` | the renders and the sheet (above) |
| `finish(job, objects, files, reply=, category=, blocks=)` | with `-- --finish`: into the project, the request done |

The steps set the editor's bar by themselves: reading the pictures (5%), building (15%),
rendering (50%), comparing (60%), adding it (95%), done. The bar shows those, or the time
gone against the estimate when that's further along, and never fills before it's done.

What the engine does with it: draws the triangles and the materials' base colours, lights
the prop's light, and plays a rigged prop's "idle" in a loop. Roughness, glossy eyes and
glow stay in the file for Blender and for later: in the game a glowing window is its bright
base colour, lit like the stone around it.

Blender 3.1 can't read WebP: the editor keeps a dropped WebP as PNG (and so does
`model_requests.py new`).

## A worked example: the round stone lantern

An original sketch (by Claude, for this example): a round stone lantern from the front,
turned 45 degrees and from above, with notes and colour swatches. The request: a prop 1.2
cells tall, smooth, the design not marked as the user's, so its model went to `temporary/`.

What the sketch says: round all the way up, so each part is a `lathe` of the front view's
outline (measured at 520 pixels to the cell): a darker two-step plinth, a slim post with a
ring, a dish, the light chamber with four windows (one each way, from the top view), a
domed roof with a lifted rim and moss on top, a small ball on the very top, a warm light.

```python
"""Round stone lantern: the worked example of tools/blender/requests.md."""
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools" / "blender"))
import request_kit as kit  # noqa: E402

job = kit.start(__file__)

# the outline, (radius, height) in cells, as the sketch's front view has it
ROOF = [(0.0, 0.80), (0.22, 0.80), (0.42, 0.835), (0.465, 0.875), (0.43, 0.888), (0.31, 0.95), (0.19, 1.02), (0.08, 1.065), (0.03, 1.08), (0.0, 1.085)]
parts = [
    kit.lathe([(0.0, 0.0), (0.40, 0.0), (0.40, 0.06), (0.37, 0.09), (0.29, 0.09), (0.27, 0.14), (0.0, 0.14)], "base"),
    kit.lathe([(0.0, 0.13), (0.11, 0.13), (0.11, 0.29), (0.135, 0.31), (0.11, 0.33), (0.11, 0.48), (0.0, 0.48)], "stone"),
    kit.lathe([(0.0, 0.47), (0.12, 0.47), (0.17, 0.49), (0.28, 0.52), (0.28, 0.56), (0.0, 0.56)], "stone"),
    kit.lathe([(0.0, 0.555), (0.19, 0.555), (0.19, 0.805), (0.0, 0.805)], "stone"),
    kit.lathe(ROOF, "stone"),
    kit.cylinder((0, 0, 1.06), 0.03, 0.05, "stone"),
    kit.sphere((0, 0, 1.13), 0.05, "stone"),
]
# four windows, one each way, cut into the chamber: their insides glow
windows = [kit.box((0.19 * math.sin(math.radians(a)), -0.19 * math.cos(math.radians(a)), 0.68), (0.12, 0.12, 0.14), "light", rotation=(0, 0, a)) for a in (0, 90, 180, 270)]


def roof_height(r):
    """The roof's top at a distance from the middle (the outline, its upper side)."""
    top = ROOF[4:]
    for (r0, z0), (r1, z1) in zip(top, top[1:]):
        if r1 <= r <= r0:
            return z0 + (z1 - z0) * (r0 - r) / (r0 - r1)
    return ROOF[-1][1]


# moss in patches on the roof's top
moss = []
for angle, r, size in ((205, 0.28, 0.09), (35, 0.22, 0.11), (120, 0.33, 0.07), (300, 0.13, 0.06), (250, 0.36, 0.05)):
    a = math.radians(angle)
    moss.append(kit.ellipsoid((r * math.cos(a), r * math.sin(a), roof_height(r)), (size, size * 0.8, 0.05), "moss", rotation=(0, 0, angle)))

colours = {"base": "#7b776d", "stone": "#b0ab9f", "light": {"color": "#ffd27a", "glow": 2.0, "rough": 0.5}, "moss": "#6f924a"}
body = kit.fuse(parts, job.id, colours, smooth=job.smooth, cuts=windows, paint=moss)
glow = kit.light((0, 0, 0.68), "#ffc46b", intensity=0.9, reach=5.0)
kit.fit([body, glow], job.height)

files = kit.previews(job, [body])
kit.compare(job, files)
kit.finish(job, [body, glow], files, reply="Made from the front, the 45-degree and the top views...")
```

The tries (each one 2 to 3 seconds of Blender):

1. The shape was right at once; every region came out grey. (The kit emptied the mesh's
   materials after giving the faces theirs, which resets them: fixed in the kit.)
2. The colours came through; the moss patches looked like shards: decimating had left long
   triangles on the flat roof, so the regions' borders were jagged. (The kit now gives the
   regions their faces on the dense mesh and decimating spares the borders.)
3. Round patches; streaks of shading on the flat parts. (The kit now turns the edges of
   long triangles where it's nearly flat, and weights the normals by the faces' areas.)
4. Clean shading. The sketch's base reads darker and its roof's rim is sharper: the base's
   colour went from `#8a867c` to `#7b776d`, the rim's outline lifted. Finished.

The result: 6000 triangles, 0.93 x 1.20 x 0.93, a warm point light inside (intensity 0.9,
5 cells, at night), `temporary/data/props/round_stone_lantern.json` and
`temporary/assets/models/round_stone_lantern.glb`; 4 minutes from start to done (writing
`model.py` before `start` not counted). The request stays done in the author's
`private/requests/round_stone_lantern/`.

## Checking the kit

The add-on's test (`tools/blender/test_addon.py`, see `README.md`) makes a small creature
and a pot through the kit in its scratch project: one mesh, its regions and glossy eyes, the
height asked, a rig whose every vertex is weighted with the eyes on the head, the idle, the
renders, the prop in `temporary/` (and a design of the user's in `content/`), the request
done, and the engine reading the rig and the idle.
