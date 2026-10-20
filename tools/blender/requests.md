# Model requests: the recipe

The map editor's **Requests** window (desktop) takes pictures of a creature, a prop or a floor
dropped on it. Each request is a folder `private/requests/<id>/`: its `request.json` and the pictures.
Its **Start** button has Claude Code follow this recipe for that request by itself
(`run_claude.py`, below), the window's console showing everything it does; and when the user
says **"make the waiting requests"** to a Claude session, the session follows it for each
waiting one. Either way the request is modelled in Blender in parts, as a modeller makes a
model for a rigger (each part its own mesh, blocked in from shapes and refined, a material per
colour region; for a creature a rig every part follows, its jaw and tongue on bones of their
own), and added to the project as a prop (a floor as a set of floor tiles: "A set of floor
tiles", below).

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
  "cells": [1, 1],
  "style": "smooth",
  "details": "",
  "pose": "t_pose",
  "shapes": { "min": 8, "max": 30, "unlimited": false },
  "notes": "Warm grey stone, the base a little darker...",
  "variants": 0,
  "vary": "",
  "mine": false,
  "check_lines": false,
  "status": "waiting",
  "progress": { "step": "", "done": 0.0, "started": 0.0, "minutes": 0.0 },
  "images": ["round_stone_lantern.png"],
  "bones": {},
  "outlines": {},
  "lines": [],
  "previews": [],
  "prop": "",
  "variant_props": [],
  "reply": "",
  "comments": "",
  "earlier": [],
  "several": false,
  "parent": "",
  "children": []
}
```

(`shapes` and `progress` are laid out over several lines in the file, as the project's records
are.)

- `kind`: `creature` (rigged, with an idle animation), `prop`, `grass` (a patch of grass
  filling its `cells` to the edges, so copies side by side join without a gap: below,
  "Grass"), `environment` (what dresses the outdoors: rocks, bushes, flowers, logs, trees;
  below, "Environment"), or `floor` (the window's **floor set**: a set of floor tiles, each a
  cell, laid to fit; below, "A set of floor tiles"; its `height` and `check_lines` go
  unused); `height` in cells (a cell is a metre; the player is about 1.5); `cells` (grass):
  the cells it fills, across x and deep z; `style`: `smooth`, `low_poly` or `accurate` (as
  close to the pictures as it can be, whatever it takes: see "Accurate" below), and with it
  `details`, the user's word on how to build it.
- `variants` (the window's **Variants**, grass and environment, and any request once it's
  made): how many variants of it to make besides it, each a prop of its own; `vary` (**What
  changes**): how they differ (its size, its colours...), else as Claude sees fit; with
  `several`, each drawing's request makes that many of its own. `variant_props`: the props
  they became (below, "Variants").
- `pose` (creatures): `t_pose`, the rest pose a rigger wants whatever the pictures show (two
  legs: standing straight, arms straight out to the sides at shoulder height; four legs:
  standing straight on straight legs, head forward, tail straight back; more legs: each leg
  straight out to its side then down to the ground, evenly spread; wings: spread straight out;
  no limbs, a grub, a snake, a fish: the body straight and level along its length, head
  forward, nothing curled), or `as_is` (the pose the pictures show).
- `shapes`: how many shapes to build it from, `min` to `max`, or `unlimited`: as many as the
  design needs, up to about 100 (work out the list of parts first; take the time it takes).
- `check_lines` (the window's **"Let me check the lines first"**): the user looks at the
  lines before the model is built. The modelling stops once the lines and the front and the
  side are drawn (step 5) and the request waits again, its `progress.step` saying
  `lines ready: check them`, until the user says go on (below, "Checking the lines first").
- `status`: `waiting`, `in_progress`, `done`, or `failed` (the run ended without the model,
  stopped with an error or went silent: `progress.step` says why, `log.txt` has the rest).
  `progress` is what the editor's bar shows (in
  the window, with the time left, and as a line along the top bar's Requests button): the
  step, how far along (0 to 1), when the modelling started (seconds since 1970) and how many
  minutes it should take; once done, how many it took.
- `bones` (creatures): the bones the user has drawn over the pictures in the editor's Bones window,
  by picture (below, "The user's bones"): lines of joints in the picture's pixels, each a part
  (`leg`, `arm`, `torso`, `head`, `ear`, `horn`, `tail`, `wing`, or `misc` with the `name` the
  user gave the piece). Empty when the user has drawn none.
- `lines`: the drawings the modeller made to work from (below, "Lines and profiles"): its
  lines over each picture, the front and the side drawn from them, and once rendered those lines over the
  model, in the folder; the window shows them as they come.
- `previews`: the renders of the model, in the folder; `prop`: the prop it became; `reply`:
  what the modeller says back (what it guessed, what to look at).
- `comments` (the window's **Make it again** box, once a run has made it): what the user says
  of that model for the next run, what to look for and what to change; `earlier`: the comments
  the runs that made it again were given, oldest first (below, "Made again, with the user's
  comments").
- `several` (the window's **"Process multiple"**): the pictures hold several drawings of the
  same kind of asset (trees, bushes, rocks, plants, grass); the first run splits it, each
  drawing a request of its own, listed in `children` (each one's `parent` this request), and
  the request is done once split (below, "Process multiple"). `outlines`: the drawings the user
  has outlined to make (the window's Outlines), by picture, each a closed line of points in the
  picture's pixels with its `height` and `cells`; only those are made, the rest left out; none,
  all of them.

The window and the files are written the same way (`content::to_text`'s layout), and either
side reads the file again before writing it, so the editor and a session can both change a
request at once. `tools/blender/model_requests.py` reads and writes them from plain Python:

```bash
python tools/blender/model_requests.py list
python tools/blender/model_requests.py start ID [--minutes N]
python tools/blender/model_requests.py step ID "what it's doing" [--done 0.4]
python tools/blender/model_requests.py wait ID [--note "why"] [--check]
python tools/blender/model_requests.py fail ID "why"
python tools/blender/model_requests.py note ID "what happened"
python tools/blender/model_requests.py zoom ID PICTURE [X0 Y0 X1 Y1] [--grid] [--lines]
python tools/blender/model_requests.py colours ID PICTURE [X0 Y0 X1 Y1]
python tools/blender/model_requests.py bones ID
python tools/blender/model_requests.py lines ID
python tools/blender/model_requests.py new "Name" PICTURE... [--kind prop] [--height 1.2] [--style low_poly] [--notes "..."] [--mine] [--check-lines]
```

(`new` makes a request without the editor, from pictures on disk. `note`, `wait --note` and
`fail` add a line to the request's `log.txt`, timed by the computer's clock; `wait --check`
leaves a request waiting for the user to check its lines. `zoom`, `colours`, `bones` and
`lines` are for reading the pictures: below.)

## From the editor: the bones, Start, Cancel and the console

Before Start, a creature's panel has **Draw the bones…**: a window with one of its pictures
large, to draw the creature's bones over it, a colour per part (`leg`, `arm`, `torso`, `head`,
`ear`, `horn`, `tail`, `wing`, and `misc` for any piece with no name there, which the user
names: a crest, a shell, a fin). A click starts a line and adds each joint, chained, from its
root out (a hip, then the knee, then the foot); a right-click puts the last joint and ends the
line; Backspace takes a joint back, Escape drops the line being drawn, Undo takes back the last
joint or line, Clear empties the picture. Each line is kept in the request's `bones` at once,
and the run reads the pictures by them (below, "The user's bones").

Each request's row in the window's list, and the request's own panel, has a **Start** button
(Start again for a failed one, Make it again for a done one: the whole recipe again, from the
pictures; to change the model a run made instead, below, "Made again, with the user's
comments") and, while it's being made, a **Cancel** button; one request is made at a time. **Claude:** at the top picks the model
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
- meanwhile the request's panel shows what Claude draws as it draws it (**Lines**: its lines
  over the pictures, the front and the side from them, then those lines over the model with
  how far they overlap): a look says whether it read the pictures right, and Cancel stops a
  run that didn't, before the long part (or tick "Let me check the lines first": below);
- ends the request **done** when Claude has added the model (`--finish`), **failed** when
  Claude ends without it, stops with an error or isn't logged in, or says nothing for 30
  minutes (then it's stopped); **waiting** again on Cancel (the editor writes a file named
  `stop` in the request's folder), and once its lines are ready for the user's check (below).
  The editor marks a request failed itself when nothing has been written in its folder for 31
  minutes (a run that was killed), and settles a Cancel with nothing running after a few
  seconds.

### Checking the lines first

Tick **"Let me check the lines first"** in the request's panel (`check_lines`) to look at the
lines before the long part. Start then has Claude follow the recipe to step 5 and stop once
`profiles.png` is drawn: `run_claude.py` ends that run with the request **waiting**, its step
`lines ready: check them` (LINES READY in the list, whose row has **Check**; a run that ends
without drawing the front and the side has failed). The request's panel then shows, at its
top, the lines over each picture and the front and the side, what Claude says about them
(what it took for the drawing, what it guessed), a note for what's off and **The lines look
right: go on**, which runs

```bash
python tools/blender/run_claude.py ID --go-on [--note "the tail is longer"]
```

Claude keeps `lines.json`, fixes what the note says first (running `lines` again), and goes
on from step 6, the model, to done. **Start over** in the panel starts again from the
pictures instead, the note going with it: the lines there are checked against the pictures,
fixed, and with the box still ticked it stops for the check again. The bar runs on the lines'
part of the estimate (about two fifths), then on the model's.

### Made again, with the user's comments

Once a run has made a request's model, its panel has **Make it again** under what Claude says:
a box for what to look for and what to change (kept in `request.json` as it's typed, its
`comments`) and **Make it again with these comments**, which runs

```bash
python tools/blender/run_claude.py ID --again [--note "the trunk thicker at its foot"]
```

(`--note` over the record's `comments`). Claude starts from what the last run made, never over
from nothing: it reads `model.py` (and `design.py` when there is one), looks at the renders and
the drawings, is told what that run said back (`reply`), what the runs that made it again before
were told (`earlier`: those changes are made, kept so) and the user's comments, and makes those
changes and what follows from them, keeping the rest. A change to the shapes or the
proportions the lines set goes into `lines.json` first (`lines` again). Then it renders,
compares and adds it again (steps 7 to 11, `--finish` as the same prop), and ends saying what it
changed for each comment and what it left. Once it's done, the comments join `earlier` and the
box empties (unless the user has written others meanwhile); a run that fails or is cancelled
leaves them in the box, to try again. A request with no `model.py`, or nothing said, fails at
once, saying so.

### Process multiple

An artwork with several drawings of the same kind of asset (five trees, a row of bushes, a
sheet of rocks) makes several assets: tick **"Process multiple"** before Start. Its
**Outline the drawings…** opens the Outlines window: a picture large, to draw a line round each
drawing to make (a click for each point, a right-click closing it; Escape drops the one being
drawn, Backspace or Undo takes a point back), each numbered, with its own height and cells (as
tall as the request and on a cell at first) and its × to take it out. Only the drawings outlined
are made, the rest left out; with none outlined, all of them are. The split run then makes a
request of each outline, by its number:

```bash
python tools/blender/model_requests.py outlines ID          # each outline numbered over its picture (outlines_PICTURE.png)
python tools/blender/model_requests.py split ID --outline 2 --name "Round bush" [--notes "the darker one"]
```

its picture cut to the outline (what's outside it clear), its height and cells the user's (the
cells as the ground it covers; grass fills them). With no outlines, that run doesn't model:
Claude finds each drawing (all of it), names it after what it is,
works out how tall it stands (the request's height is the tallest's, the others' by how they
compare with it in the picture) and what's only its, and makes it a request of its own:

```bash
python tools/blender/model_requests.py split ID PICTURE X0 Y0 X1 Y1 --name "Tall pine" --height 9 [--notes "the darker one"]
```

The box (in the picture's pixels, or fractions of it) cropped is the new request's picture;
it gets the request's settings (kind, style and details, pose, shapes, mine, the lines' check)
and notes, its own notes, and where on the picture it was, the whole picture named for the
style they share; its id is the request's followed by its name's. The request is done once
split ("split into N requests"), its panel listing them under **Its requests**, with
**Make them all**: each one not made yet is run in turn, the next starting once the last has
ended (one at a time, as always; one whose lines wait for the user's check is left to them).
Each is a request as any other, made by the whole recipe: its own log, its renders, **Make it
again**, and a **Show it** back to the request it came from. Started again once split, the
request runs nothing. Its variants settings go to each (`variants`, `vary`): each drawing's
request makes that many variants of its own.

### Variants of a model made

Once a run has made a request's model (any kind but a set of floor tiles), its panel has
**Variants** and **What changes** with its settings, and **Make variants** under Make it
again, which runs

```bash
python tools/blender/run_claude.py ID --variants
```

Claude starts from what the last run made, as a run made again does (its `model.py`, the
renders, the drawings, what it said back), keeps the model itself exactly as it was made, and
adds as many variants as `variants` says, as `vary` says they differ ("Variants" below): the
style, the notes and the rest as they were. Variants an earlier run made are made again,
numbered from 2 again; `variant_props` lists the ones this run made. A request not made yet, or
with no variants asked for, runs nothing.

Claude Code must be logged in for this, once (the desktop app's own login isn't the command
line's): until then a run fails at once with "Claude Code isn't logged in", naming the
program's path (it's often not on the PATH: `~/.local/bin/claude.exe`), and the request's
panel has a **Log in to Claude Code…** button that opens it in a terminal, in the project:
type `/login` there, then Start again. `python tools/blender/test_run_claude.py` checks the whole of
it against a stand-in for Claude: a run that finishes, one that ends without the model, one
that crashes, one not logged in, one gone silent, one cancelled, one that stops after the
lines for the user's check, one that goes on from them with the user's note, one made again
with the user's comments, one split into a request per drawing, grass with its variants, and
the variants of a model made; and a set of floor tiles,
laid by the engine's rules (`floor_wfc.py`: every two cells side by side agreeing, rims all
round, the same seed the same floor), its run the floor recipe.

## The recipe

Blender runs through `python tools/blender/run_request.py ID [--finish]`: in the background
(no window opens), on the request's `model.py`, the program named by `BLENDER`, else the
newest under Program Files (`"C:/Program Files/Blender Foundation/Blender 3.1/blender.exe"`
here). Everything runs from the project folder.

1. **List them**: `python tools/blender/model_requests.py list`. Take the first waiting one
   (not one whose lines wait for the user's check: its line says "lines ready: check them").
2. **Start it**: `python tools/blender/model_requests.py start ID`. The editor's bar starts
   (the estimate: 8 minutes for a prop, 12 for a creature, a fifth less in low-poly, two and a
   half times as long accurate, half as long again with unlimited shapes; give `--minutes`
   when the pictures promise much more or less work).
3. **Read it**: `private/requests/ID/request.json`, then every picture (look at each one;
   `python tools/blender/model_requests.py zoom ID PICTURE X0 Y0 X1 Y1` enlarges a part of one
   into the folder, the box in pixels or fractions of the picture, to look at closely: a small
   creature in a big picture, a marking, whether those are legs).
   Note what views there are (front, side, back, details), the proportions (the height
   split into the head, the body, the legs; the widest point; where things attach), the
   colour regions with a hex for each (their lit tones, measured:
   `python tools/blender/model_requests.py colours ID PICTURE X0 Y0 X1 Y1`) and the few shapes
   that make the silhouette. Then tell the design from the drawing (below, "The design, not
   the drawing"), before building anything. The notes win over the pictures. What no picture
   shows, guess from the design's own logic, and say so in the reply. When the user has drawn the
   creature's bones over the pictures (`bones`), `python tools/blender/model_requests.py bones
   ID` draws them over each one: read the pictures by them (below, "The user's bones").
4. **Draw the lines** of each picture in `private/requests/ID/lines.json` (below, "Lines and
   profiles"): the design's outline, each part's outline, its middle line and its markings, in
   the picture's pixels, and where the picture was seen from (its `camera`); with the user's
   bones, what each of their lines is on (the picture's `bones`), each part's outline round its
   bone line. Then
   `python tools/blender/model_requests.py lines ID` draws them over the picture
   (`lines_PICTURE.png`, which the editor shows the user as it comes) and says how far each
   outline and the picture's silhouette overlap (crimson on the drawing where the picture goes
   past the outline, blue where the outline goes past the picture): look, and move the lines
   until they sit on the design, each outline on its picture's silhouette about 90% or more.
   Draw the design's lines, not the drawing's: no shine, shade or ink.
5. **Draw the front and the side** in `lines.json` from those lines, in the model's pose (a
   T-pose when asked): a picture from the front or the side gives its view as it is; one from
   another angle is drawn again, from the front and from the side, as a model sheet would show
   it. Accurate: every part and detail in both views; else the outline and the main parts.
   With the user's bones, each carried over too, as a line named `bone CHAIN`, the same joints
   in both views. `lines` again draws `profiles.png`, both views in cells, the request's
   height marked.
   **With `check_lines`, stop here**: `python tools/blender/model_requests.py wait ID --check`
   leaves it waiting for the user's check (a run from the editor's Start ends that way by
   itself, its last word kept as the request's reply), and say what was taken for the
   drawing, what was guessed and what's unsure in the lines. Once the user says go on (the
   window's **The lines look right: go on**, or in the session), keep `lines.json`, fix first
   what they say is off (`lines` again), and go on from step 6.
6. **Write `private/requests/ID/model.py`** with the kit (below), on the profiles:
   `job.profiles()` has each line in cells, as the model will stand, so each shape's size and
   place come from them (and `fit` gives it the request's height). Build it **in parts**
   (below, "Parts"), never as one lump fused and sculpted: list the parts first, as a comment
   at the top of `model.py` (what each is made of, which bones move it); then for each part,
   block its general shape in from a few shapes (`kit.part(name, [shapes], colours, bone=` or
   `bones=)`), and refine that part on its own: more shapes, `editing` and `extrude` (pull a
   snout out, inset a nostril, bevel a horn), `modifier` (subdivision to round it, bevel for
   crisp edges, solidify for a thin flap, simple deform to bend it), `sculpt` (a brow, a
   cheek, a knuckle). Creatures: a metaball `blob` or a few ellipsoids for each soft mass,
   `capsule`s and `tube`s for limbs and tails, `slab`s for ears, fins, crests and wings,
   `paint` volumes for markings (a belly, spots, a mask) on the part they're on, `eyes`.
   Props: each piece a part: `lathe` for anything round, `box` and `cylinder` for hard shapes,
   `cuts` for openings, `paint` for patterns, `light` for a lamp; a lid or a door that opens on
   a bone of its own. Big readable shapes: it's a base to refine, seen from the game's camera.
   Keep to the request's `shapes` (each shape, or a blob's element, counts as one; with
   `unlimited`, write the list of parts as a comment at the top of `model.py` first). A
   creature in `t_pose` is built in the T-pose of its body plan (above: limbs straight out, or
   its body straight) whatever the pictures' pose; `as_is` follows the pictures' pose; either
   way its mouth is closed (an open one is the "roar" action). Say in the reply which it got.
   The shapes are a start, not a limit: reshape whatever they can't follow (below,
   "Reshaping").
7. **Render it**: `python tools/blender/run_request.py ID`. It writes `preview_front.png`, `preview_side.png`,
   `preview_three_quarter.png` (Workbench, the material colours) and `compare.png` (the
   pictures on top, the renders under them, the same height, and a row of the lines laid on
   the model: below, "Lines and profiles"). Look at `compare.png`, and at a preview at full
   size when a detail matters.
8. **Compare and try again**, usually three to six times: how far each outline and the model
   overlap (the lines' row says, and where one goes past the other), then the silhouette (front and
   side outlines: proportions, the head against the body, where limbs attach), then the
   masses, then the colour regions and their borders, then the details that carry the
   character (the eyes' size and place, ears, a tail's curve). Edit `model.py`, render again.
   Stop when the renders read as the pictures' design at a glance; don't chase a sketch's
   wobbles. The renders are lit evenly on purpose: compare shapes and colours, not light (a
   picture's shine, shade or rim light is no difference to fix).
9. **Creatures: rig it** in `model.py` after `fit`: `rig(kit.parts(), chains)` from the
   model's own landmarks (in cells, the model on the origin facing -Y): `spine` (hips to
   chest), `neck` and `head`, a chain per limb named with its side (`leg.L`, `arm.R`), `tail`,
   `ear.L`...; with the user's bones, `job.chains()`: their chains, carried into the front and
   the side, in cells (below, "The user's bones"). Add the chains the parts need besides: `jaw`
   (the hinge to the chin), `tongue` (its root, middle and tip), and `eye.L`, `eye.R`, `lid.L`,
   `lid.R` when they move, each hanging from the bone it's on (`parents={"jaw": "head",
   "tongue": "jaw"}`). Each part is skinned as it asked (below, "Parts").
   Then `idle(arm)` (`blink=` with lids) and the actions the design calls for, the mouth
   opening and closing first (`action(arm, "roar", ...)`). Render again: `preview_idle.png`
   and `preview_roar.png` (each action's) show four moments (look for vertices torn by the
   wrong bone, the jaw turning the right way, the lips meeting at rest), and the rig's line
   says how each part was skinned and how many vertices the automatic weights missed (many:
   a bone is outside the part).
10. **Finish it**: `python tools/blender/run_request.py ID --finish`. The model is added with
   the add-on's `write_prop` (to `temporary/`, or `content/` when `mine`), as the prop
   named by the request's id (numbered when another prop has it; `finish(..., category="tree")`
   for a tree: its cells are its foot's, what stands below a character's height, the crown
   spreading over the cells round it, walked under, and it's stood on its trunk's middle); the engine's own reading
   of the file is printed (triangles, size, its light, its rig's joints and "idle"); the
   request is done, with its renders, the prop, the reply and the time it took. Put the
   reply in `model.py` (`finish(..., reply=...)`): what was guessed, what to look at.
11. **Check it**: `cargo run -p engine-content --bin content -- check`. The prop shows in the
   map editor's asset browser at once (with a TEMPORARY tag when it went there), and the
   Requests window shows the request done, with its renders and "Use it".
12. **Next**: back to 1 until none waits. Then tell the user what was made, with the replies.

A request left in progress (a session that stopped): the editor marks it failed after 31
minutes without news; Start again (or `model_requests.py wait ID`) puts it back in the queue.

## The design, not the drawing

A picture shows a design through a drawing. The model gets the design and none of the
drawing, so sort what each picture shows before building, whatever the style:

- **the design**: what the thing is, held in the hand: its shapes and proportions, its parts,
  its colours, markings and patterns, its eyes, holes and grooves, and its materials (shiny,
  matt, metal, glowing);
- **light and shade**: shine spots and streaks (highlights), reflections, rim light along an
  edge, shading and the shade side, shadows one part throws on another, glow halos and bloom,
  a tint from the light or from the background bouncing in;
- **the artist's ways**: outlines and ink lines, cel-shading bands, hatching, brush and paper
  texture, sparkles, motion and impact lines, blur, perspective (near parts drawn bigger, far
  ones smaller), a squash or stretch for the action;
- **around it**: the background, text and numbers (a "Lv. 10"), other creatures, a frame, the
  ground and its shadow on it.

How to tell them apart:

- The light comes from one side: shine sits on the side facing it on every part, shade on the
  other side and underneath. A light or dark patch that follows the light is light; one with
  its own shape and place, whichever way the light comes (and in every picture), is a marking.
- Light and shade change a colour's lightness (and an artist's shade often leans toward blue or
  purple); a marking is another colour with an edge of its own.
  `python tools/blender/model_requests.py colours ID PICTURE X0 Y0 X1 Y1` lists a box's main
  colours, the most covered first, each with its share, hex, name and where it is in the box
  (the colour along most of its edges is the background): one part's tones from light to dark
  are one colour in light and shade.
- A shiny surface (metal, chrome, pearl, glass, lacquer, a beetle's shell, wet eyes) is drawn
  with its reflections: dark and light blobs or bands curving round its shape, white glints in
  them, the same on each shiny part. That's its material, not spots.
- An outline runs along the silhouette and where parts overlap, about as wide everywhere; a
  line that ends inside a surface, or crosses it, can be a groove, a seam, a crack or a
  marking: zoom in, and look at the other pictures.
- Between several pictures, what changes is the drawing; what stays is the design.
- What the notes or the details say a thing is, it is.

What the drawing becomes:

- shine and reflections: a glossy material in the part's lit tone (`dict(kit.GLOSSY,
  color="#e9e4ee")`; `"metal": 1.0` and a little roughness for metal), never a white or dark
  patch. The game draws base colours and lights them itself (the kit, below: what the engine
  does with it), so a shine painted into a colour would be wrong under any other light;
- a glow: `"glow"` in the material (and a `light` for a lamp);
- shade, shadows, rim light, a tint: nothing; the part gets its lit tone;
- outlines, sketch lines, the background, text, other creatures: nothing;
- perspective: the design's real proportions, from the other views or its own logic.

Write the sorting at the top of `model.py`, as comments, before any shape: the design (each
part, its colour's hex, its material) and the drawing (what was left out, and why). The reply
says what was taken for the drawing, and anything unsure.

## Lines and profiles

Before any shape, the run draws what it will build, in two steps (recipe, steps 4 and 5), and
the drawings check the model after each render:

1. **Lines on the pictures.** In `lines.json`, each picture's lines in its own pixels (x to the
   right, y down, as `zoom --grid` marks them): the outline of the whole design (a closed line
   named `outline`: what the model is measured against), each part's outline (head, body, each
   segment, horn, ears, limbs, tail), the middle line where it's symmetric, the markings'
   edges, and marks for landmarks (an eye's middle, a horn's tip, where a limb joins). And the
   picture's `camera`, where it was seen from: `turn`, degrees round from the front (0 straight
   at its front; 90 from its left, so it faces your left; -90 from its right, so it faces your
   right; 180 from behind; a three-quarter view about 30 to 50 either way) and `tilt`, degrees
   above it (negative: below).
2. **The front and the side**, from those lines: `"front"` as seen straight from the front,
   `"side"` straight from its left, facing left (`"faces": "right"` when it's drawn facing
   right), both at about the pictures' scale, in pixels as the pictures are, each standing on
   its lowest point, in the model's pose. A picture from the front or the side gives its view
   straight. A picture from another angle doesn't: the design is drawn again from the front
   and from the side, as a model sheet would show it, each part's true length worked out from
   how much the picture foreshortens it, its depth from another picture or from its round
   shapes, what's hidden from the design's own logic. That's the expensive part, and the one
   that makes the model right: take the time it takes.

```json
{
  "pictures": {
    "beetle.png": {
      "view": "three-quarter, from its front right, a little above",
      "camera": { "turn": -45, "tilt": 15 },
      "lines": [
        { "name": "outline", "closed": true, "smooth": true, "points": [[118, 528], [112, 505], [125, 490]] },
        { "name": "head", "ellipse": [180, 450, 32, 34] },
        { "name": "horn", "closed": true, "points": [[205, 425], [240, 358], [258, 445]] },
        { "name": "eye", "ellipse": [215, 462, 12, 9], "angle": 20 },
        { "name": "horn tip", "mark": [242, 359] }
      ]
    }
  },
  "front": { "lines": [{ "name": "outline", "closed": true, "smooth": true, "points": [[170, 535], [165, 500], [195, 380]] }] },
  "side": { "lines": [{ "name": "outline", "closed": true, "smooth": true, "points": [[60, 535], [55, 500], [195, 380]] }] }
}
```

A line is `points` (`closed` for an outline, `smooth` for a curve through them), an `ellipse`
(`[x, y, half its width, half its height]`, turned by `angle` degrees) or a `mark` (`[x, y]`);
a name keeps its colour in every drawing. `python tools/blender/model_requests.py lines ID`
checks the file (saying all that's wrong at once) and draws:

- `lines_PICTURE.png`: the part of the picture the lines are in, enlarged, the lines over it,
  its pixels marked along the edges. The editor shows these to the user as they come (the
  **Lines** row): they're how the user sees that the run read the pictures right. Move the
  lines until each sits on the design; `zoom ID PICTURE X0 Y0 X1 Y1 --grid --lines` shows a
  part closer, the picture's pixels marked. Its caption (and `lines`' line for the picture)
  says how far the outline and the picture's own silhouette overlap and where one goes past
  the other (as for the renders, below), and the drawing shows where, tinted under the lines:
  crimson where the silhouette goes past the outline (the line to move out), blue where the
  outline goes past the silhouette (to move in), the caption's squares the key; where they
  part by a pixel or two it isn't tinted. The silhouette is every pixel in the outline's box,
  with room round it, that isn't the background: the colour along most of the box's edges (as
  `colours` finds it), reached from them, and a darker tone of it right under the creature
  (its shadow on the ground); in a picture with see-through edges, every pixel that isn't
  see-through. Move the lines until each outline overlaps its picture's silhouette about 90%
  or more. Another creature, text or anything else in that box that isn't the background
  counts as silhouette too: keep the outline tight on the design, never stretched over it,
  and say so in the reply when it keeps the overlap down. A busy background (the box's edges
  not mostly one colour) can't be scored, and the caption says so: judge that outline by eye.
  Trust your eyes over the number too where the creature has no line round it and is the
  background's own colour (or, low down, a darker tone of it): the silhouette can't tell it
  from the background there;
- `profiles.png`: the front and the side in cells, the request's height a dashed line, the
  ground and the middle;
- what's missing: a picture with no lines, no outline or no camera, no front or side yet, a
  side facing the other way than it says.

In `model.py`, `job.profiles()` gives both views in cells, as the model will stand (`fit`
stands it the same way): `{"front": {name: [(x, z), ...]}, "side": {name: [(y, z), ...]}}`,
each the request's height tall from z 0 and centred across, the front's x to the right as seen
from the front (the model's left), the side's front toward -y; a name used twice gets " 2".
Size and place each shape from them: a part's height and width are its extents in the front,
its depth and where it sits front to back its extents in the side (`min` and `max` of its
points); a flat part's front outline is a `slab` as it is; a round part's `lathe` profile is
half its front outline.

After each render, `run_request.py` lays the lines on the model: `fit_front.png` and
`fit_side.png` (the front's and the side's lines on the straight renders, where they fall in
cells) and `match_PICTURE.png` (the picture's lines on the model seen from its camera, lined up
on the outline's box), each saying how far the outline and the model's silhouette overlap and
where one goes past the other; they're `compare.png`'s last row, and the editor shows them. A
close model overlaps about 85% or more in each. A T-pose model differs from a picture in
another pose where the poses differ (limbs, a curled body): judge those parts by the front and
the side.

## The user's bones

The user can draw a creature's bones over its pictures before Start (the editor's **Draw the
bones…**, above): request.json's `bones` has, for each picture they've drawn on, its lines in the
order drawn, each a part and its joints in the picture's pixels, from its root out (a hip, a
shoulder, the neck, a tail's base), each pair of joints a bone:

```json
"bones": {
  "front.png": [
    { "part": "leg", "points": [[320, 660], [300, 720], [300, 795]] },
    { "part": "misc", "name": "chest fluff", "points": [[330, 400], [380, 455]] }
  ]
}
```

(laid out over more lines in the file.) The parts are `leg`, `arm`, `torso`, `head`, `ear`,
`horn`, `tail`, `wing` and `misc`, a piece with no name in that list, which carries the `name`
the user gave it. They're the user's reading of the body: which parts there are, how many, where
they join and where they bend. Take it over your own, and say in the reply where you think
it's off.

`python tools/blender/model_requests.py bones ID` draws them over each picture
(`bones_PICTURE.png`, the editor shows it too: each line in its part's colour, its root ringed,
numbered and named as the editor names it, "leg 1", "leg 2", "chest fluff") and lists each
line's joints. Work out which part of the creature in the picture each line is on (which leg,
of which side, front or back; which ear; what each named piece is) and say it in `lines.json`,
the picture's `bones`: one for each of the user's lines, in their order, what it's on and the
rig's chain it becomes:

```json
"front.png": {
  "view": "front", "camera": { "turn": 0, "tilt": 0 },
  "bones": [{ "is": "its right leg", "chain": "leg.R" }, { "is": "the fluff on its chest", "chain": "fluff" }],
  "lines": [...]
}
```

The chains are the kit's names (step 9): `spine` (or `torso`), `neck`, `head`, a side for
each pair (`leg.L`, `arm.R`, `ear.L`, `wing.R`), `tail`, `horn`; a named piece by its name.
`lines` then draws `bones_PICTURE.png` again with what each line was taken for under it (for
the user to check, with "Let me check the lines first"), and says what's not said yet.

Draw the lines with them: each part's outline round its bone line, its joints as marks where
they help. In the front and the side, carry each bone line over as a line named `bone CHAIN`
(`bone leg.L`, `bone tail`), the same joints in both views, in the model's pose (straightened
in a T-pose). `lines` lists the chains it finds in both and says what's wrong (a chain with
other joints in the front than in the side; a `.L` or `.R` chain only in the side, which then
stands in the middle). In `model.py`, `job.chains()` makes them the rig's chains in cells as
the model stands (each joint's x from the front, its y from the side, its height from both),
the body's chain first: `kit.rig(body, job.chains())`.

## Parts

A model is made of parts, as a modeller makes it for a rigger: each part its own mesh, its own
shapes, refined on its own, and moved by the bones it should be (`kit.part`: `bone=` the one
bone it follows whole, `bones=` the bones and chains that bend it, the only ones its automatic
weights may use). A creature's:

- **the body**: torso and neck (a `blob`, or a few ellipsoids remeshed together), bent by the
  spine and the neck (`bones=["spine", "neck"]`);
- **the head** with the upper jaw: skull, snout, the upper lip, the palate inside, following
  the head whole (`bone="head"`); the upper teeth a crisp part on the head too (`remesh=False`);
- **the lower jaw**: the lower lip, the floor of the mouth and its teeth (a part, or two on the
  same bone), on a `jaw` bone from the hinge (under the ear, at the back of the mouth) to the
  chin, hanging from the head. **The mouth is built closed**: the lips meet, the teeth inside,
  so opening it is the jaw turning, and closing it again looks right. A picture of an open
  mouth (a scream, a bite) becomes the `roar` action, not the rest pose. The mouth's inside
  (gums, palate, its floor) has its colour, so it reads when the jaw opens;
- **the tongue**: its own part, lying in the lower jaw, on a `tongue` chain of two or three
  bones hanging from the jaw (`bones=["tongue"]`), so it moves by itself: curls, licks, lolls;
- **the eyes**: `kit.eyes(head, centres, radius, separate=True)` makes them parts (`eye.L` on
  the model's left, `eye.R`), on the head, or on `eye.L` and `eye.R` bones that look about;
  **lids**, when the design has them, parts on `lid.L` and `lid.R` bones, so it blinks
  (`idle(arm, blink=(degrees about X, Y, Z))`);
- **ears, horns, crests, antennae**: a part each, on their own chains (`ear.L`) or the head;
- **limbs**: a part each, with its hand or foot, toes and claws (crisp parts on the last bone),
  bent by its own chain and the bone it hangs from (`bones=["leg.L", "spine"]`), so its root
  goes with the body;
- **the tail, wings, fins, flaps**: a part each, on their chains.

Parts meet by overlapping: a limb's root sinks into the body, the neck into the head, as on any
game model; the weights move them together. Shapes that only touch make a lumpy part: give them
real overlap, or remesh the part (the default). The budget is the whole model's: about 6000
triangles smooth (the head and the body most of it), 800 low-poly, 20000 or more accurate; a
part takes `faces=` (1500 by default, 300 low-poly).

A prop's parts are its pieces (a lantern's plinth, post, chamber and roof; a chest's box, lid
and lock), crisp ones as they are (`remesh=False`), each refined on its own; one that moves (a
lid, a door, a wheel) on a bone of its own, rigged like a creature's jaw.

## The wind

The game's wind sways what's painted for it, and nothing else: a colour layer named "wind",
its R how far a point sways (0 at its root: a stem's foot, where hair grows; 1 at the tips),
its G how much it flutters (leaves, petals, fur, a cloth's edge), its B its phase (when it
sways). The game adds how high the point stands, so taller things bend further, and where it
is, so pieces flutter each a little in their own time. It moves every point of one place the
same (a leaf made thick, its faces and its rim, stays whole), so what's joined must be painted
the same where it's joined.

**A plant is one thing in the wind.** Its leaves touch and overlap: in one phase they lean
together, in different ones they lean through each other. Paint a plant (a bush, a rosette, a
mound, a creeper, a tuft of broad leaves, a tree's crown and its leaves) in one call with all
its moving parts: `kit.plant_wind([leaves, dark_leaves, stalks], root=(0, 0, 0), sway=,
flutter=, stems=[stalks])` (`root` its foot; for a tree its trunk's foot). One phase for all of
it; the sway rising over the whole plant from its foot, so a leaf and the stalk it grows on
bend as one; each leaf (each loose piece) fluttering from its own foot (its point nearest the
root), so it stays on its stalk; `stems` sway but never flutter. Never a plant's parts one by
one, each in a `kit.wind` call of its own: each measured on its own and in its own time, a
leaf parts from its stalk and the leaves cross. A variant: its own call (`seed=` another, its
own time).

What moves apart, piece by piece, is `kit.wind(part, sway=, flutter=, root=)`, each loose piece
in its own time: hair, a mane's locks, a cloth, a tail's tuft, a flag, flowers on long stems of
their own. `kit.fur` paints its strands itself (rooted, swaying and fluttering to their tips,
each in its own time). A creature's body, head and limbs stay unpainted: the rig moves them,
the wind doesn't. A prop's: plants, grass, a banner; never a rock, a wall, a plant's crown it
grows from, its soil or its stones.

The rig and the actions, in `model.py` after `fit`:

```python
chains = job.chains()                       # the user's bones (or the model's own landmarks)
chains["jaw"] = [hinge, chin]               # in cells, the model as it stands
chains["tongue"] = [root, middle, tip]
arm = kit.rig(kit.parts(), chains, parents={"jaw": "head", "tongue": "jaw"})
kit.idle(arm)
kit.action(arm, "roar", {"jaw": [(0, (0, 0, 0)), (0.25, (32, 0, 0)), (0.8, (32, 0, 0)), (1, (0, 0, 0))],
                         "head": [(0, (0, 0, 0)), (0.25, (-10, 0, 0)), (1, (0, 0, 0))]}, seconds=1.4)
