"""One model request made by Claude Code, unattended, with its log: what the map editor's
Requests window starts with its Start button (and stops with Cancel), or by hand:

    python tools/blender/run_claude.py ID [--model opus|sonnet|haiku] [--go-on] [--note "what's off in the lines"]
    python tools/blender/run_claude.py ID --again [--note "what to change"]
    python tools/blender/run_claude.py ID --variants
    python3 tools/blender/run_claude.py ID --prompt [--go-on] [--again] [--variants] [--note-file PATH]
    python3 tools/blender/run_claude.py ID --settle [the same] [--reply "what was made"]

A request whose pictures hold several drawings of the same kind of asset ("several", the
editor's "Process multiple": trees, bushes, rocks...) is split by its first run: Claude finds
each drawing and makes it a request of its own (model_requests.py split), which the editor's
"Make them all" then has made one after another; the request itself is done once split. When
the user has outlined the drawings to make ("outlines"), only those are made, each cut to its
outline, as tall and on as many cells as the user said; the rest are left out.

Claude Code (`claude`: CLAUDE names the program, else the PATH, else ~/.local/bin) gets the
recipe (requests.md) for that one request and only the tools it needs, set in the request's
claude-settings.json: reading the project, writing in the request's folder, Blender through
run_request.py, model_requests.py and the content check. Anything else is refused, never
asked (nobody is there to answer). Everything it does goes into the request's log.txt, a line
each, timed by the computer's clock: Claude reached and the model it runs, what was sent,
each message, tool call and answer, the end (the raw stream in run.jsonl). Meanwhile the
request is in progress; it ends done once Claude has added the model; failed when Claude ends
without it, stops with an error, or says nothing for 30 minutes (then it's stopped); waiting
again when a file named "stop" appears in its folder (the editor's Cancel).

A request whose pictures the user has drawn the creature's bones over (the editor's Bones window:
request.json's "bones") has Claude read the pictures by them, say what each line is on, and
rig the creature on them.

A request that checks its lines first ("Let me check the lines first": "check_lines") has
Claude stop once it has drawn the lines and the front and the side (profiles.png): the run
ends with the request waiting for the user's check ("lines ready: check them"). --go-on (the
editor's "The lines look right: go on") runs again from the model, on those lines; --note is
what the user says is off in them, which Claude fixes first (going on, or starting again).

A request made once can be made again with the user's comments on it (the editor's "Make it
again with these comments": request.json's "comments", or --note): --again has Claude start
from what the last run made (its model.py, its lines, its renders, what it said back) and
change what the comments say, keeping the rest. Once it's done, the comments join "earlier",
which the runs made again after that are told too.

A request of grass fills its cells to their edges (request.json's "cells"), so copies side by
side join without a gap, and in pieces the engine can thin further off (model_requests'
GRASS_MOST vertices a cell as made, GRASS_THINNEST at its thinnest: the engine's own). One whose
settings ask for variants ("variants": how many besides it,
"vary": how they differ; grass and environment) has them made with the model, each a prop of
its own. Any request made can have variants made from its model later (the editor's "Make
variants"): --variants has Claude start from what the last run made, keep the model itself, and
add the variants the request asks for, its style, notes and the rest as they are.

In the cloud (the web editor's runs, in a sandbox of Anthropic's on the user's own API key:
cloud_run.py), it's Claude there that follows the prompt, not Claude Code here: --prompt starts
the run as Start does (its checks, in progress from now, its log) and writes the prompt for it
to read (its checks with no cargo there), and --settle ends it as a run here ends, by what's on
disk (`--reply`: its one line on what it made).
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import model_requests as mr  # noqa: E402
import request_lines as rl  # noqa: E402

# seconds without a word from Claude: the run is stopped, the request failed
QUIET = 30 * 60
# about the most vertices a model drawn by the thousand has (requests.md, "Drawn by the thousand")
MANY_VERTICES = 300
# lines of a tool's answer kept in the log (run.jsonl has everything)
SHOWN = 30

# The prompt, in pieces: a whole run follows the recipe from reading the request to adding the
# model; a request that checks its lines first stops after the front and the side (LINES_RUN),
# and once the user has looked, --go-on goes on from the model (GO_ON_RUN).
HEAD = """You are modelling one request of the Battle Frontier map editor, unattended: nobody can
answer a question, so decide for yourself, and say what you guessed in the reply.

The request: "{name}", {what}, in private/requests/{id}/ (its request.json
and its pictures). Notes from the user: {notes}
{settings}

"""
READ = """- read private/requests/{id}/request.json and look at every picture in that folder (to see a
  part closer: python tools/blender/model_requests.py zoom {id} PICTURE X0 Y0 X1 Y1, in pixels
  or fractions of the picture, with no box it gives the size; then look at the zoom it writes);
"""
# what the user's ratings teach of the request's kind (model_requests.py lessons): the style its
# best rated models share, carried on; what its worst did, left out ({kind}: the request's)
LESSONS = """- what the user's ratings teach of {kind} models (requests.md, "What the ratings teach"): run
  python tools/blender/model_requests.py lessons {id}
  It lists the {kind} models the user rated best and worst (0 to 100: 100 is exactly as designed,
  as its description says), each with its notes, what the user said of it, what its run said,
  its renders and its model.py, and what the runs before learnt from them. Look at the best ones'
  renders and read their model.py: build this one with their DNA (how the parts are made and
  joined, the proportions, the level of detail, how the colours and materials are handled, the
  silhouette), so the game's {kind}s look made by one hand, this one still its own design. Look
  at the worst ones' renders, and leave out what they did. When the ratings show something the
  lessons don't say yet, write them again, whole and short (the style and the way of building,
  never one model's details):
  python tools/blender/model_requests.py learnt {id} --works "..." --fails "..."
"""
# the user has drawn the creature's bones over the pictures (the editor's Bones window): the run reads
# the pictures by them ({drawn}: on which pictures, how many lines)
BONES = """- the user has drawn the creature's bones over its pictures in the editor's Bones window, on
  {drawn}: request.json's "bones" has, for each picture, lines of joints in its pixels, each
  from its root (a hip, a shoulder, the neck, a tail's base) out, each one a part: leg, arm,
  torso, head, ear, horn, tail, wing, or misc, a piece the user gave a name of their own. Run
  python tools/blender/model_requests.py bones {id}
  and look at each bones_PICTURE.png: work out which part of the creature in the picture each
  line is on (which leg, of which side, front or back; which ear; what each named piece is).
  It's the user's reading of the body: take it over your own (the parts there are, how many,
  where they join and bend), and say in the reply where you think it's off. Say it in
  lines.json, each picture's "bones": one for each of the user's lines, in their order,
  {{"is": "its front left leg", "chain": "leg.L"}}, the chain being the one the rig gets
  (spine, neck, head, leg.L, arm.R, tail, ear.L, horn, wing.L...; a named piece by its name).
  Draw your lines with them: each part's outline round its bone line, a mark at each joint.
  In the front and the side, carry them over as lines named "bone CHAIN" ("bone leg.L"), the
  same joints in both views, in the model's pose: the creature is rigged on them;
"""
LINES = """- before building, tell the design from the drawing (requests.md, "The design, not the
  drawing"). The design is what the model gets: its shapes, proportions, parts, colours,
  markings, patterns and materials. The drawing is how the picture shows it, and the model gets
  none of it: light and shade (shine spots and streaks; reflections, which a shiny surface
  shows as dark and light blobs curving round it, with white glints; rim light, shading,
  shadows, glow halos, a tint from the light or the background), the artist's ways (outlines,
  ink lines, cel-shading bands, brush texture, sparkles, motion lines, perspective) and what's
  around it (the background, text, other creatures, its shadow on the ground). Shine and
  reflections make a glossy material, a glow a glowing one, never pale or dark patches; a
  part's colour is its lit tone, neither its shine nor its shade (python
  tools/blender/model_requests.py colours {id} PICTURE X0 Y0 X1 Y1 lists a box's main
  colours). What the notes or the details say a thing is, it is. Write both lists as comments
  at the top of model.py before any shape, and say in the reply what you took for the drawing;