```

`preview_roar.png` shows the roar in four moments: the mouth closed, opening, open, closed; if
the jaw goes up into the head, its angle wants the other sign. Every action is in the file by
its name, for the game to play (the idle loops; a timeline plays the others).

## Reshaping: any tool

The kit's shapes and `part` make each part's clean base fast; whatever they can't follow,
reshape with the tool that does it best, for any style (the more so for accurate):

- **by hand**: `with kit.editing(obj) as bm:` gives the mesh as a bmesh: extrude faces
  (`kit.extrude(bm, faces, move)`: bmesh's own extrude leaves the old faces inside the mesh),
  inset, bevel, subdivide, bridge, delete, move faces, edges and vertices (`bmesh.ops.translate`,
  `rotate`, `scale`), on a shape before it's a part or on the part;
- **modifiers**: `kit.modifier(obj, "SUBSURF", levels=2)`: any of Blender's modifiers, applied
  for good: bevel, solidify, simple deform (bend, twist, taper), displace, cast, smooth,
  mirror, array, lattice, shrinkwrap, weld...;
- **sculpting**: `kit.sculpt(part, brush, at, radius, strength, direction)`: the grab,
  inflate, smooth, flatten, pinch and crease brushes, scripted, with a smooth falloff, limited
  to a colour region if asked: best on a remeshed part, where the mesh is dense; it refines a
  part's shape, it doesn't make one;
- **curves**: `kit.tube(points, radius, region, radius_end)` for horns, tails, tentacles,
  antennae and handles that bend;
- anything else in Blender's Python (`bpy`, `bmesh`, `mathutils`, numpy).

Mind the order: for each part its shapes, then `part` (it remeshes them into one surface,
softening edges at its voxel size: a finer `voxel` keeps them crisper; `remesh=False` keeps
the shapes as they are), then its refining (edits, modifiers, sculpting), then the eyes,
`fit` (every part together), `fur` (and `fit` again), then the rig and the actions. Refine
before the rig, never after: its weights are made for the parts as they are then.

## Accurate

`"style": "accurate"` asks for the closest match to the pictures, whatever it takes, and the
request's `details` say what matters most. Measure the proportions off the pictures (heights
in heads, widths against heights), model every part, marking and colour they show, and use the
most fitting tool for each part rather than the quickest:

- **fur and hair**: `kit.fur(part, region, ...)`, strands grown on a colour region as
  Blender's hair grows them, made mesh (glTF and the game have no particle hair), painted for
  the wind: thousands of strands for a coat (`count`), their `length`, `thickness` and
  `droop`; then `fit` again so it stands its height with its fur on, then the rig. Short
  dense fur reads better than long sparse fur, and each strand is 6 faces: mind the budget;
- **details**: more parts and shapes, `cuts` for openings and grooves, `slab`s for thin flat
  parts (claws, fins, leaves, feathers), a finer remesh (`voxel=size/180`) and a bigger budget
  for each `part` (20000 triangles or more in all);
- **patterns**: `paint` volumes for each marking, one per patch, shaped to the pictures;
- anything else Blender does that ends as meshes with materials (the game draws triangles,
  base colours and base colour textures): a texture painted from the pictures, displacement,
  subdivision applied on a part.

It may take a long time, and that's fine; the editor's bar runs on a longer estimate, and the
run keeps going while Claude writes a long script.

## Grass

A request of the kind `grass` is a patch of grass (or of any low growth: clover, moss, a
flowerbed) made to tile: laid in cells side by side, its copies join without a gap. So it fills
its cells (`cells`, 1 x 1 unless the request says; `job.cells`) to the edges, exactly:

- fit it with `kit.fit_cells(parts, job.cells, job.height)` in place of `kit.fit`: scaled each
  way so its bounds are its cells exactly (Blender's X and Y from -0.5 to 0.5 for one cell),
  `job.height` tall, standing on the origin;
- its blades (or leaves) evenly dense right up to the borders: as many near an edge as in the
  middle, none poking past it (they'd make its footprint a cell bigger), no bare margin, no
  soil, base or stones showing at an edge (the neighbour's grass meets it there); clumps cut
  by an edge as if the next cell carried them on;
- the wind paints it (`kit.plant_wind`, rooted at the ground: blades sway from their foot);
- a grass prop: `kit.finish(..., category="grass")` (its default for the kind), never blocking
  the way. Tall ground can grow it too (a terrain's Grows here, in the editor), tiled: each whole
  over its cells, lined up with them, one copy to its cells across a whole field;
- light, then: at most 1,000 vertices a cell (`mr.GRASS_MOST`, the engine's own most: more and
  it doesn't grow), as the engine counts them in its .glb, where flat faces and seams split
  vertices. Fewer, simpler blades (a strip of 2 or 3 segments each, a leaf a few quads) rather
  than many fine ones; nothing nobody sees (no faces under the ground, no inner shells).

`kit.finish` says when a grass model doesn't fill its cells, and refuses one too heavy to grow
(counted in a .glb written to the run's folder first: nothing goes into the project).

## Environment

A request of the kind `environment` is an asset that dresses the outdoors: a rock, a bush, a
flowerbed, a log, a stump, a tree. Made as a prop is, with what the outdoors asks: its
category the one that fits (`tree`, `flora`, `rock`, `grass`, `decoration`) in `kit.finish`,
blocking the way when it's big enough to walk round (a rock, a tree, a log), not when it's
walked through (flowers, a low bush); what the wind moves painted for it ("The wind").

## Variants

A request's `variants` (grass and environment ask from the first run, any made one through
**Make variants**) is how many variants of it to make besides it: the same kind of thing, in the
same style, each a prop of its own, as `vary` says they differ (else each a little different,
as the same kind of thing differs in nature: its size, its shape, how its parts sit, its colours
a shade apart; never another kind of thing). Build the model from a function of its settings,
the model itself one call of it and each variant another:

```python
def bush(seed, spread=1.0, tall=1.0, tint=0.0):
    """The bush, from its settings: every part, as the model's own are made."""
    ...
    return made


files = []
for k, (seed, spread, tall, tint) in enumerate([(211, 1.15, 0.9, 0.04), (377, 0.9, 1.12, -0.03)], start=2):
    made = bush(seed, spread, tall, tint)
    kit.fit(made, job.height * tall)                  # (grass: kit.fit_cells)
    kit.plant_wind(made[:2], root=(0, 0, 0), seed=seed)
    files.append(kit.variant_preview(job, made, k))   # variant_2.png...
    kit.finish_variant(job, k, made, category="flora", blocks=False)
    kit.drop(made)
made = bush(5)                                        # the model itself
kit.fit(made, job.height)
kit.plant_wind(made[:2], root=(0, 0, 0))
files = kit.previews(job, kit.parts()) + files
kit.finish(job, kit.parts(), files, category="flora", blocks=False, reply="...")
```

Each variant is a prop of its own, its id the model's with `_variant_2`, `_variant_3`..., named
"Name (variant 2)"; the window shows its render with the model's, and lists them (Its
variants). Look at each beside the model and the pictures when you render.

## A set of floor tiles

A request of kind `floor` (the window's **floor set**) makes no single prop but a set of floor
tiles, which the game lays by wave function collapse (`world::floors`) wherever flooring goes:
a building's floors (the Build tool's Flooring) and the ground out of doors (the Ground tool's
Floors tab). Its pictures show a floor, a pavement or a tiling (old ruins, flagstones, marble,
an intricate wood floor) or a part of one. Its run reads the pictures (step 3) and then, with
no lines (a floor is seen from above, laid out):

- **How the pattern breaks into cells.** Each tile is a cell, a metre a side, and each of its
  four edges says what it is, a word, going round from the north (Blender's +Y), then east
  (+X), south and west: two tiles side by side fit where the edge between them is the same
  word on both. The few tiles that make most of the floor (plain stones, planks, a field of
  marble) have one word all round (`joint`) and the most weight; what goes over two cells or
  more (the halves or quarters of a big slab) meets along a word of its own (`long`), its
  other edges the common one; a border's tiles have their outer edges `rim` (a set with rims
  lays them all round a floor and round its holes, and never faces a rim toward another
  tile; without rims, only a word some tile has on all four edges faces out); a medallion's
  or an inlay's pieces have inner edges of words of their own, so they only meet each other.
  A tile that `turns` may be laid a quarter, a half or three quarters turned (clockwise seen
  from above), its weight shared among its turns. The list goes at the top of `model.py`:
  each tile, its edges, its weight, whether it turns, what it shows.
- **Each tile modelled where it lies:** a slab 0.04 to 0.06 thick on the origin (X and Y from
  -0.5 to 0.5, standing on Z 0), its top what's seen, its pattern running right to the edges
  where the next tile carries it on; light, as a floor is laid on many cells (about 400
  triangles a tile smooth, 100 low-poly, 1500 accurate; the request's shapes are its most
  detailed tile's). Then `kit.floor_tile(job, "Name", [objects], [north, east, south, west],
  weight=, turns=)` for each.
- **Laid out:** `files = kit.layout(job)` lays the set on 8 x 6 cells by the game's rules
  (`floor_wfc.py`: the engine's, in plain Python) and renders it from above (`layout.png`)
  and slanting (`layout_side.png`), printing each edge that doesn't fit (none should);
  `kit.compare(job, files)` sets them beside the pictures. Rendered (`run_request.py ID`)
  and changed until it reads as the picture's floor.
- **Into the project:** `kit.finish_floor_set(job, files, reply=...)`, then `--finish`: each
  tile a prop `ID_TILE` (named after the request and the tile, category `floor`, never in
  the way), kept where it was modelled (never moved to stand on the origin: a half slab stays
  on its half), its record's `floor` block the set and the tile's edges
  (`{"set": "ID", "edges": ["joint", "long", "joint", "joint"], "weight": 1.0, "turns": true}`);
  a tile of the set an earlier run made and this one didn't is taken away (its record and
  its model); the request's `prop` is the set's first tile. Its panel then says **Became the
  set**, with the tiles and their edges, and **Use it** opens the Ground tool's Floors tab
  with the set to lay.

Made again with the user's comments, it starts from the last run's `model.py` like any request
("Made again, with the user's comments"); a sheet of several floors splits like any request
("Process multiple"), each one a set of its own.

## The kit: `request_kit.py`

`model.py` starts with:

```python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools" / "blender"))
import request_kit as kit  # noqa: E402