- draw the design's lines over each picture (requests.md, "Lines and profiles"): write
  private/requests/{id}/lines.json with each picture's outline (a closed line named
  "outline"), each part's outline, its middle line and its markings, in the picture's pixels,
  and its camera (turn and tilt: where it was seen from). Then
  python tools/blender/model_requests.py lines {id}
  draws them over the picture (lines_PICTURE.png, which the user sees too) and says how far
  each outline and the picture's silhouette overlap (all in the outline's box, with room
  round it, that isn't the background or the shadow under the creature) and where one goes
  past the other, tinted on the drawing: crimson where the silhouette goes past the outline
  (move it out there), blue where the outline goes past the silhouette (move it in). Look at
  each, and move the lines until every line sits on the design and each outline overlaps its
  picture's silhouette about 90% or more. Another creature (or text) inside that box counts
  as silhouette too: keep the outline tight on the design, never stretched over it, and say
  so in the reply. A busy background can't be scored: judge that outline by eye. To see a
  part closer, its pixels marked:
  python tools/blender/model_requests.py zoom {id} PICTURE X0 Y0 X1 Y1 --grid --lines
  A lines.json from an earlier run may be there: check it against the pictures, keep what's
  right;
- then draw the front and the side in lines.json from those lines, in the model's pose, at
  the pictures' scale: a picture from the front or the side gives that view; a picture from
  another angle means drawing the design again, from the front and from the side, as a model
  sheet would show it, each part's true size worked out from how the picture foreshortens it.
  It's the long part, and the one that makes the model right. Run lines again and look at
  profiles.png (both views in cells, the request's height marked);
"""
MODEL = """- write private/requests/{id}/model.py with tools/blender/request_kit.py, built on the
  profiles: job.profiles() has each of their lines in cells, as the model will stand, so
  each shape's size and place come from them. Build it in parts, as a modeller makes a model
  for a rigger, never one lump fused and sculpted (requests.md, "Parts"): list the parts first
  at the top of model.py (what each is made of, which bones move it); block each part in from
  a few shapes with kit.part (its own mesh; bone= the bone it follows whole, bones= the bones
  that bend it). The kit is a start, not a limit: refine each part with the tool that does it
  best for the style: more shapes, kit.editing (bmesh by hand: extrude, inset, bevel, move
  faces, edges and vertices; kit.extrude pulls faces out), kit.modifier (any of Blender's
  modifiers: subdivision, bevel, solidify, simple deform...), kit.sculpt (grab, inflate,
  smooth, flatten, pinch, crease), kit.tube for curved parts, or any of Blender's Python.
  A creature: the body, the head with the upper jaw, the
  lower jaw, the tongue, the teeth, the eyes, each ear, horn, limb, wing, fin and the tail a
  part each. The mouth is built closed, the lips meeting: the lower jaw on a jaw bone, the
  tongue on a chain of its own hanging from it, so the mouth opens and closes and the tongue
  moves by itself; a mouth open in the pictures becomes the "roar" action, not the rest pose.
  A prop: each piece a part (a lid or a door that moves on a bone of its own). What the wind
  moves is painted for it (requests.md, "The wind"): kit.fur paints its strands; a plant (its
  leaves, stalks, petals) in ONE kit.plant_wind call with all its moving parts, stalks as stems;
  kit.wind for what moves apart (hair, a cloth, a tail's tuft); nothing else;
- render it: python tools/blender/run_request.py {id} (it lays the lines on the model too)
- compare private/requests/{id}/compare.png and the previews with the pictures (the shapes,
  proportions and colours, not the light: the previews are lit evenly, on purpose); its last
  row has the lines on the model and how far each outline and the model overlap (aim for
  about 85% or more), and where one goes past the other (a model posed otherwise than a
  picture can't match it where the poses differ: judge those parts by the front and the
  side). Change model.py and render again until it matches (usually three to six tries);
- a creature: rig it{rig}: every part on the bones that move it, the jaw's and the tongue's
  chains added (and the eyes' and the lids' when they move); give it its idle and its roar
  (kit.idle, kit.action), render again and look at preview_idle.png and preview_roar.png:
  nothing torn, the jaw turning the right way, the lips meeting at rest (requests.md, step 9);
{variants}- add it to the project: python tools/blender/run_request.py {id} --finish
- check: cargo run -q -p engine-content --bin content -- check
"""
# a request whose settings ask for variants: made with the model, each a prop of its own
# ({count}: how many; {vary}: how they differ; {last}: the last one's number)
VARIANT_STEP = """- its variants, {count} besides it, each a prop of its own (requests.md, "Variants"): {vary}
  model.py builds the model from a function of its settings (a seed, sizes, colours, how its
  parts sit), the model itself one call of it and each variant another, in the same style,
  with the same parts, paint and wind as the model. First each variant k from 2 to {last}:
  build it, fit it as the model is fitted, files += [kit.variant_preview(job, its parts, k)]
  (variant_k.png), kit.finish_variant(job, k, its parts, ...) (the category and settings the
  model's kit.finish gets), then kit.drop(its parts); then the model itself, its previews,
  and kit.finish(job, kit.parts(), files + the variants' renders, ...). When you render, look
  at each variant_k.png beside the model and the pictures;
"""
# a request a run made, its variants made from that model (the editor's "Make variants"): the
# model itself kept ({made}: the variants an earlier run made)
VARIED = """- the last run made it: private/requests/{id}/model.py built it (its parts listed at its top,
  with the design and the drawing sorted as comments; design.py too when there is one, which
  model.py runs first) on lines.json (drawn over the pictures in lines_PICTURE.png, the front
  and the side in profiles.png). Its renders are preview_*.png and compare.png, and it's in the
  game as the prop {prop}{made}. What it said back:
{reply}
  Read model.py, and look at the renders and the drawings, before changing anything;
- make its variants now, {count} besides it, each a prop of its own (requests.md, "Variants of a
  model made"): {vary}
  Keep the model itself as the last run made it, the same shapes and the same seed: if
  model.py builds it in one go, make that a function of its settings first, the model itself
  the same call as before; then the variants as "Variants" says (kit.variant_preview,
  kit.finish_variant and kit.drop for each, the model itself after them, kit.finish with every
  render). Variants an earlier run made are made again by these, numbered from 2 again;
- render it: python tools/blender/run_request.py {id}, and look at each variant_k.png beside
  the model and the pictures: each the same kind of thing, in the same style, as different as
  that says; change model.py and render again until they are;
- add them to the project: python tools/blender/run_request.py {id} --finish
- check: cargo run -q -p engine-content --bin content -- check
"""
# a request that checks its lines first: where its first run stops
STOP = """- once profiles.png is drawn and every line sits on the design, stop there: no shapes, no
  render. The user looks at your lines first (in the editor: lines_PICTURE.png and
  profiles.png), then says go on, maybe with a note on what's off, and another run builds the
  model on them. This run ends with the request waiting for that check: leave its status be.
"""
# where a run after the user's check starts: their note (or that they look right) in {checked}
CHECKED = """- the lines are drawn and the user has checked them: private/requests/{id}/lines.json,
  drawn over the pictures in lines_PICTURE.png and as the front and the side in profiles.png
  (look at them). Keep lines.json. {checked}
- model.py may already start with the design and the drawing sorted, as comments, from the run
  that made the lines (requests.md, "The design, not the drawing"): build on them. If it
  doesn't, sort them first, at its top, and say in the reply what you took for the drawing;
"""
RULES = """
Run commands with the Bash tool, exactly as written above. Write only inside
private/requests/{id}/. Don't use git, don't change anything else in the project (finishing
writes the prop itself), don't touch other requests. {end}"""
MADE = "End with one line saying what you made."

# ({noted}: what the user said is off in the lines an earlier run made, when they start again)
PROMPT = (HEAD + "Follow tools/blender/requests.md for this one request, from its step 3 (it's started already):\n"
          + READ + "{lessons}{bones}{noted}" + LINES + MODEL + RULES.replace("{end}", MADE))
LINES_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, from its step 3 to its step 5 (it's\n"
             "started already), then stop: the user checks the lines before the model is built (the request\n"
             "says \"Let me check the lines first\"):\n"
             + READ + "{bones}{noted}" + LINES + STOP + RULES.replace("{end}", "End with a few lines for the user, who\n"
                                                           "checks the lines next: what you took for the drawing, what you guessed, and anything in the\n"
                                                           "lines you're unsure of."))
GO_ON_RUN = (HEAD + "Follow tools/blender/requests.md for this one request from its step 6, the model (it's started\n"
             "already):\n" + READ + "{lessons}" + CHECKED + MODEL + RULES.replace("{end}", MADE))
# a request made once, made again with the user's comments on it ({said}), from what the last
# run made ({reply}: what it said back; {earlier}: what the runs made again before were told)
AGAIN = """- the last run made it: private/requests/{id}/model.py built it (its parts listed at its top,
  with the design and the drawing sorted as comments; design.py too when there is one, which
  model.py runs first) on lines.json (drawn over the pictures in lines_PICTURE.png, the front
  and the side in profiles.png). Its renders are preview_*.png and compare.png, and it's in the
  game as the prop {prop}. What it said back:
{reply}
  Read model.py, and look at the renders and the drawings, before changing anything;
{earlier}- the user has looked at that model, and says for this run:
{said}
  Make those changes, and what follows from them, keeping the rest as the last run made it:
  start from its model.py, never over from nothing. A change to the shapes or the proportions
  the lines set goes into lines.json first (the pictures' lines, and the front and the side
  with them; run python tools/blender/model_requests.py lines {id} again);
"""
# a request whose pictures hold several drawings of the same kind of asset: split first
SPLIT = """- its pictures hold several drawings of the same kind of asset (the request says "Process
  multiple": one asset per drawing), each to be made a request of its own, modelled one after
  another: find each drawing of it (all of each: a tree's crown and its trunk's foot, a bush
  and the stems it stands on), and give each a short name saying what it is ("Tall pine",
  "Round bush", "Flat rock"), how tall it stands in cells (the request's {height} is the
  tallest's; each other by how it compares with that one in the picture, as they stand in
  it), and what's only its (its colours, a marking, a part the others don't have);
- for each, in the pictures' order (left to right, then top to bottom):
  python tools/blender/model_requests.py split {id} PICTURE X0 Y0 X1 Y1 --name "NAME" --height H --notes "what's only its"
  the box a little bigger than the drawing, all of it in and as little of the next one as can
  be, in the picture's pixels (zoom with no box gives its size); then look at the picture it
  writes for the new request, to be sure the whole drawing is in it (split it again, a better
  box, if not: the request made wrong is said so in your reply);
- that's all: no lines, no model (each new request is made by a run of its own). The same
  drawing twice is one request; a drawing of something else (a figure, a building, a word)
  none;
"""
SPLIT_RUN = (HEAD + "This one request is split first (tools/blender/requests.md, \"Process multiple\"; it's started already):\n"
             + READ + SPLIT + RULES.replace("{end}", "End with a few lines for the user: the requests you made, each one's name and height, and\n"
                                                    "anything on the pictures you left out, and why."))
# split, the user having outlined the drawings to make ({outlined}: how many on which pictures;
# {count}: how many in all): those only, each cut to its outline
OUTLINED = """- its pictures hold several drawings of the same kind of asset (the request says "Process
  multiple": one asset per drawing), and the user has outlined the ones to make: {outlined}
  (request.json's "outlines", each a closed line round a drawing in its picture's pixels,
  with the height and the cells the user wants it). Make a request of each outlined drawing,
  and of nothing else on the pictures: what isn't outlined is left out. Run
  python tools/blender/model_requests.py outlines {id}
  and look at each outlines_PICTURE.png (each outline numbered in its colour), then give each
  a short name saying what it is ("Tall pine", "Round bush", "Flat rock") and what's only its
  (its colours, a marking, a part the others don't have);
- for each, by its number, 1 to {count}:
  python tools/blender/model_requests.py split {id} --outline K --name "NAME" --notes "what's only its"
  which cuts its picture to its outline (what's outside it left out) and gives it the height
  and the cells the user set; then look at the picture it writes for the new request, to be
  sure it's the drawing outlined;
- that's all: no lines, no model (each new request is made by a run of its own);
"""
OUTLINED_RUN = (HEAD + "This one request is split first, into the drawings the user outlined (tools/blender/requests.md,\n"
                "\"Process multiple\"; it's started already):\n" + READ + OUTLINED
                + RULES.replace("{end}", "End with a few lines for the user: the requests you made, each one's number and name."))
# a set of floor tiles: the pattern on the pictures broken into cells, each a tile whose edges
# say what goes on into the next, laid by wave function collapse
FLOOR = """- the pictures show a floor, a pavement or a tiling (or a part of one): make it a set of floor
  tiles, which the game lays by wave function collapse (requests.md, "A set of floor tiles").
  Each tile is a cell (a metre a side): a slab 0.04 to 0.06 thick, modelled on the origin
  (Blender's X and Y from -0.5 to 0.5, standing on Z 0), its top what's seen. Each of its four
  edges says what it is, a word (north +Y, east +X, south, west): two tiles side by side fit
  where the edge between them is the same word on both. First work out how the pattern breaks
  into cells: the few tiles that make most of it (plain stones, planks, a field of marble:
  edges like "joint", weighted to be laid the most); what goes over two cells or more (the
  halves or quarters of a big slab, meeting along a word of their own); a border (its outer
  edges "rim": a set with rims lays them all round a floor, and round its holes) and its
  corners; a medallion, an inlay or a motif (its pieces' inner edges words of their own, so
  they only meet each other); which may be turned. Write the list at the top of model.py:
  each tile, its edges north east south west, its weight, whether it turns, what it shows;
- build each in model.py with tools/blender/request_kit.py: kit.box and kit.slab for the
  stones, planks and joints, kit.part with remesh=False to join them crisp (its colours, and
  its paint volumes for veins, inlays and markings), kit.editing for bevels and worn edges, or
  for what's too fine to model a material whose Base Color is an image painted from the
  picture; the colours measured with
  python tools/blender/model_requests.py colours {id} PICTURE X0 Y0 X1 Y1; its pattern
  running right to the edges where the next tile carries it on, then
  kit.floor_tile(job, "Name", [objects], [north, east, south, west], weight=, turns=) for each;
- lay it out: files = kit.layout(job) lays the set on 8 x 6 cells by the game's rules and
  renders it from above (layout.png) and slanting (layout_side.png), printing every edge that
  doesn't fit (none should); then kit.compare(job, files);
- render it: python tools/blender/run_request.py {id}, and look at layout.png and compare.png
  beside the pictures: the pattern, its proportions, the colours, the joints, how it runs on
  across cells; change model.py and render again until it reads as the picture's floor
  (usually three to six tries);
- add it to the project: kit.finish_floor_set(job, files, reply=...) at the end of model.py,
  then python tools/blender/run_request.py {id} --finish (each tile a prop of category "floor",
  of the set named after the request);
- check: cargo run -q -p engine-content --bin content -- check
"""
FLOOR_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, from its step 3 (it's started already):\n"
             "it's a set of floor tiles (\"A set of floor tiles\"):\n" + READ + "{lessons}{noted}" + FLOOR + RULES.replace("{end}", MADE))
AGAIN_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, made again from what the last run made,\n"
             "with the user's comments on it (requests.md, \"Made again, with the user's comments\"; it's started\n"
             "already):\n" + READ + AGAIN
             + MODEL.replace("- write private/requests/{id}/model.py with", "- change private/requests/{id}/model.py, the last run's, with", 1)
             + RULES.replace("{end}", "End with a few lines for the user: what you changed for each of their comments, and what\n"
                                      "you left as it was."))
VARIANTS_RUN = (HEAD + "Follow tools/blender/requests.md for this one request: variants of the model made (requests.md,\n"
                "\"Variants of a model made\"; it's started already):\n" + READ + VARIED
                + RULES.replace("{end}", "End with a few lines for the user: the variants you made, and how each differs from the\n"
                                         "model."))
# a set of floor tiles made again: the same, its tiles changed in the last run's model.py
AGAIN_FLOOR_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, a set of floor tiles, made again from what\n"
                   "the last run made, with the user's comments on it (requests.md, \"Made again, with the user's comments\"\n"
                   "and \"A set of floor tiles\"; it's started already):\n" + READ + AGAIN
                   + FLOOR.replace("- build each in model.py with", "- change the last run's model.py: each tile built with", 1)
                   + RULES.replace("{end}", "End with a few lines for the user: what you changed for each of their comments, and what\n"
                                            "you left as it was."))


def find_claude():
    env = os.environ.get("CLAUDE")
    if env and Path(env).is_file():
        return env
    for name in ("claude", "claude.exe", "claude.cmd"):
        found = shutil.which(name)
        if found:
            return found
    home = Path.home()
    for p in (home / ".local" / "bin" / "claude.exe", home / ".local" / "bin" / "claude", home / ".claude" / "local" / "claude",
              Path(os.environ.get("APPDATA", str(home))) / "npm" / "claude.cmd"):
        if p.is_file():
            return str(p)
    return None


def program(claude):
    """How to start it: a .cmd through cmd, a .py (the tests' stand-in) through Python."""
    low = claude.lower()
    if low.endswith(".py"):
        return [sys.executable, claude]
    if low.endswith((".cmd", ".bat")):
        return ["cmd", "/c", claude]
    return [claude]


# the API keys an environment can hold, which Claude Code goes on before its own sign-in
KEYS = ("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN")


def signed_in(claude):
    """What the run goes on: the user's own Claude sign-in, as `claude auth status` tells it (the
    environment's keys left out, so it tells Claude Code's own). A Claude subscription is routed
    to the run: an API key in the environment would bill the key instead, so it's left out then;
    an Anthropic Console sign-in likewise; without either, an API key the environment holds. The
    environment to run Claude Code in and what it's on, or None and why not."""
    plain = {k: v for k, v in os.environ.items() if k not in KEYS}
    try:
        asked = subprocess.run(program(claude) + ["auth", "status"], capture_output=True, text=True, timeout=60, env=plain,
                               stdin=subprocess.DEVNULL)
        status = json.loads(asked.stdout or "null")
    except (OSError, subprocess.TimeoutExpired, ValueError):
        status = None
    if not isinstance(status, dict):
        # an older Claude Code that doesn't say: the run finds out
        return dict(os.environ), "Claude Code's sign-in (it didn't say which)"
    if status.get("loggedIn") and status.get("authMethod") == "claude.ai":
        plan = str(status.get("subscriptionType") or "").strip()
        return plain, "your Claude subscription" + (f" ({plan.title()})" if plan else "") + ", Claude Code's own sign-in"
    if status.get("loggedIn"):
        return plain, f"Claude Code's own sign-in ({status.get('authMethod')})"
    key = next((k for k in KEYS if os.environ.get(k)), None)
    if key:
        return dict(os.environ), f"the API key in {key}"
    return None, "nothing: Claude Code isn't signed in"


def allowed(rid):
    """What the run may do without asking; everything else is refused."""
    here = f"private/requests/{rid}"
    return [
        "Read", "Glob", "Grep",
        f"Write({here}/**)", f"Edit({here}/**)",
        f"Bash(python tools/blender/run_request.py {rid})", f"Bash(python tools/blender/run_request.py {rid} *)",
        "Bash(python tools/blender/model_requests.py *)",
        "Bash(cargo run -q -p engine-content --bin content -- check)",
        "Bash(ls)", "Bash(ls *)",
    ]


STYLES = {
    "smooth": "Style: smooth: rounded shapes, smooth shaded (parts with smooth=True, about 6000 triangles in all).",
    "low_poly": "Style: low-poly: few faces, flat shaded (parts with smooth=False, about 800 triangles in all).",
    "accurate": ("Style: accurate: make it match the pictures as closely as you can, whatever that takes: every part, marking and colour "
                 "of the design (none of the drawing's light, shade or lines), the proportions measured off the pictures. Use the most fitting tool for each part (requests.md, \"Accurate\"): kit.fur for fur "
                 "and hair, kit.sculpt and kit.editing for what the shapes can't do, kit.tube for curved parts, painted colours for patterns, more shapes, "
                 "cuts and slabs for the details, a finer voxel and a bigger budget for each part "
                 "(20000 triangles or more in all). It may take a long time: that's fine."),
}
LINES = {
    "smooth": "Lines: the outline and the main parts, over the pictures and in the front and the side.",
    "low_poly": "Lines: the outline and the main parts, over the pictures and in the front and the side.",
    "accurate": ("Lines: every part, marking and detail, each where the pictures put it, over the pictures and in the front and "
                 "the side too: the model is only as close as these are."),
}
# a set of floor tiles: laid on many cells, so each tile light
FLOOR_STYLES = {
    "smooth": "Style: smooth: softly rounded edges, smooth shaded; each tile light, as a floor is laid on many cells (about 400 triangles a tile).",
    "low_poly": "Style: low-poly: few faces, flat shaded (about 100 triangles a tile).",
    "accurate": ("Style: accurate: the pattern as close to the pictures as it can be: every stone, plank, joint, vein and inlay where the "
                 "pictures put it, the colours measured off them (none of the drawing's light or shade), an image texture painted from the "
                 "picture for what's too fine to model; each tile still light, as a floor is laid on many cells (about 1500 triangles a tile)."),
}
# a flower, made finely, part by part (requests.md, "A flower"): how it's made, the kit's own;
# what it's made of, its kind's instructions (request_kinds.json)
FLOWER = ("Flower: made finely, part by part (requests.md, \"A flower\"): its stem, its leaves, its petals and its heart, as its "
          "kind's instructions say. kit.finish's category \"flower\", never blocking the way: a meadow can grow it among its grass.")
# a rock, made finely on about its footprint, as tall as its pictures' proportions (requests.md,
# "A rock"; {w} x {d}: its cells)
ROCK = ("Rock: made finely, on about {w} x {d} cells (Blender's X and Y), as the user asks: its footprint, not its height "
        "(requests.md, \"A rock\"). Its height is the pictures' proportions': draw the front {w} cells wide and the side {d} long "
        "in their proportions, and python tools/blender/model_requests.py lines works out how tall that makes it and writes it in "
        "the request (profiles.png stands it so; job.height is it then). Fit it with kit.fit_footprint(parts, job.cells) in place of "
        "kit.fit: about its cells, its proportions kept, not exactly. kit.finish's category \"rock\", blocking the way.")
# a model drawn by the thousand: as light as it can be (requests.md, "Drawn by the thousand")
MANY = ("Drawn by the thousand: it's strewn over whole fields with the grass and the flowers (meadows, procedural ground), so make it "
        "as light as it can be while it reads right a few cells off (requests.md, \"Drawn by the thousand\"): the fewest vertices that "
        f"keep its silhouette (about {MANY_VERTICES} in all, as the engine counts them in its .glb: flat faces and seams split vertices), "
        "no subdivision, no faces nobody sees (under the ground, inside), flat colours over textures, few materials, no fur; each loose "
        "piece (a leaf, a petal, a blade) a piece of its own, so the engine leaves pieces out further off.")
# the game's look, or not (the prop's toon tag)
TOON = ("Toon: it's drawn with the game's toon look (light in steps, ink lines round it and along its sharp folds; requests.md, "
        "\"The toon look\"): clear shapes and flat colour regions read best, fine surface detail the light's steps would band left out. "
        "The prop is added toon.")
# a tree (requests.md, "A tree"): how it's made, the kit's own and the engine's; its trunk, its
# foliage, its colours and its wind, its kind's instructions (request_kinds.json)
TREE = ("Tree (requests.md, \"A tree\", \"The wind\"): its trunk, its foliage and how the wind moves it as its kind's instructions "
        "say. Its leaves are geometry: no transparency, the engine draws no cut-out textures and no camera-facing billboards for a "
        "model's parts. kit.finish's category \"tree\", blocking the way, its footprint the trunk's cells (the canopy may reach past them).")
# the window's kind's instructions (request_kinds.json): a line each, the user's
INSTRUCTIONS = "Its kind's instructions (the user's, a feature each: follow every one):"
POSES = {
    "t_pose": ("Pose: T-pose, whatever pose the pictures show (the rest pose a rigger wants): two legs: standing straight, arms straight out "
               "to the sides at shoulder height; four legs: standing straight on straight legs, head forward, tail straight back; more legs: each "
               "leg straight out to its side, then down to the ground, evenly spread; wings: spread straight out to the sides; no limbs (a grub, "
               "a snake, a fish): its body straight and level along its length, head forward, nothing curled. Zoom in to be sure which it has. "
               "Say in your reply which of these it got."),
    "as_is": "Pose: as-is: give it the pose the pictures show.",
}


def settings_lines(record, root=None):
    """What the request's settings ask of the model, a line each (a set of floor tiles': of each
    tile, and no lines: it draws none), with its kind's instructions (the window's kinds:
    request_kinds.json, the project's own with `root`)."""
    floor = record["kind"] == "floor"
    styles = FLOOR_STYLES if floor else STYLES
    out = [styles.get(record["style"], styles["smooth"])]
    if record["style"] == "accurate" and record["details"].strip():
        out.append("Model details from the user: " + record["details"].strip())
    if not floor:
        out.append(LINES.get(record["style"], LINES["smooth"]))
    if record["kind"] == "creature":
        out.append(POSES.get(record["pose"], POSES["t_pose"]))
    if record["kind"] == "grass":
        w, d = record["cells"]
        out.append(f"Grass: it fills {w} x {d} cell{'' if w * d == 1 else 's'} to the edges, as a tile does (requests.md, \"Grass\"): fit it with "
                   f"kit.fit_cells(parts, ({w}, {d}), job.height) in place of kit.fit, so its bounds are exactly its cells: copies in the "
                   "cells side by side join without a gap. A grass prop: kit.finish's category \"grass\", never blocking the way. "
                   f"In pieces: it's grown over whole fields and drawn thinner further off by leaving out whole pieces (what's joined "
                   "together: a blade, a leaf, a stalk with its seeds): at its thinnest (its biggest piece, with any mat under them as wide "
                   f"as its cells) at most {mr.GRASS_THINNEST * w * d} vertices, and at most {mr.GRASS_MOST * w * d} in all ({mr.GRASS_MOST} a "
                   "cell, as the engine counts them in its .glb: flat faces and seams split vertices). kit.finish refuses more.")
    elif record["kind"] == "rock":
        w, d = record["cells"]
        out.append(ROCK.format(w=w, d=d))
    elif record["kind"] != "floor" and list(record["cells"]) != [1, 1]:
        # (a drawing outlined with its own cells: the ground it covers)
        w, d = record["cells"]
        out.append(f"Cells: it covers about {w} x {d} cells on the ground (Blender's X and Y), as the user says: size it so.")
    if record["kind"] == "environment":
        out.append("Environment: an asset that dresses the outdoors (requests.md, \"Environment\"): a rock, a bush, flowers, a log, a "
                   "tree... kit.finish's category the one that fits (tree, flora, rock, grass, decoration), blocking the way when it's big "
                   "enough to walk round (a rock, a tree, a log), not when it's walked through (flowers, a low bush).")
    if record["kind"] == "flower":
        out.append(FLOWER)
    if record["kind"] == "tree":
        out.append(TREE)
    # what its kind (and its variety) asks of every run, the user's: a feature each
    said = mr.instructions(record, mr.kinds(root))
    if said:
        out.append(INSTRUCTIONS + "".join(f"\n  - {key}: {text}" if key else f"\n  - {text}" for key, text in said))
    if record["kind"] != "floor" and record.get("many"):
        out.append(MANY)
    out.append(TOON)
    if record["kind"] != "floor" and record["variants"] > 0:
        n = record["variants"]
        how = record["vary"].strip()
        whose = "each of its drawings' requests makes" if record["several"] and not record["parent"] else "make"
        out.append(f"Variants: {whose} {n} besides it, each a prop of its own (requests.md, \"Variants\")"
                   + (f"; how they differ, as the user says: {how}" if how else "") + ".")
    shapes = record["shapes"]
    if floor:
        out.append(f"Shapes: each tile as many as its design needs, up to about {mr.SHAPES_CAP} for the most detailed." if shapes["unlimited"] else
                   f"Shapes: {shapes['min']} to {shapes['max']} shapes for the most detailed tile (boxes, slabs, cylinders, each counts as one), a plain one only a few.")
    elif shapes["unlimited"]:
        out.append(f"Shapes: as many as the design needs, up to about {mr.SHAPES_CAP}. Before writing model.py, think it through: list the parts "
                   "the design is made of and the shape for each; a long, careful script is fine, and so is taking your time.")
    else:
        out.append(f"Shapes: build it from {shapes['min']} to {shapes['max']} shapes (spheres, capsules, boxes, cylinders, lathes, slabs, "
                   "a blob's elements, each counts as one).")
    return "\n".join(out)


def part_of(record, go_on=False, again=False, variants=False):
    """Which part of the recipe a run does: "all" of it; "lines", up to the front and the side,
    for a request that checks its lines first; "model", from the model on, once they're checked;
    "again", the model the last run made, made again with the user's comments on it;
    "variants", the model made kept and its variants made; "split", a request whose pictures
    hold several of the same kind of asset, each made a request."""
    if variants:
        return "variants"
    if again:
        return "again"
    if go_on:
        return "model"
    if record["several"] and not record["parent"]:
        return "split"
    if record["kind"] == "floor":
        return "floor"
    return "lines" if record["check_lines"] else "all"


def indented(text):
    """A text of the user's (or the last run's) in the prompt, its lines indented under the
    one that says whose it is."""
    return "\n".join("    " + line.rstrip() for line in text.strip().splitlines())


def vary_text(record):
    """How the variants differ, in the prompt: as the user says, else as the same kind of thing
    differs."""
    said = record["vary"].strip()
    if said:
        return "they differ as the user says:\n" + indented(said)
    return ("each a little different from it, as the same kind of thing differs in nature: its size,\n"
            "  its shape, how its parts sit, its colours a shade apart; never another kind of thing.")


def prompt(record, go_on=False, note="", again=False, variants=False, root=None):
    """What Claude is asked for the request: the recipe, or its part (part_of), with what the
    user says is off in the lines (`note`, from the window's check) fixed first; made again
    (`again`), what the user says of the last run's model (`note`, else the record's comments);
    `variants`, its variants made from its model. A request that asks for variants has them
    made with its model. With the project (`root`), a run that builds the model reads first what
    the ratings teach of its kind, when they teach anything yet."""
    rid = record["id"]
    lessons = LESSONS.format(kind=mr.kind_key(record), id=rid) if root is not None and mr.lessons(root, rid) else ""
    note = (note.strip() or record["comments"].strip()) if again else note.strip()
    said = indented(note)
    if note:
        checked = (f"What the user says is off in them:\n{said}\n"
                   "  Fix that in lines.json first (the pictures' lines, and the front and the side with them),\n"
                   f"  run python tools/blender/model_requests.py lines {rid} again and look at each drawing, and\n"
                   "  leave the rest as it is;")
    else:
        checked = "The user says they look right: leave lines.json as it is;"
    # starting again rather than going on: the note is about the lines there
    noted = (f"- the user looked at the lines the last run made (lines.json) and says this is off in them:\n{said}\n"
             "  Fix that in lines.json first (the pictures' lines, and the front and the side with them),\n"
             "  then go on below;\n") if note else ""
    drawn = rl.user_bones(record)
    bones = BONES.format(id=rid, drawn=bones_drawn(drawn)) if drawn else ""
    # the rig on the bones carried into the front and the side
    rig = (" on the user's bones (kit.rig(kit.parts(), chains) with chains = job.chains(): the front's and the side's \"bone\" "
           "lines as chains in cells, the body's first)") if drawn else ""
    # made again: what the runs made again before were told (made: keep them so)
    told = [c.strip() for c in record["earlier"] if str(c).strip()]
    earlier = ("- the runs that made it again before this one were told, in turn (those changes are made:\n  keep them):\n"
               + "\n".join(f"    {k}. " + "\n       ".join(c.splitlines()) for k, c in enumerate(told, 1)) + "\n") if told else ""
    part = part_of(record, go_on, again, variants)
    shapes = mr.outlined(record)
    template = {"all": PROMPT, "lines": LINES_RUN, "model": GO_ON_RUN, "again": AGAIN_FLOOR_RUN if record["kind"] == "floor" else AGAIN_RUN,
                "variants": VARIANTS_RUN, "split": OUTLINED_RUN if shapes else SPLIT_RUN, "floor": FLOOR_RUN}[part]
    kind, height = record["kind"], record["height"]
    if kind == "floor":
        what = "a set of floor tiles, each a cell, laid to fit by wave function collapse"
    elif kind == "grass":
        w, d = record["cells"]
        what = f"grass filling {w} x {d} cell{'' if w * d == 1 else 's'} to the edges, {height} cells tall"
    elif kind == "tree":
        what = f"a tree {height} cells tall, its trunk a mesh and its foliage clusters of leaf cards"
    elif kind == "environment":
        what = f"an environment asset {height} cells tall"
    elif kind == "flower":
        what = f"a flower {height} cells tall, made finely"
    elif kind == "rock":
        w, d = record["cells"]
        what = f"a rock on about {w} x {d} cells, as tall as its pictures' proportions make it"
    else:
        what = f"a {kind} {height} cells tall"
    # asked as a kind of the window's own (a kind the user added), or a variety of one: named so
    k, v = mr.kind_of(record, mr.kinds(root))
    named = (k or {}).get("name", "").strip()
    if v is not None:
        what = f"a {v.get('name', '').strip().lower()} (a variety of {named.lower() or kind}): {what}"
    elif k is not None and k.get("id") != kind and named:
        what = f"a {named.lower()}: {what}"
    count = record["variants"]
    vary = vary_text(record)
    # the variants made with the model (a run that makes it), as the settings ask
    step = VARIANT_STEP.format(count=count, vary=vary, last=count + 1) if count > 0 and kind != "floor" else ""
    made = f" (with its variants {', '.join(record['variant_props'])})" if record["variant_props"] else ""
    return template.format(id=rid, name=record["name"], what=what, height=height, notes=record["notes"].strip() or "(none)",
                           settings=settings_lines(record, root), checked=checked, noted=noted, bones=bones, rig=rig,
                           prop=record["prop"] or "(none)", reply=indented(record["reply"]) or "    (nothing)", earlier=earlier, said=said,
                           variants=step, count=len(shapes) if part == "split" and shapes else count, vary=vary, made=made,
                           outlined=outlines_said(record, shapes), lessons=lessons)


def outlines_said(record, shapes):
    """How many drawings the user has outlined, on which pictures: "3 on front.png and 1 on
    side.png"."""
    each = [(pic, sum(1 for p, _ in shapes if p == pic)) for pic in record["images"]]
    said = [f"{n} on {pic}" for pic, n in each if n]
    return said[0] if len(said) == 1 else ", ".join(said[:-1]) + " and " + said[-1] if said else "none"


def bones_drawn(drawn):
    """Which pictures the user has drawn bones on, and how many lines: "front.png (5 lines) and
    side.png (3 lines)"."""
    said = [f"{pic} ({len(lines)} line{'' if len(lines) == 1 else 's'})" for pic, lines in drawn.items()]
    return said[0] if len(said) == 1 else ", ".join(said[:-1]) + " and " + said[-1]


def drawn_since(path, since):
    """A file written since then (a second's slack for the file system's clock)."""
    try:
        return path.stat().st_mtime >= since - 1.0
    except OSError:
        return False


def clip(text, n=SHOWN):
    lines = text.splitlines()
    return text if len(lines) <= n else "\n".join(lines[:n]) + f"\n... ({len(lines) - n} more lines)"


def flat(content):
    """A tool's answer as text (a picture as a word)."""
    if isinstance(content, str):
        return content
    out = []
    for c in content or []:
        if isinstance(c, dict) and c.get("type") == "text":
            out.append(c.get("text", ""))
        elif isinstance(c, dict) and c.get("type") == "image":
            out.append("(a picture)")
        else:
            out.append(json.dumps(c, ensure_ascii=False)[:200])
    return "\n".join(out)


def what(name, args):
    """A tool call in a line: its command, file or pattern."""
    key = {"Bash": "command", "PowerShell": "command", "Read": "file_path", "Write": "file_path", "Edit": "file_path", "Glob": "pattern", "Grep": "pattern"}.get(name)
    if key and key in args:
        return str(args[key]) + (f"   ({args['description']})" if args.get("description") else "")
    return json.dumps(args, ensure_ascii=False)[:300]


def describe(event, run):
    """What one event of Claude's stream says, as lines of the log; what the run learns."""
    kind = event.get("type")
    if kind == "stream_event":
        # a piece of what Claude is writing (a long script can take minutes): counted, and
        # said once a minute
        delta = (event.get("event") or {}).get("delta") or {}
        run["written"] = run.get("written", 0) + len(delta.get("text") or delta.get("partial_json") or delta.get("thinking") or "")
        if time.time() - run.get("told", 0) > 60 and run["written"]:
            run["told"] = time.time()
            return [f"(Claude is writing: {run['written']:,} characters so far)"]
        return []
    if kind == "assistant":
        run["written"], run["told"] = 0, time.time()
    if kind == "system" and event.get("subtype") == "init":
        run["model"] = event.get("model")
        return [f"Claude reached: model {event.get('model')}, Claude Code {event.get('claude_code_version', '?')}, "
                f"permission mode {event.get('permissionMode')}, session {event.get('session_id')}"]
    if kind == "system":
        return [f"(Claude Code: {event.get('subtype')})"]
    if kind == "assistant":
        if event.get("error"):
            run["error"] = event["error"]
        out = []
        for c in event.get("message", {}).get("content", []):
            if c.get("type") == "text" and c.get("text", "").strip():
                out.append("Claude: " + c["text"].strip())
            elif c.get("type") == "thinking" and c.get("thinking", "").strip():
                out.append("Claude thinks: " + clip(c["thinking"].strip(), 12))
            elif c.get("type") == "tool_use":
                out.append(f"→ {c.get('name')}: {what(c.get('name'), c.get('input', {}))}")
        return out
    if kind == "user":
        content = event.get("message", {}).get("content", [])
        out = []
        for c in content if isinstance(content, list) else []:
            if isinstance(c, dict) and c.get("type") == "tool_result":
                out.append(("× " if c.get("is_error") else "← ") + clip(flat(c.get("content")).strip() or "(nothing)"))
        return out
    if kind == "result":
        run["result"] = event
        secs = (event.get("duration_ms") or 0) / 1000
        cost = event.get("total_cost_usd")
        out = [f"Claude finished ({event.get('subtype')}{', with an error' if event.get('is_error') else ''}): {event.get('num_turns')} turns, "
               f"{int(secs // 60)} min {int(secs % 60)} s" + (f", ${cost:.2f} at API prices" if cost else "")]
        denials = event.get("permission_denials") or []
        if denials:
            out.append("Refused, not allowed in this run: " + "; ".join(f"{d.get('tool_name')} {what(d.get('tool_name'), d.get('tool_input') or {})}" for d in denials))
        if event.get("result"):
            out.append("Its last word: " + clip(str(event["result"]).strip(), 12))
        return out
    return []


def stop_tree(proc):
    """Claude and what it started (Blender, a shell)."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True)
    else:
        try:
            os.killpg(proc.pid, 9)
        except OSError:
            proc.kill()


def unrunnable(root, rid, record, part, go_on, note, again, variants, say):
    """What keeps a run from starting (the request's status then), or None: nothing to go on
    from, to make again from, to change or to vary yet, or split already."""
    here = mr.folder(root, rid)
    if go_on and not (here / "lines.json").is_file():
        return mr.fail(root, rid, "nothing to go on from: no lines.json yet (Start draws the lines first)")["status"]
    if again and not (here / "model.py").is_file():
        return mr.fail(root, rid, "nothing to make again from: no model.py yet (Start makes it first)")["status"]
    if again and not note:
        return mr.fail(root, rid, "nothing to change: say what in the comments first")["status"]
    if variants and not ((here / "model.py").is_file() and record["prop"]):
        # (made or not, the request stays as it is)
        say("Nothing to make variants of: no model made yet (Start makes it first)")
        return record["status"]
    if variants and record["variants"] < 1:
        say("No variants asked for: say how many in Variants first")
        return record["status"]
    if part == "split" and record["children"]:
        say(f"Split already, into {len(record['children'])} requests: nothing to run (the editor's Make them all makes them)")
        return record["status"]
    return None


def announce(record, part, note, say):
    """What the run is, in its log: its part of the recipe, what the user said, their bones."""
    if part == "lines":
        say("This run: the lines, the front and the side, then it stops for the user to check them (\"Let me check the lines first\")")
    elif part == "model":
        say("This run goes on from the model, on the lines the user checked: " + (f"what's off: {note.strip()}" if note.strip() else "they look right"))
    elif part == "again":
        say(f"This run makes it again from the last run's model, with the user's comments: {note}")
    elif part == "variants":
        how = record["vary"].strip()
        say(f"This run makes {record['variants']} variant{'' if record['variants'] == 1 else 's'} of it from its model, the model itself kept: "
            + (how if how else "as Claude sees fit"))
    elif part == "split" and mr.outlined(record):
        say(f"This run splits the request: the {len(mr.outlined(record))} drawings the user outlined, each a request of its own (\"Process multiple\"); the rest left out")
    elif part == "split":
        say("This run splits the request: each drawing on its pictures made a request of its own (\"Process multiple\")")
    if part not in ("model", "again") and note.strip():
        say(f"What the user says is off in the lines there: {note.strip()}")
    drawn = rl.user_bones(record)
    if drawn:
        say(f"The user has drawn the creature's bones over {bones_drawn(drawn)}: Claude reads the pictures by them")


def conclude(root, rid, part, began, again, note, result, returncode, error, claude, say):
    """A run's end, by what's on disk and what Claude said last (`result`: its result event, or
    {}; `error`: an error its messages had): done (split: its requests to make), waiting for the
    user's check of its lines, or failed and why. The request's status."""
    here = mr.folder(root, rid)
    record = mr.load(root, rid)
    last = str(result.get("result") or "").strip()
    if part == "split" and record["children"] and returncode == 0 and not result.get("is_error"):
        # split: done, its requests to make
        p = dict(record["progress"])
        took = (time.time() - p["started"]) / 60.0 if p["started"] > 0 else 0.0
        p.update(step=f"split into {len(record['children'])} requests", done=1.0, minutes=float(round(took, 1)))
        mr.update(root, rid, status="done", progress=p, **({"reply": last} if last else {}))
        say(f"Split into {len(record['children'])} requests: {', '.join(record['children'])}")
        return "done"
    if record["status"] == "done":
        say(f"Done: the prop {record['prop']}" + (f", and its variants {', '.join(record['variant_props'])}" if record["variant_props"] else ""))
        if again:
            # the comments made: told to the runs made again after this one; the field emptied
            # unless the user has written others meanwhile
            fields = {"earlier": record["earlier"] + [note]}
            if record["comments"].strip() in ("", note):
                fields["comments"] = ""
            mr.update(root, rid, **fields)
        return "done"
    if last:
        # what it says back (for a check, what it guessed in the lines): the window shows it
        mr.update(root, rid, reply=last)
    if error == "authentication_failed" or "Not logged in" in last:
        # the program by its path: it's often not on the PATH (the editor's "Log in" opens it)
        reason = f'Claude Code isn\'t signed in to Claude: the editor\'s Claude sign-in (or "{claude} auth login" in a terminal), then Start again'
    elif returncode != 0 or result.get("is_error"):
        reason = f"Claude stopped with an error ({result.get('subtype') or 'no result'}, exit code {returncode})"
    elif part == "lines" and drawn_since(here / "profiles.png", began):
        # where it was asked to stop: the user's turn
        return mr.wait_for_check(root, rid)["status"]
    elif part == "lines":
        reason = "Claude ended without drawing the front and the side (profiles.png)"
    elif part == "split":
        reason = "Claude ended without splitting it (no request made of a drawing)"
    else:
        reason = "Claude ended without adding the model"
    return mr.fail(root, rid, reason)["status"]


def run(root, rid, model=None, quiet=QUIET, go_on=False, note="", again=False, variants=False):
    """The run, start to end; the request's status as it ended. `go_on`: on from the model, on
    the lines the user checked, `note` saying what's off in them. `again`: the model the last
    run made, made again as the user's comments say (`note`, else the record's). `variants`:
    variants of the model made, as many as the request says, the model itself kept."""
    record = mr.load(root, rid)
    here = mr.folder(root, rid)
    stop = here / "stop"
    if stop.exists():
        stop.unlink()
    began = time.time()
    part = part_of(record, go_on, again, variants)
    if again:
        note = note.strip() or record["comments"].strip()

    def say(text):
        mr.log(root, rid, text)

    say("———————— a run starts ————————")
    ended = unrunnable(root, rid, record, part, go_on, note, again, variants, say)
    if ended is not None:
        return ended
    claude = find_claude()
    if not claude:
        return mr.fail(root, rid, "Claude Code not found: install it, or set CLAUDE to the claude program")["status"]
    try:
        version = subprocess.run(program(claude) + ["--version"], capture_output=True, text=True, timeout=60).stdout.strip() or "?"
    except (OSError, subprocess.TimeoutExpired):
        version = "?"
    env, on = signed_in(claude)
    if env is None:
        return mr.fail(root, rid, f'Claude Code isn\'t signed in to Claude: the editor\'s Claude sign-in (or "{claude} auth login" in a terminal), then Start again')["status"]
    settings = here / "claude-settings.json"
    settings.write_text(json.dumps({"permissions": {"allow": allowed(rid)}}, indent=2) + "\n", encoding="utf-8")
    args = ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-mode", "dontAsk", "--strict-mcp-config", "--no-session-persistence",
            "--settings", str(settings.relative_to(root))]
    if model:
        args += ["--model", model]
    text = prompt(record, go_on, note, again, variants, root=root)
    mr.start(root, rid, mr.estimate(record, part))
    mr.step(root, rid, "Claude is starting", 0.05)
    say(f"Claude Code: {claude} ({version})")
    say(f"On: {on}")
    announce(record, part, note, say)
    say(f"Sent: claude {' '.join(args)}   (the prompt on its input; model: {model or 'its default'})")
    say("Allowed without asking: " + ", ".join(allowed(rid)))
    say("Prompt:\n" + text)
    extra = {} if os.name == "nt" else {"start_new_session": True}
    proc = subprocess.Popen(program(claude) + args, cwd=str(root), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace", bufsize=1, env=env, **extra)
    state = {"last": time.time(), "model": None, "result": None, "error": None}
    lock = threading.Lock()
    raw = open(here / "run.jsonl", "a", encoding="utf-8", newline="\n")

    def out():
        for line in proc.stdout:
            with lock:
                state["last"] = time.time()
                try:
                    event = json.loads(line)
                except ValueError:
                    if line.strip():
                        say(line.rstrip())
                    continue
                # the whole events kept raw, not the pieces of one being written
                if event.get("type") != "stream_event":
                    raw.write(line)
                    raw.flush()
                for text in describe(event, state):
                    say(text)

    def err():
        for line in proc.stderr:
            with lock:
                state["last"] = time.time()
                if line.strip():
                    say("(Claude Code says) " + line.rstrip())

    readers = [threading.Thread(target=out, daemon=True), threading.Thread(target=err, daemon=True)]
    for t in readers:
        t.start()
    try:
        proc.stdin.write(text)
        proc.stdin.close()
    except OSError:
        pass
    ended = None
    while proc.poll() is None:
        time.sleep(0.5)
        if stop.exists():
            stop_tree(proc)
            ended = "cancelled"
        elif time.time() - state["last"] > quiet:
            stop_tree(proc)
            ended = "quiet"
        if ended:
            break
    proc.wait()
    for t in readers:
        t.join(timeout=10)
    raw.close()
    # what the run cost, kept with the request (its runs' costs add up)
    cost = (state.get("result") or {}).get("total_cost_usd")
    if cost:
        mr.update(root, rid, costs=list(mr.load(root, rid).get("costs", [])) + [round(float(cost), 2)])
    if ended == "cancelled":
        stop.unlink(missing_ok=True)
        say("Cancelled from the editor: Claude stopped, the request waits again")
        return mr.update(root, rid, status="waiting", progress=dict(mr.FIELDS["progress"]))["status"]
    if ended == "quiet":
        return mr.fail(root, rid, f"no word from Claude for {max(1, round(quiet / 60))} min, so it was stopped")["status"]
    return conclude(root, rid, part, began, again, note, state["result"] or {}, proc.returncode, state["error"], claude, say)


# ---------------------------------------------------------------- in the cloud: the web editor's runs

# the prompt's checks, which build the engine: in the cloud there's no Rust, the editor reads it
CHECK = "- check: cargo run -q -p engine-content --bin content -- check"
CLOUD_CHECK = "- check: none in the cloud (no Rust there): the web editor reads the model once it's back"
CLOUD = """In the cloud: this is a sandbox of Anthropic's, the web editor's run, on the user's own API
key: they pay for it, so no detours. Blender runs through run_request.py as below (on a virtual
display), and there's no cargo and no Rust: skip any cargo step of requests.md (the editor
reads the model once it's back).

"""


def prompt_file(rid):
    """Where --prompt writes the prompt, for the sandbox's Claude to read."""
    return Path(tempfile.gettempdir()) / f"bf-prompt-{rid}.txt"


def start_in_cloud(root, rid, go_on=False, note="", again=False, variants=False):
    """--prompt: the run's start in the cloud, as Start's here (its checks, in progress from
    now, its log), and its prompt written for the sandbox's Claude to follow. The request's
    status."""
    record = mr.load(root, rid)
    part = part_of(record, go_on, again, variants)
    if again:
        note = note.strip() or record["comments"].strip()

    def say(text):
        mr.log(root, rid, text)

    say("———————— a run starts ————————")
    ended = unrunnable(root, rid, record, part, go_on, note, again, variants, say)
    if ended is not None:
        print(f"Nothing to run: {mr.load(root, rid)['progress']['step'] or 'the request stays as it is'}")
        return ended
    text = CLOUD + prompt(record, go_on, note, again, variants, root=root).replace(CHECK, CLOUD_CHECK)
    mr.start(root, rid, mr.estimate(record, part))
    mr.step(root, rid, "Claude is starting", 0.05)
    say("In the cloud: Claude in a sandbox of Anthropic's, on the user's own API key (the web editor's run)")
    announce(record, part, note, say)
    say("Prompt:\n" + text)
    path = prompt_file(rid)
    path.write_text(text, encoding="utf-8")
    print(f"Your instructions for {rid}: read {path} whole, then follow them, from {root}")
    return "in_progress"


def settle(root, rid, reply="", go_on=False, note="", again=False, variants=False):
    """--settle: the run's end in the cloud, as a run's here ends, by what's on disk (`reply`:
    the sandbox's Claude's line on what it made). The request's status."""
    record = mr.load(root, rid)
    part = part_of(record, go_on, again, variants)
    if again:
        note = note.strip() or record["comments"].strip()

    def say(text):
        mr.log(root, rid, text)

    began = record["progress"]["started"] or time.time()
    return conclude(root, rid, part, began, again, note, {"result": reply} if reply.strip() else {}, 0, None, "claude", say)


def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not argv or argv[0].startswith("-"):
        print(__doc__)
        return 2
    rid, model, quiet, note = argv[0], None, QUIET, ""
    if "--model" in argv:
        model = argv[argv.index("--model") + 1]
    if "--quiet" in argv:  # seconds, for the tests
        quiet = float(argv[argv.index("--quiet") + 1])
    if "--note" in argv:
        note = argv[argv.index("--note") + 1]
    if "--note-file" in argv:
        note = Path(argv[argv.index("--note-file") + 1]).read_text(encoding="utf-8")
    how = ("--go-on" in argv, note, "--again" in argv, "--variants" in argv)
    root = mr.find_root(Path.cwd())
    if "--prompt" in argv:
        status = start_in_cloud(root, rid, *how)
    elif "--settle" in argv:
        status = settle(root, rid, argv[argv.index("--reply") + 1] if "--reply" in argv else "", *how)
    else:
        status = run(root, rid, model, quiet, *how)
    print(f"{rid}: {status}" + (f", {mr.LINES_READY}" if status == "waiting" and mr.lines_ready(mr.load(root, rid)) else ""))
    return 0 if status in ("done", "waiting", "in_progress") else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