job = kit.start(__file__)   # the request, the scene emptied, the request in progress
```

`job.record` is the request; `job.creature`, `job.floor` (a set of floor tiles), `job.grass` (and
`job.cells`), `job.environment`, `job.variants`, `job.smooth`, `job.height`, `job.pictures()`.
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

| Making it, part by part | |
|---|---|
| `part(name, shapes, colours, bone=, bones=, remesh=True, smooth=, voxel=, faces=, cuts=[...], paint=[...])` | one part of the model, its own mesh named `name`: its shapes remeshed into one clean surface as `fuse` does (about `faces` triangles: 1500 smooth, 300 low-poly), or with `remesh=False` joined as they are (crisp: teeth, claws, plates, a lid), each face its shape's region; `bone`: the bone it follows whole; `bones`: the bones and chains that bend it (the only ones its weights may use); neither: every bone, by bone heat |
| `parts()` | every part made so far, for `fit`, `rig`, `previews` and `finish` |
| `fuse(shapes, name, colours, smooth=, voxel=, faces=, cuts=[...], paint=[...])` | what `part` remeshes with: joined and voxel-remeshed into one clean watertight mesh, `cuts` carved out, each face given the material of the nearest shape's region (then of the `paint` volume it's in), decimated to about `faces` triangles (smooth 6000, low-poly 800) sparing the borders between regions, smooth (weighted normals) or flat shaded |
| `colours` | region to `"#rrggbb"` or `{"color": ..., "rough": 0.65, "metal": 0, "glow": 0, "specular": 0.5}`: the region's lit tone, its shine as roughness (above, "The design, not the drawing") |
| `eyes(head, [centres], radius, colour=, look=, separate=True)` | glossy eyes with a glint, seated on the head's surface: parts of their own (`eye.L`, `eye.R`, on the bones of their names, else the head); without `separate`, set into the part (group "eyes", which the rig gives the head) |
| `fur(part, region, count=, length=, thickness=, droop=)` | fur or hair on a colour region, as mesh strands the game draws, painted for the wind; `fit` again after it |
| `plant_wind(parts, root=(0, 0, 0), sway=0.35, flutter=0.5, stems=(), seed=1)` | a plant's moving parts painted for the wind as one plant ("The wind"): one time for all of it, swaying the further out over the whole plant, each leaf fluttering from its own foot, `stems` never fluttering |
| `wind(part, sway=1.0, flutter=0.0, root=None)` | painted for the wind ("The wind"): swaying up to `sway` and fluttering up to `flutter` the further from `root` (a point; by default its lowest), each loose piece in its own time: for what moves apart (hair, a cloth), never a plant's leaves |
| `tube(points, radius, region, radius_end=)` | a curved part through the points, round, tapering |
| `with editing(obj) as bm:` | the mesh as a bmesh to edit by hand (extrude, inset, bevel, move...), written back after |
| `extrude(bm, faces, move)` | in an editing block: faces pulled out with their sides, the old ones gone |
| `modifier(obj, kind, **settings)` | any of Blender's modifiers, applied |
| `sculpt(obj, brush, at, radius, strength=, direction=, region=, times=)` | grab, inflate, smooth, flatten, pinch or crease, with a smooth falloff |
| `fit([objects], height)` | scaled to `height` cells, standing on the origin |
| `fit_cells([objects], cells, height)` | grass ("Grass"): scaled each way so its bounds are exactly its `cells` (across X, deep Y) and `height` tall, standing on the origin: copies side by side join without a gap |
| `light(at, color, intensity, reach, kind="point")` | a lamp's light, which becomes the prop's (lit at night) |
| `rig(parts, {chain: [points]}, parents=)` | an armature, each part skinned as it asked (whole to its `bone`, bone heat among its `bones`, else among every bone); a chain hangs from the bone nearest its first point, or the chain or bone `parents` names |
| `idle(arm, seconds=2.0, amount=1.0, blink=)` | a looping "idle" action from the bones' names: spine breathes, neck and head nod and look, tail/ear/antenna sway, arms and fins swing, wings stir, the jaw barely moves, the eyes glance, the lids blink (`blink`: the degrees that close them), legs stay |
| `action(arm, name, {bone or chain: [(when, (x, y, z)), ...]}, seconds=, loop=)` | another action by its name ("roar", "bite", "lick"): turns in degrees at fractions of its length, eased between; in the file for the game to play, `preview_NAME.png` shows it |
| `job.profiles()` | the front and the side from `lines.json`, in cells as the model stands (above, "Lines and profiles") |
| `job.chains()` | the user's bones carried into the front and the side (`bone CHAIN` lines) as the rig's chains, in cells, the body's first (above, "The user's bones") |
| `previews(job, objects)`, `compare(job, files)` | the renders and the sheet (above); the model seen from each picture's `camera` too, and `views.json` for laying the lines on them |
| `finish(job, objects, files, reply=, category=, blocks=)` | with `-- --finish`: into the project, the request done (its category and blocking by its kind unless said: a character; grass, never in the way; else a decoration); the variants this run added listed |
| `variant_preview(job, objects, k)` | variant `k`'s render, seen as the model's three-quarter one: `variant_k.png`, for `finish`'s files ("Variants") |
| `finish_variant(job, k, objects, category=, blocks=)` | with `-- --finish`: variant `k` (2 on) into the project as a prop of its own, `<prop>_variant_k`, "Name (variant k)" |
| `drop([objects])` | out of the scene, their meshes with them (a variant once it's added) |
| A set of floor tiles | |
| `floor_tile(job, name, objects, [north, east, south, west], weight=1.0, turns=False)` | one tile of a `floor` request's set: its meshes, modelled a cell wide on the origin, and its edges' words; kept on the job (`job.tiles`) |
| `layout(job, cells=(8, 6))` | the set laid by the game's rules (`floor_wfc.py`), rendered from above and slanting (`layout.png`, `layout_side.png`), each edge that doesn't fit printed; returns the files |
| `finish_floor_set(job, files, reply=)` | with `-- --finish`: each tile a `floor` prop of the set, where it was modelled; a tile an earlier run made and this one didn't taken away; the request done |

The steps set the editor's bar by themselves: reading the pictures (5%), building (15%),
rendering (50%), comparing (60%), adding it (95%), done. The bar shows those, or the time
gone against the estimate when that's further along, and never fills before it's done.

What the engine does with it: draws the triangles and the materials' base colours, every part
skinned to the one rig, lights the prop's light, plays a rigged prop's "idle" in a loop, and
its other actions when a timeline asks for them by name. Roughness, glossy eyes and
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
Each piece is a part: the plinth, the post and the dish crisp as they are, the chamber
remeshed to carve its windows, the roof remeshed with its ball to paint the moss on.

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
colours = {"base": "#7b776d", "stone": "#b0ab9f", "light": {"color": "#ffd27a", "glow": 2.0, "rough": 0.5}, "moss": "#6f924a"}
smooth = job.smooth
kit.part("plinth", [kit.lathe([(0.0, 0.0), (0.40, 0.0), (0.40, 0.06), (0.37, 0.09), (0.29, 0.09), (0.27, 0.14), (0.0, 0.14)], "base")], colours, smooth=smooth, remesh=False)
kit.part("post", [kit.lathe([(0.0, 0.13), (0.11, 0.13), (0.11, 0.29), (0.135, 0.31), (0.11, 0.33), (0.11, 0.48), (0.0, 0.48)], "stone")], colours, smooth=smooth, remesh=False)
kit.part("dish", [kit.lathe([(0.0, 0.47), (0.12, 0.47), (0.17, 0.49), (0.28, 0.52), (0.28, 0.56), (0.0, 0.56)], "stone")], colours, smooth=smooth, remesh=False)
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

kit.part("chamber", [kit.lathe([(0.0, 0.555), (0.19, 0.555), (0.19, 0.805), (0.0, 0.805)], "stone")], colours, smooth=smooth, cuts=windows)
kit.part("roof", [kit.lathe(ROOF, "stone"), kit.cylinder((0, 0, 1.06), 0.03, 0.05, "stone"), kit.sphere((0, 0, 1.13), 0.05, "stone")], colours, smooth=smooth, paint=moss)
glow = kit.light((0, 0, 0.68), "#ffc46b", intensity=0.9, reach=5.0)
kit.fit(kit.parts() + [glow], job.height)

files = kit.previews(job, kit.parts())
kit.compare(job, files)
kit.finish(job, kit.parts() + [glow], files, reply="Made from the front, the 45-degree and the top views...")
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
done, and the engine reading the rig and the idle. Then a lizard in parts: each its own mesh,
crisp ones keeping their shapes' regions, the jaw and the teeth and the eyes on one bone each,
the tongue on its own chain, the legs and the tail on theirs, the jaw opening without the head
moving, the idle and a roar in the file, every part on one skin, and the engine reading them.
And a set of floor tiles: a flag and a long slab's half, laid out and rendered from above and
slanting with every edge fitting, added as floor props of the set where they were modelled
(the half on its half), and made again without the half, which goes.
