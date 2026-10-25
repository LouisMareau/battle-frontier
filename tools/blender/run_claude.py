"""One model request made by Claude Code, unattended, with its log: what the map editor's
Requests window starts with its Start button (and stops with Cancel), or by hand:

    python tools/blender/run_claude.py ID [--model opus|sonnet|haiku] [--go-on] [--note "what's off in the lines"]
    python tools/blender/run_claude.py ID --again [--note "what to change"]
    python tools/blender/run_claude.py ID --variants
    python tools/blender/run_claude.py ID --stage study|research|review|look|detail|merge|body|dress|assemble [--go-on] [--role subject|references|methods]
    python tools/blender/run_claude.py ID --stage STAGE --direct      the director judges that stage's work
    python tools/blender/run_claude.py ID --retrospect                after the last: what it taught, kept
    python3 tools/blender/run_claude.py ID --prompt [--go-on] [--again] [--variants] [--stage NAME] [--note-file PATH]
    python3 tools/blender/run_claude.py ID --settle [the same] [--reply "what was made"]

A request of the "agentic" style (made in stages; requests.md, "Made in stages") is made by
a run per stage, --stage NAME, each charged with one thing (the study, the
research online, its review, the look, the detail, the merge, the body, the dress, the
assembly): its prompt is that charge, and it ends with the stage's files written, the editor
starting the next. The research stage runs three researchers at once, a Claude each (the
subject, the reference pictures, the methods; --role runs one alone), the only runs that may
read the web (Claude Code's WebSearch and WebFetch, and model_requests.py fetch for the
pictures, within the request's room for them); the review, the lead's, asks for a second
pass of them when something's missing, else compiles what they found. After each stage the
director's run (--direct) judges its work against the artwork, 0 to 100: under the grade
needed (model_requests.py's GRADE_NEEDED) the work goes back to the stage at fault, its run
told what to fix; the record's "next" says what the editor starts. Its agents run on Sonnet,
its director on Opus, unless --model says.

A request whose pictures hold several drawings of the same kind of asset ("several", the
editor's "Process multiple": trees, bushes, rocks...) is split by its first run: Claude finds
each drawing and makes it a request of its own (model_requests.py split), which the editor's
"Make them all" then has made one after another; the request itself is done once split. When
the user has outlined the drawings to make ("outlines"), only those are made, each cut to its
outline, as tall and on as many cells as the user said; the rest are left out.

Claude Code (`claude`: CLAUDE names the program, else the PATH, else ~/.local/bin) gets the
recipe (requests.md) for that one request and only the tools it needs, set in the request's
claude-settings.json: reading the project, writing in the request's folder (and running its
own helper scripts there, and python -c one-liners), Blender through
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
import re
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
  A character or a creature: the body, the head with the upper jaw, the
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
- a character (a creature too): rig it{rig}: every part on the bones that move it, the jaw's and the tongue's
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
{staged}- make its variants now, {count} besides it, each a prop of its own (requests.md, "Variants of a
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
CHECKED_LINES = """- the lines are drawn and the user has checked them: private/requests/{id}/lines.json,
  drawn over the pictures in lines_PICTURE.png and as the front and the side in profiles.png
  (look at them). Keep lines.json. {checked}
"""
CHECKED = CHECKED_LINES + """- model.py may already start with the design and the drawing sorted, as comments, from the run
  that made the lines (requests.md, "The design, not the drawing"): build on them. If it
  doesn't, sort them first, at its top, and say in the reply what you took for the drawing;
"""
RULES = """
Run commands with the Bash tool, exactly as written above, from the project folder, {folder}:
the shell keeps its folder between calls, so never cd into the request's folder (every path
above is from the project's); if a command fails with "can't open file tools/...", the shell
has moved: run cd {folder} first, on its own. One command a call: a call is refused whole when
any part of it isn't allowed (nothing joined with ; or &&, nothing from /tmp), and a refusal
is of that one call, never of the session: run the allowed command on its own and go on.
Allowed besides the commands above: python -c "...", python - with a heredoc, a helper script
of your own in private/requests/{id}/ run as python private/requests/{id}/NAME.py (one that
writes lines.json from numbers, say), mkdir there, ls, and reading files; to change a file,
the Edit tool (not a script that rewrites it). Write only inside private/requests/{id}/. Don't
use git, don't change anything else in the project (finishing writes the prop itself), don't
touch other requests. {end}"""
MADE = "End with one line saying what you made."

# ({noted}: what the user said is off in the lines an earlier run made, when they start again)
PROMPT = (HEAD + "Follow tools/blender/requests.md for this one request, from its step 3 (it's started already):\n"
          + READ + "{directed}{stage_lessons}" + "{lessons}{bones}{noted}" + LINES + MODEL + RULES.replace("{end}", MADE))
LINES_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, from its step 3 to its step 5 (it's\n"
             "started already), then stop: the user checks the lines before the model is built (the request\n"
             "says \"Let me check the lines first\"):\n"
             + READ + "{directed}{stage_lessons}" + "{bones}{noted}" + LINES + STOP + RULES.replace("{end}", "End with a few lines for the user, who\n"
                                                           "checks the lines next: what you took for the drawing, what you guessed, and anything in the\n"
                                                           "lines you're unsure of."))
GO_ON_RUN = (HEAD + "Follow tools/blender/requests.md for this one request from its step 6, the model (it's started\n"
             "already):\n" + READ + "{directed}{stage_lessons}" + "{lessons}" + CHECKED + MODEL + RULES.replace("{end}", MADE))
# a request made once, made again with the user's comments on it ({said}), from what the last
# run made ({reply}: what it said back; {earlier}: what the runs made again before were told)
AGAIN = """- the last run made it: private/requests/{id}/model.py built it (its parts listed at its top,
  with the design and the drawing sorted as comments; design.py too when there is one, which
  model.py runs first) on lines.json (drawn over the pictures in lines_PICTURE.png, the front
  and the side in profiles.png). Its renders are preview_*.png and compare.png, and it's in the
  game as the prop {prop}. What it said back:
{reply}
  Read model.py, and look at the renders and the drawings, before changing anything;
{staged}{earlier}- the user has looked at that model, and says for this run:
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
             + READ + "{directed}{stage_lessons}" + SPLIT + RULES.replace("{end}", "End with a few lines for the user: the requests you made, each one's name and height, and\n"
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
                "\"Process multiple\"; it's started already):\n" + READ + "{directed}{stage_lessons}" + OUTLINED
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
             "it's a set of floor tiles (\"A set of floor tiles\"):\n" + READ + "{directed}{stage_lessons}" + "{lessons}{noted}" + FLOOR + RULES.replace("{end}", MADE))
AGAIN_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, made again from what the last run made,\n"
             "with the user's comments on it (requests.md, \"Made again, with the user's comments\"; it's started\n"
             "already):\n" + READ + "{directed}{stage_lessons}" + AGAIN
             + MODEL.replace("- write private/requests/{id}/model.py with", "- change private/requests/{id}/model.py, the last run's, with", 1)
             + RULES.replace("{end}", "End with a few lines for the user: what you changed for each of their comments, and what\n"
                                      "you left as it was."))
VARIANTS_RUN = (HEAD + "Follow tools/blender/requests.md for this one request: variants of the model made (requests.md,\n"
                "\"Variants of a model made\"; it's started already):\n" + READ + "{directed}{stage_lessons}" + VARIED
                + RULES.replace("{end}", "End with a few lines for the user: the variants you made, and how each differs from the\n"
                                         "model."))
# a request made in stages (requests.md, "Made in stages"): each stage a run of its own, with
# one charge, the plan folder shared between them ({stage}: this run's)
STAGED = """This request is made in stages, a run of Claude each with one charge, the editor starting the
next once the last has ended (requests.md, "Made in stages"): {stages}. This run is its
{STAGE} stage, {k} of {n}: {about}. The stages share private/requests/{id}/plan/: each
writes its own file there and no other, and plan/notes.log is append-only, a line each
starting with "[{stage}] " (what you guessed, what you're unsure of, what another stage should
know or change): add lines to it, never change or remove any.

"""
# a model made in stages, made again or varied: its files
WAS_STAGED = """- it was made in stages (requests.md, "Made in stages"): model.py builds it from body.py (the
  body), materials.py (the colours and the paint) and detail.py (the detached parts and the
  detail), each loaded with kit.load, on plan.md; change whichever holds what the user asks
  for (and plan.md with it);
"""
SHAPE_PLAN = """- write private/requests/{id}/plan/shape.md, the shape plan the other stages build on: the
  design and the drawing sorted (above); the parts the design is made of (what each is, its
  mass, where it attaches, its size and place in cells from the profiles); the proportions
  measured off the pictures (heights in heads, widths against heights); the pose it's built
  in; the block-out (a primitive for each mass: ellipsoid, capsule, tube, blob, with its
  centre, radii and rotation in cells); the rig's chains (named as requests.md step 9 names
  them, each joint in cells, the body's first; with the user's bones, theirs); and what stays
  detached from the body (the lower jaw, the eyes, horns, claws, a prop's lid), named, for
  the detail stage to plan. The breakdown stage cuts each contoured element out next: give
  every line of lines.json its own name (a part, a marking, a piece of clothing or gear) and
  contour every element the pictures show, the outline included;
- append to plan/notes.log what you took for the drawing, what you guessed and what you're
  unsure of ("[study] " lines);
"""
STUDY_RUN = (HEAD + STAGED + "Follow tools/blender/requests.md for this one request from its step 3 to its step 5 (it's\n"
             "started already), then plan its shape:\n" + READ + "{directed}{stage_lessons}" + "{lessons}{bones}{noted}"
             + LINES.replace("Write both lists as comments\n  at the top of model.py before any shape", "Write both lists\n  at the top of plan/shape.md (below) before any shape")
             + SHAPE_PLAN + RULES.replace("{end}", "End with one line saying what you planned."))
# the study stage of a request that checks its lines first: it stops at the lines, as
# LINES_RUN does, and another run of it plans the shape on the lines the user checked
STUDY_LINES_RUN = (HEAD + STAGED + "Follow tools/blender/requests.md for this one request, from its step 3 to its step 5 (it's\n"
                   "started already), then stop: the user checks the lines before the stages go on (the request\n"
                   "says \"Let me check the lines first\"):\n"
                   + READ + "{directed}{stage_lessons}" + "{bones}{noted}"
                   + LINES.replace("Write both lists as comments\n  at the top of model.py before any shape", "Keep both lists for\n  the reply")
                   + STOP + RULES.replace("{end}", "End with a few lines for the user, who\n"
                                                   "checks the lines next: what you took for the drawing, what you guessed, and anything in the\n"
                                                   "lines you're unsure of."))
STUDY_GO_ON_RUN = (HEAD + STAGED + "The lines are drawn and checked: plan its shape on them (it's started already):\n"
                   + READ + "{directed}{stage_lessons}" + "{lessons}" + CHECKED_LINES
                   + SHAPE_PLAN.replace("the\n  design and the drawing sorted (above)", "the\n  design and the drawing sorted (requests.md, \"The design, not the drawing\")")
                   + RULES.replace("{end}", "End with one line saying what you planned."))
# the games whose anime style the game's characters aim at: what the breakdown's research plan
# and the researchers look to for the style's proportions, faces, hair, shading and rigging
STYLE_GAMES = "Granblue Fantasy: Relink, Wuthering Waves, Genshin Impact, Zenless Zone Zero and DragonSword: Awakening"
# the breakdown stage: each contoured element of the study cut out enhanced, contoured again
# precisely and analysed, and the research planned from what it found
BREAKDOWN_RUN = (HEAD + STAGED + "Break the artwork down, element by element, for the stages after you (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan/shape.md and plan/notes.log, and the lines the study stage made: python
  tools/blender/model_requests.py breakdown {id} lists each picture's contoured elements with
  their boxes (look at lines_PICTURE.png too);
- for each contoured element of each picture (every line, the outline included as the whole;
  a mark alone, when it names something the pictures show, too), cut it out enhanced:
  python tools/blender/model_requests.py cutout {id} PICTURE "NAME"
  (plan/breakdown/PICTURE/NAME.png, enlarged and sharpened; NAME.json beside it holds its
  box, its scale and the study's line in the cutout's pixels, a start). Look at it, and
  zoom further where a detail hides (python tools/blender/model_requests.py zoom {id}
  plan/breakdown/PICTURE/NAME.png X0 Y0 X1 Y1);
- contour it again, precisely, in NAME.json's "lines" (in the cutout's pixels; the shapes
  lines.json takes: points with closed and smooth, ellipse, mark; a line's "corners", the
  indices of its points where the edge turns sharply): the element's edge as it truly runs,
  straight runs as straight segments through their corners, curves as smooth lines, so a
  flat face or a right angle is told from a rounded one; then its inner lines: seams, folds,
  plates, the border of each material and each colour. Then
  python tools/blender/model_requests.py contour {id} PICTURE "NAME"
  draws them over the cutout (NAME_lines.png, the corners squared): look, and move the lines
  until they sit on the enhanced edge to the pixel;
- write NAME.md beside it, the element's analysis: its geometry (the straight edges and their
  angles, the curves and how tight, its symmetry, its proportions against the body in heads
  or against the part it's on, measured on the cutout and given in the picture's pixels),
  what it's made of and its finish (skin, fur, cloth, leather, metal, glass, wood...; matte,
  glossy, rough, worn, how it catches light in the picture), its colours (measured: python
  tools/blender/model_requests.py colours {id} PICTURE X0 Y0 X1 Y1), how it attaches and
  moves, and what the picture leaves unsure;
- write private/requests/{id}/plan/breakdown.md, the index: a line per element (its picture,
  its name, its files, its material in a word, and what to model it as: the skin's, a
  detached part, cards, a painted texture), then the style the pictures are drawn in (anime,
  cartoon, painterly, realistic) and what that asks of the model in a few lines;
- write private/requests/{id}/plan/research_plan.md, the plan the researchers follow, a
  section each ("## subject", "## references", "## methods"), each item a question to answer
  with the element it serves: for the subject, what the design is drawn from and what to
  settle about it; for the references, each material and finish to find references of (a
  metal limb: metal items and how their roughness and specular read; a leather pouch: how
  leather is made and how it reflects; cloth, fur, glass, wood likewise) and each view or
  detail the pictures lack; for the methods, each unusual thing to learn: the style and what
  it asks (anime proportions, how anime eyes are made and animated, hair and how it's rigged,
  when that's the style; the game's characters aim at the anime style of {games}: how those
  games build, shade and rig their characters is the target), and how each material is done
  in Blender and the kit (a glossy
  metal, a rough leather, a texture painted from the picture);
- append to plan/notes.log what you're unsure of and what the study's lines got wrong (so
  the study's next run learns), "[breakdown] " lines;
""" + RULES.replace("{end}", "End with one line saying how many elements you broke down."))
# what the stages after the breakdown read of it
BREAKDOWN_READ = """- read plan/breakdown.md and, for each element you work on, its analysis (plan/breakdown/
  PICTURE/NAME.md) and its precise contour drawn over the enhanced cutout (NAME_lines.png
  over NAME.png): the geometry as measured, straight and curved told apart, the corners,
  and the material and its finish;
"""
# the research stage: three researchers at once (a run of Claude each, its own file; in the
# cloud, one session does the three in turn), and a second pass when the lead's review asks
# more ({more}: MORE then)
RESEARCHER = """You are its {ROLE} researcher, one of three working at once ({roles}), each writing
private/requests/{id}/plan/research/{role}.md and nothing else of the plan, with
"[research:{role}] " lines in plan/notes.log. Pass {pass_} of at most 2.{more}

"""
MORE = """ The lead reviewed the first pass and asks for more:
read plan/research/more.md (your section first) and answer it in your file under a "## Second
pass" heading, keeping the first pass's text."""
RESEARCH_READ = """- read plan/shape.md (what the study stage made of the pictures: the parts, the proportions,
  what it guessed) and every line of plan/notes.log (its guesses and doubts), and look at the
  lines it made (lines_PICTURE.png, profiles.png);
- read plan/research_plan.md, the breakdown's plan for the research: your section first,
  question by question (each names the element it serves: look at that element's cutout and
  analysis in plan/breakdown/, listed in plan/breakdown.md), then the other sections for what
  touches yours;
"""
# how the references researcher brings a picture in: fetched here; in the cloud (its sandbox's
# network shut), its address noted instead
FETCH_HOW = """find a picture that shows it and fetch it:
  python tools/blender/model_requests.py fetch {id} URL --name NAME (into plan/refs/: png,
  jpeg, webp or gif, 8 MB at most each, 16 in all for the request: the command says how many
  are taken), then look at it (Read) and keep it only when it shows what was wanted (one that
  doesn't is left out of your file); up to eight this pass. They're for the modeller to look
  at, never copied: no texture or shape taken from them"""
CLOUD_FETCH_HOW = """find a picture that shows it, look at it through the web fetch tool when it can be read, and
  note its address and what it shows: none can be brought into this sandbox (its network is
  shut), the user opens them by their addresses; nothing copied from them"""
SUBJECT = """- research online with the web search and fetch tools what the design is drawn from: a named
  creature, character or object the notes or the pictures name (how it looks, from every
  side); a made-up one: the animals, plants or things it's built from, their anatomy and
  proportions (heights in heads, where the limbs attach, how the joints bend), how their
  parts attach and move, how their fur, scales, feathers or bark lie, the colours such a
  thing has in life where the pictures are unsure; and the lead's doubts (the notes), each
  answered where the web can. A diagram or a photo that says it best is kept as the
  references researcher keeps pictures, at most three;
- write private/requests/{id}/plan/research/subject.md: what the design is, in a line; then
  the facts, each with where it came from (the page's address); the lead's doubts answered;
  what stays unknown. What the user's notes say wins over anything found;
"""
REFERENCES = """- first, the reference pictures earlier requests kept, each with what it shows:
  python tools/blender/model_requests.py refs (words after it narrow the list); one that shows
  what you need comes in with python tools/blender/model_requests.py reuse {id} PATH --name
  NAME (into plan/refs/, counted as a fetched one is); the web is for the rest;
- gather reference pictures of what the user's pictures don't show: the views the model needs
  (the back, the underside, the feet, the face straight on, the inside of the mouth) and the
  details (how the fur lies, a scale's shape, a claw, a horn's ridges), of the thing the
  design is drawn from (a named one: itself; a made-up one: what it's built from). For each,
  {fetching};
- a character: two or three pictures of characters from the anime-styled games the look aims
  at ({games}), kept as references of the style: how their faces, hair, hands and clothes are
  built in 3D;
- write private/requests/{id}/plan/research/references.md: each picture kept, with its file in
  plan/refs/ (or its address), what it shows (which view, which detail), where it came from;
  and the views and details you couldn't find;
"""
METHODS = """- first, the how-tos earlier requests kept (kit calls that worked, the director's word for
  it): python tools/blender/model_requests.py methods lists them, each a file in
  private/requests/.lessons/methods/: read the ones that apply, and keep them in your file as
  they are, with their source (the web is for what the library lacks);
- research online how to do the steps the plan calls for that are unusual, in Blender or the
  kit (tools/blender/request_kit.py: read what it offers first; requests.md's "Accurate",
  "Parts", "The wind"): how scales are laid as cards, how a mane or feathers are carded, how
  a jaw, a wing or a tail is rigged, how a fin or a cloth bends, how a marking is painted,
  how a surface is sculpted to a reference: for each, find a source that says, read it, and
  keep the gist, worked into the kit's own calls where it maps onto them. A picture that
  shows a technique is kept as the references researcher keeps pictures, at most three;
- the style: the game's characters aim at the anime style of {games}: look up how those
  games model their characters (proportions, faces and eyes, hair as cards or strands,
  cloth), shade them (toon ramps, outlines) and rig them (faces, hair, cloth), and what of
  it the kit can do (requests.md, "The toon look");
- write private/requests/{id}/plan/research/methods.md: each how-to a few lines, in the order
  the stages will need them, with its source; what you couldn't find;
"""
ROLE_CHARGE = {"subject": SUBJECT, "references": REFERENCES, "methods": METHODS}
RESEARCH_END = RULES.replace("{end}", "End with one line saying what you found.")
# one researcher's run (here: the three at once)
RESEARCHER_RUN = (HEAD + STAGED + RESEARCHER + "Look the design up online, for the stages after you (it's started already):\n"
                  + READ + "{directed}{stage_lessons}" + RESEARCH_READ + "{charge}" + RESEARCH_END)
# in the cloud, one session does the three in turn
RESEARCH_CLOUD_RUN = (HEAD + STAGED + """In the cloud this stage's three researchers run one after another, in this one session, you
being each in turn ({roles}): each writes its own file, private/requests/{id}/plan/research/ROLE.md,
and "[research:ROLE] " lines in plan/notes.log. Pass {pass_} of at most 2.{more}

Look the design up online, for the stages after you (it's started already):
""" + READ + "{directed}{stage_lessons}" + RESEARCH_READ + "- as the subject researcher:\n" + SUBJECT + "- as the references researcher:\n" + REFERENCES
                      + "- as the methods researcher:\n" + METHODS + RESEARCH_END)
# what earlier requests taught a stage (requests.md, "What the stages learn"): read first
STAGE_LESSONS_READ = """- what earlier requests taught this stage ({path}; dated, a request each: measure rather
  than take their numbers):
{lines}
"""
# the director's grades against the user's ratings so far
CALIBRATED = """- your grades against the user's ratings of the finished models so far ({path}): where
  the user rated higher than you graded, you've been harsh; lower, lenient; grade accordingly:
{lines}
"""
# what a stage sent back by the director is told ({notes}: the director's, indented)
DIRECTED = """- the director judged the {judged} stage's work {grade} of 100 (under the {needed} needed) and sent
  it back to this stage, with this to fix:
{notes}
  Fix that first, then do the stage over as below, its files written afresh;
"""
# the director: a run after each stage, judging its work against the artwork; under the grade
# needed the work goes back to the stage that caused the fault, its run told what to fix
STAGED_DIRECTOR = """This request is made in stages, a run of Claude each with one charge, the editor starting the
next once the last has ended (requests.md, "Made in stages"): {stages}; after each, the
director judges its work (this run). The stages share private/requests/{id}/plan/.

"""
DIRECTOR_CRITERIA = {
    "study": ("the lines: each outline on its picture's silhouette (the overlap on lines_PICTURE.png), the parts' lines on the parts; "
              "the front and the side (profiles.png) true to the pictures' proportions (measure: heights in heads, widths against "
              "heights); plan/shape.md: every part the pictures show, none invented, the measurements right, the design told from the drawing"),
    "breakdown": ("every contoured element of the study has its enhanced cutout, its precise contour sitting on the enhanced edge "
                  "(straight runs straight, the corners listed where the edge turns, curves smooth: look at a few NAME_lines.png against "
                  "their NAME.png) and its analysis true to the picture (measure a few: a straight edge called curved, a metal called "
                  "cloth, a proportion off by a head sends it back); plan/breakdown.md complete, with the style; plan/research_plan.md "
                  "asking for each material and finish, each missing view and the style's how-tos"),
    "research": ("the subject found is the design's own (not another creature or thing of a like name); the facts true to the pictures "
                 "(the pictures and the user's notes win over the web); the references show what the pictures lack, of the right "
                 "thing (look at each in plan/refs/); the how-tos map onto the kit; the three files complete, their sources given"),
    "review": ("plan/research.md keeps what's right and drops what misleads; a second pass's questions (plan/research/more.md) are "
               "the gaps that matter; nothing the pictures contradict is kept"),
    "look": ("each colour region measured off the pictures (python tools/blender/model_requests.py colours {id} PICTURE X0 Y0 X1 Y1 "
             "checks one), its lit tone, on the right part, its borders where the pictures put them; materials.py's COLOURS and PAINT "
             "as plan/look.md has them"),
    "detail": ("every detached part and detail the pictures show is planned (teeth, claws, tufts, a mane, a crest; on a clothed "
               "character, what stands off the body: a jacket's tails, hair, accessories), none invented, each on the right part with "
               "the right bone, within the budget"),
    "merge": ("plan.md is whole and consistent: every part once, each with its shape, size, place, colours and bone; the research's "
              "facts folded in; nothing the pictures show missing; the block-out and the sculpting right for the style"),
    "body": ("the renders against the pictures (compare.png: the lines on the model and the overlap numbers; the previews): the "
             "silhouette, the proportions and the masses match where the body stage owns them (on a clothed character, the figure in "
             "its fitted clothes; hair, what hangs or stands off the body and accessories are the dress stage's and don't count "
             "against the body), the colour regions sit right; a primitive left showing through unchanged isn't accurate"),
    "dress": ("the detached parts and the detail on the renders against the pictures: the eyes' size and place, the ears, the horns, "
              "the fur's density and length, the markings; nothing missing, nothing the pictures don't show"),
    "assemble": ("the finished model: preview_idle.png and preview_roar.png (nothing torn, the jaw turning the right way, the lips "
                 "meeting at rest), the last compare.png, the prop added (temporary/ or content/) as designed, the reply honest "
                 "about what was guessed"),
}
DIRECTOR_RUN = (HEAD + STAGED_DIRECTOR + """You are the director of this request, made in stages: after each stage you judge its work
against the artwork (the user's pictures and notes: the design, never the drawing's light),
for how accurate it is. Judged now: the {JUDGED} stage.
""" + READ + "{stage_lessons}{calibrated}" + """- read what the stages have made so far, as far as they go: plan/shape.md, plan/research/,
  plan/research.md, plan/look.md, plan/detail.md, plan.md, lines.json and the lines drawn
  (lines_PICTURE.png, profiles.png), materials.py, body.py, detail.py, model.py, the renders
  (compare.png, preview_*.png), and every line of plan/notes.log; your earlier judgments are
  in plan/director/;
- judge the {judged} stage's work ({files}): {criteria}. Measure, don't guess: zoom into the
  pictures, compare the numbers, look at every render and reference;
- grade it 0 to 100 (100: exactly as the artwork and the notes have it; {needed} and over goes
  on, under {needed} the work goes back). When it goes back, decide which stage starts over:
  the earliest whose work caused the fault ({before}), the stages after it following;
- write private/requests/{id}/plan/director/{judged}.md: its first line "grade: N", its second
  "verdict: go on" or "verdict: back to STAGE", then what's right, then each fault (where it
  shows: which picture, line, render or file; what it should be, measured; how to fix it), and
  why that stage. The stage sent back reads it: be exact and complete;
- append a line to plan/notes.log: "[director] {judged}: N/100, go on", or "[director]
  {judged}: N/100, back to STAGE: why";
- change no other file.
""" + RULES.replace("{end}", "End with one line: the grade and the verdict."))
# the retrospective, after the last stage: what the request taught, kept for the next ones
RETROSPECT_RUN = (HEAD + """This request was made in stages, each a run of Claude with one charge, a director judging
each against the artwork (requests.md, "Made in stages"), and it's finished. You are its
retrospective: what it taught, kept for the next requests (requests.md, "What the stages
learn"). Nobody answers questions: decide.
""" + READ + """- read every line of plan/notes.log and plan/director.log, every plan/director/*.md (the
  grades and faults, stage by stage, and what was sent back), plan.md, the stages' files
  (plan/shape.md, plan/research/, plan/research.md, plan/look.md, plan/detail.md,
  materials.py, body.py, detail.py, model.py), the renders (compare.png, preview_*.png) and
  the request's reply; and what the stages' lessons say already (private/requests/.lessons/
  stages/*.md: don't repeat them);
- for each stage, what this request taught it that the next requests' runs of it should
  know: at most three lines a stage, each a fact of method and not of this model (what to
  measure first, a kit call that did the job, a fault the director found that the stage
  should check itself, a step that wasted a run); a stage that taught nothing gets none; and
  for the director, what to check first next time. Each line:
  python tools/blender/model_requests.py lesson {id} STAGE "the line"
  (STAGE: study, research, review, look, detail, merge, body, dress, assemble, or director);
- the how-tos that worked (the director's grades of the body, the dress and the assembly say
  which: {needed} and over): each a short file written in the request's folder (a heading,
  when it applies, the kit calls as written, 10 to 30 lines), then kept in the library:
  python tools/blender/model_requests.py method {id} "topic" --file private/requests/{id}/plan/method_NAME.md
  (python tools/blender/model_requests.py methods lists the topics kept: a topic given its
  exact name again replaces it, when this request did it better); at most four;
- the reference pictures worth keeping for requests of the same kind of thing (plan/research.md
  says what each shows; none of the user's own pictures):
  python tools/blender/model_requests.py keep-ref {id} FILE "what it shows, of what"
  at most six;
- change no file of the request but the method files you write.
""" + RULES.replace("{end}", "End with one line: how many lessons, how-tos and references you kept."))
# the lead's review of the research: on the first pass, more asked when something's missing
# (the researchers run again on plan/research/more.md), else compiled; the second compiles
REVIEW_FIRST = """- then, this being the first pass: when something the model needs is still missing (a view
  no reference shows, a fact unsure, a how-to unknown) and the web would likely have it, write
  private/requests/{id}/plan/research/more.md, a section per researcher ("## subject",
  "## references", "## methods") with what each should find, and stop there: the three run
  again on it, and you review again. Otherwise write plan/research.md as below;
"""
REVIEW_LAST = """- then, this being the second pass (the last: no more research after it), write
  plan/research.md as below;
"""
REVIEW_RUN = (HEAD + STAGED + "Review the research (pass {pass_} of at most 2; it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan/shape.md and every line of plan/notes.log, then the researchers' files:
  plan/research/subject.md, plan/research/references.md and plan/research/methods.md, and
  look at every picture in plan/refs/ beside the user's pictures;
- check them: each reference against the design (does it show the view or the detail it was
  kept for, of the right kind of thing? one that misleads is left out), each fact against the
  pictures and the user's notes (the notes win), each how-to against the kit (does the kit do
  it, and how); each question of plan/research_plan.md answered, or said unanswerable; and
  what's still missing for a model as detailed as the pictures and the style allow;
{review_end}- private/requests/{id}/plan/research.md, the one research the stages after you read: what
  the design is, in a line; the facts kept, each with its source; the references kept, each
  with its file in plan/refs/ (or its address), what it shows and when a builder should look
  at it; the how-tos kept, in the order the stages need them; what stays unknown, decided by
  you;
- append to plan/notes.log what you dropped and why, and what stays unknown, "[review] "
  lines;
""" + RULES.replace("{end}", "End with one line saying what you decided."))
LOOK_RUN = (HEAD + STAGED + "Plan its colours and its materials (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan/shape.md (the parts, as the study stage planned them, by name), plan/research.md
  (what the research stage found online: the colours such a thing has in life, its
  references in plan/refs/: look at them) and plan/notes.log, and look at the lines the study
  stage made (lines_PICTURE.png, profiles.png);
""" + BREAKDOWN_READ + """- tell the design's colours from the drawing's light (requests.md, "The design, not the
  drawing"): a part's colour is its lit tone, neither its shine nor its shade; shine and
  reflections make a glossy material, a glow a glowing one. Measure each region off the
  pictures: python tools/blender/model_requests.py colours {id} PICTURE X0 Y0 X1 Y1 lists a
  box's main colours (zoom shows a part closer);
- write private/requests/{id}/plan/look.md: each colour region with its hex and the part it's
  on (shape.md's names), where its border runs (in cells, from the profiles); the markings and
  the patterns (a belly, a mask, spots, stripes, the eyes, the teeth, the claws), each as a
  paint volume (its shape, size and place in cells, on which part); what's glossy, matte or
  glowing; what's too fine to model and would be a texture painted from the pictures;
- write private/requests/{id}/materials.py, a module the build stages import with
  kit.load("materials"): COLOURS, a dict of each region's name to its hex (or a material dict,
  as kit.part and kit.wrap take colours); PAINT(kit), a function returning a dict of each
  part's name to the list of paint volumes for its markings, each a kit shape with its region
  (as paint= takes them); and nothing else: it builds no part (the body and dress stages do);
- append to plan/notes.log what you guessed and what you'd change in shape.md (don't change
  it: the merge stage decides), "[look] " lines;
""" + RULES.replace("{end}", "End with one line saying what you planned."))
DETAIL_RUN = (HEAD + STAGED + "Plan its detached parts and its detail (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan/shape.md and plan/look.md (the parts and the colours, by name), plan/research.md
  (what the research stage found online: how the fur, scales or feathers lie, the how-tos,
  its references in plan/refs/: look at them) and plan/notes.log, and look at the lines
  (lines_PICTURE.png, profiles.png);
""" + BREAKDOWN_READ + """- write private/requests/{id}/plan/detail.md: the detached parts, each named (on a clothed
  character, what stands off the body: a jacket's tails, a cape, a skirt's flare, hair and a
  ponytail, straps, accessories; the clothes that follow the body are the body's skin, the
  body stage's), with its shape,
  its size and place in cells, its colour regions (look.md's) and the bone or chain it follows
  (shape.md's chains: the lower jaw with its teeth and the tongue on the jaw's and the
  tongue's, the eyes and the lids, claws, horns, plates, wings' spars, a prop's lid or door
  on a bone of its own; requests.md, "Parts"); then the detail the body's skin can't carry,
  grown on it where the pictures show it (requests.md, "Accurate", 4): tufts of fur,
  feathers, scales, leaves, spines, a mane, hair, cloth as kit.cards (the region they root
  on, their count, size, bend, lean, along, colour), a coat of strands as kit.fur (region,
  count, length, droop), each where it roots and how dense; what the wind moves (kit.wind,
  the strands, kit.plant_wind for a plant) and what stays still; and what it all costs
  against the budget (the style's triangles);
- append to plan/notes.log what you guessed and what you'd change in shape.md or look.md
  (don't change them: the merge stage decides), "[detail] " lines;
""" + RULES.replace("{end}", "End with one line saying what you planned."))
MERGE_RUN = (HEAD + STAGED + "Bring the plans together into one (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan/shape.md, plan/research.md, plan/look.md and plan/detail.md, and every line of
  plan/notes.log (the stages' guesses, doubts and what each would change in the others'), and
  look at the lines (lines_PICTURE.png, profiles.png), the references in plan/refs/ and
  materials.py;
""" + BREAKDOWN_READ + """- check they agree: every part named in one plan is in the others under the same name, each
  colour region sits on a part, each detached part has its bone or chain, nothing is planned
  twice (a horn as a part and as cards), nothing the pictures show is left out, the budget
  holds. Decide each disagreement by the pictures and the user's notes;
- write private/requests/{id}/plan.md, the one plan the build stages follow, whole: the design
  and the drawing sorted (from shape.md); the research's facts that bear on the build, and the
  references in plan/refs/ a builder should look at, named; the parts in the order they're built, each with its
  shape, size and place in cells, its colours and paint, its bone; the body's block-out and
  how it's sculpted to the profiles (accurate), or its parts (smooth, low-poly); the
  materials; the detail, where it roots and how dense; the rig's chains with their joints in
  cells, and the actions (idle, roar...); the budget. A build stage reads plan.md and the
  pictures, and no other plan: everything it needs is in it;
- append to plan/notes.log each disagreement and how you decided it, "[merge] " lines; leave
  lines.json, materials.py and the plan/ files as they are;
""" + RULES.replace("{end}", "End with one line saying what you decided."))
BODY_RUN = (HEAD + STAGED + "Build its body on the plan (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan.md (whole: follow it) and plan/notes.log, and materials.py (its COLOURS and
  PAINT: use them, change nothing in it), and look at the lines (lines_PICTURE.png,
  profiles.png; job.profiles() has them in cells, as the model will stand) and at the
  references plan.md names in plan/refs/; the how-tos plan.md or plan/research.md name are
  in private/requests/.lessons/methods/ (python tools/blender/model_requests.py methods
  lists them): kit calls that worked before, to build on;
""" + BREAKDOWN_READ + """{lessons}- write private/requests/{id}/body.py, a module model.py imports with kit.load("body"):
  def build(job, kit, materials) builds the body as plan.md says and returns its objects:
  the skin (accurate: the block-out laid for every mass, kit.wrap over them all, sculpted to
  the profiles; requests.md, "Accurate", 1 to 3), or its parts (smooth, low-poly: the torso,
  the neck, the head with the upper jaw, the limbs, the tail, fleshy ears, a kit.part each
  with its bone), coloured with materials.COLOURS and painted with materials.PAINT(kit). On a
  clothed character the body is the figure as the pictures' silhouette has it: the clothes
  that follow the body (a bodysuit, trousers, a shirt, boots) are the body's skin, in their
  colours, their seams and stripes painted; only what stands off the body (a jacket's tails,
  a cape, a skirt's flare, hair and a ponytail, straps, accessories) is the dress stage's.
  No detached part, no card, no fur, no rig: the dress and assemble stages add those. When
  body.py and model.py are there from an earlier run of this stage, change them (Edit) rather
  than start over; model.py needs no change when it still fits;
- write private/requests/{id}/model.py (requests.md, "The kit": the usual start), then
  materials = kit.load("materials"); body = kit.load("body").build(job, kit, materials);
  kit.fit(body, job.height) (or as the settings above fit it); files = kit.previews(job,
  body); kit.compare(job, files). No finish yet: the assemble stage adds it;
- render it: python tools/blender/run_request.py {id}
- compare private/requests/{id}/compare.png and the previews with the pictures (the shapes,
  proportions, masses and colour regions, not the light: the previews are lit evenly, on
  purpose); its last row has the lines on the model and how far each outline and the model
  overlap (aim for about 85% or more where the poses match). Change body.py and render again
  until the body matches (usually three to six tries): a primitive left showing through
  unchanged isn't accurate;
- append to plan/notes.log what you changed from the plan and why, and what the dress stage
  should know (the body's objects' names, their regions), "[body] " lines;
""" + RULES.replace("{end}", "End with one line saying what you built."))
DRESS_RUN = (HEAD + STAGED + "Dress the body built: its detached parts and its detail (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan.md and plan/detail.md (whole: follow them) and plan/notes.log (the body stage's
  lines say what it changed), materials.py and body.py (read them, change nothing in them)
  and model.py, and look at the renders (preview_*.png, compare.png), the lines and the
  references plan.md names in plan/refs/; the how-tos plan.md or plan/research.md name are in
  private/requests/.lessons/methods/ (python tools/blender/model_requests.py methods lists
  them): kit calls that worked before, to build on;
""" + BREAKDOWN_READ + """- write private/requests/{id}/detail.py, a module model.py imports with kit.load("detail"):
  def build(job, kit, materials, body) adds to the body built and returns what it adds: the
  detached parts as kit.part parts overlapping the body (the lower jaw with its teeth and the
  tongue, the eyes with kit.eyes, the lids, claws, horns, plates, a prop's lid or door), each
  with bone= or bones= as plan.md says; then the detail grown on the body where the pictures
  show it (requests.md, "Accurate", 4): kit.cards for tufts of fur, feathers, scales, leaves,
  spines, a mane, cloth; kit.fur for a coat of strands; paint for the markings the body stage
  left; on a clothed character, what stands off the body (a jacket's tails, a cape, a skirt's
  flare, hair and a ponytail, straps, accessories: the fitted clothes are the body's); and
  what the wind moves painted for it (kit.wind; a plant's parts in ONE kit.plant_wind call),
  as plan.md and detail.md say, within the budget;
- in model.py, after the body is built and fitted: detail = kit.load("detail").build(job,
  kit, materials, body), then fit again when the detail changed its height (kit.fit(body +
  detail, job.height)), then the previews and the compare on body + detail; change nothing
  else in it;
- render it: python tools/blender/run_request.py {id}
- compare compare.png and the previews with the pictures: the details carry the character
  (the eyes' size and place, the ears, the horns, the fur's density and length); change
  detail.py and render again until they match;
- append to plan/notes.log what you changed from the plan and why, "[dress] " lines;
""" + RULES.replace("{end}", "End with one line saying what you added."))
# the last stage: a character rigged ({assemble}: RIGGED), the model finished
RIGGED = """rig it{rig}: every part on the
  bones that move it, as plan.md's chains say, the jaw's and the tongue's chains added (and
  the eyes' and the lids' when they move); give it its idle and its roar (kit.idle,
  kit.action), render and look at preview_idle.png and preview_roar.png: nothing torn, the
  jaw turning the right way, the lips meeting at rest (requests.md, step 9); then """
ASSEMBLE_RUN = (HEAD + STAGED + "Put it together, rig it and finish it (it's started already):\n" + READ + "{directed}{stage_lessons}" + """- read plan.md and every line of plan/notes.log (each stage's guesses and changes), body.py,
  materials.py, detail.py and model.py, and look at the renders (preview_*.png, compare.png)
  and the lines;
- in model.py: the body and the detail built and fitted as they are; {assemble}the previews,
  and kit.finish(job, kit.parts(), files, reply=...) (with the category and the settings its
  kind asks, above); its reply for the user: what was guessed (the notes say), what to look at;
- render it: python tools/blender/run_request.py {id}, and compare: when something's off that
  another stage owns (a part's shape in body.py, a colour in materials.py, the fur in
  detail.py), fix it there, the small fixes that finish it, never a remake, and say so in
  plan/notes.log ("[assemble] " lines);
{variants}- add it to the project: python tools/blender/run_request.py {id} --finish
- check: cargo run -q -p engine-content --bin content -- check
""" + RULES.replace("{end}", MADE))
STAGE_RUNS = {"breakdown": BREAKDOWN_RUN, "review": REVIEW_RUN, "look": LOOK_RUN, "detail": DETAIL_RUN, "merge": MERGE_RUN, "body": BODY_RUN, "dress": DRESS_RUN, "assemble": ASSEMBLE_RUN}
# a set of floor tiles made again: the same, its tiles changed in the last run's model.py
AGAIN_FLOOR_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, a set of floor tiles, made again from what\n"
                   "the last run made, with the user's comments on it (requests.md, \"Made again, with the user's comments\"\n"
                   "and \"A set of floor tiles\"; it's started already):\n" + READ + "{directed}{stage_lessons}" + AGAIN
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


def allowed(rid, stage=None):
    """What the run may do without asking; everything else is refused. The research stage
    alone reads the web (model_requests.py fetch brings its pictures in)."""
    here = f"private/requests/{rid}"
    out = [
        "Read", "Glob", "Grep",
        f"Write({here}/**)", f"Edit({here}/**)",
        f"Bash(python tools/blender/run_request.py {rid})", f"Bash(python tools/blender/run_request.py {rid} *)",
        "Bash(python tools/blender/model_requests.py *)",
        # a helper script of its own, in its folder (one that writes lines.json from numbers, say),
        # a one-liner, and a folder made there
        f"Bash(python {here}/*)", "Bash(python -c *)", "Bash(python - *)", f"Bash(mkdir {here}/*)", f"Bash(mkdir -p {here}/*)",
        "Bash(cargo run -q -p engine-content --bin content -- check)",
        "Bash(ls)", "Bash(ls *)",
    ]
    if stage == "research":
        out += ["WebSearch", "WebFetch"]
    return out


STYLES = {
    "smooth": "Style: smooth: rounded shapes, smooth shaded (parts with smooth=True, about 6000 triangles in all).",
    "low_poly": "Style: low-poly: few faces, flat shaded (parts with smooth=False, about 800 triangles in all).",
    "accurate": ("Style: accurate: make it match the pictures as closely as you can, whatever that takes: every part, marking and colour "
                 "of the design (none of the drawing's light, shade or lines), the proportions measured off the pictures. The body is made in "
                 "one skin, in this order (requests.md, \"Accurate\"): 1. block it out: the primitives laid out for every mass (torso, neck, "
                 "head with the upper jaw, limbs, tail, fleshy ears), a scaffold that goes after; 2. kit.wrap(blockout, name, colours) wraps "
                 "one fresh surface over them all, melted, subdivided, dense; 3. sculpt that skin to the profiles (kit.sculpt: grab, "
                 "inflate, flatten, smooth, crease; kit.editing), measuring against the lines, until it matches the pictures' proportions "
                 "and not just the scaffold: a primitive left showing through unchanged isn't accurate; 4. what stays detached (the lower "
                 "jaw, teeth, tongue, eyes, lids, claws, horns, plates) as kit.part parts overlapping it, and the detail the skin can't carry "
                 "grown where the pictures show it as curved cards or meshes: kit.cards for tufts of fur, feathers, scales, leaves (kit.fur "
                 "for a coat of strands), painted colours for patterns. A bigger budget (20000 triangles or more in all). It may take a long "
                 "time: that's fine."),
}
# the agentic style: accurate, made in stages by agents
STYLES["agentic"] = STYLES["accurate"].replace("Style: accurate:", "Style: agentic (accurate, made in stages by agents: requests.md, \"Made in stages\"):", 1)
LINES = {
    "smooth": "Lines: the outline and the main parts, over the pictures and in the front and the side.",
    "low_poly": "Lines: the outline and the main parts, over the pictures and in the front and the side.",
    "accurate": ("Lines: every part, marking and detail, each where the pictures put it, over the pictures and in the front and "
                 "the side too: the model is only as close as these are."),
}
LINES["agentic"] = LINES["accurate"]
# a set of floor tiles: laid on many cells, so each tile light
FLOOR_STYLES = {
    "smooth": "Style: smooth: softly rounded edges, smooth shaded; each tile light, as a floor is laid on many cells (about 400 triangles a tile).",
    "low_poly": "Style: low-poly: few faces, flat shaded (about 100 triangles a tile).",
    "accurate": ("Style: accurate: the pattern as close to the pictures as it can be: every stone, plank, joint, vein and inlay where the "
                 "pictures put it, the colours measured off them (none of the drawing's light or shade), an image texture painted from the "
                 "picture for what's too fine to model; each tile still light, as a floor is laid on many cells (about 1500 triangles a tile)."),
}
FLOOR_STYLES["agentic"] = FLOOR_STYLES["accurate"].replace("Style: accurate:", "Style: agentic (accurate):", 1)
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
INSTRUCTIONS = "The user's instructions (for every kind, then its kind's and its variety's; a feature each: follow every one):"
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
    if record["style"] in ("accurate", "agentic") and record["details"].strip():
        out.append("Model details from the user: " + record["details"].strip())
    if not floor:
        out.append(LINES.get(record["style"], LINES["smooth"]))
    if record["kind"] == "character":
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
    said = mr.instructions(record, mr.catalogue(root))
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


def part_of(record, go_on=False, again=False, variants=False, stage=None):
    """Which part of the recipe a run does: "all" of it; "lines", up to the front and the side,
    for a request that checks its lines first; "model", from the model on, once they're checked;
    "again", the model the last run made, made again with the user's comments on it;
    "variants", the model made kept and its variants made; "split", a request whose pictures
    hold several of the same kind of asset, each made a request; a `stage` of a request made
    in stages, that stage (model_requests.py's STAGES)."""
    if stage:
        return stage
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


def prompt(record, go_on=False, note="", again=False, variants=False, root=None, stage=None, role=None, cloud=False, direct=False, retrospect=False):
    """What Claude is asked for the request: the recipe, or its part (part_of), with what the
    user says is off in the lines (`note`, from the window's check) fixed first; made again
    (`again`), what the user says of the last run's model (`note`, else the record's comments);
    `variants`, its variants made from its model; a `stage` of a request made in stages, that
    stage's charge (the study's, `go_on`: the shape planned on the lines the user checked; the
    research's, `role`: one of its researchers, else the three in turn, as the `cloud` does;
    `direct`: the director's judgment of that stage's work; `retrospect`: what the finished
    request taught, kept). A
    request that asks for variants has them made with its model. With the project (`root`), a
    run that builds the model reads first what the ratings teach of its kind, when they teach
    anything yet."""
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
    part = part_of(record, go_on, again, variants, stage)
    shapes = mr.outlined(record)
    if retrospect:
        template = RETROSPECT_RUN
    elif direct:
        template = DIRECTOR_RUN
    elif stage == "study":
        template = STUDY_GO_ON_RUN if go_on else STUDY_LINES_RUN if record["check_lines"] else STUDY_RUN
    elif stage == "research":
        template = RESEARCHER_RUN if role and not cloud else RESEARCH_CLOUD_RUN
    elif stage:
        template = STAGE_RUNS[stage]
    else:
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
    elif kind == "character":
        what = f"a character {height} cells tall, rigged, with an idle animation"
    else:
        what = f"a {kind} {height} cells tall"
    # asked as a kind of the window's own (a kind the user added), or a variety of one: named so
    k, v = mr.kind_of(record, mr.kinds(root))
    named = (k or {}).get("name", "").strip()
    # (the generic variety, named after its category's making, says nothing more)
    variety = (v or {}).get("name", "").strip().lower()
    if variety and variety != kind and (v or {}).get("id") != kind:
        what = f"a {variety}: {what}"
    elif v is None and k is not None and k.get("id") != kind and named:
        what = f"a {named.lower()}: {what}"
    count = record["variants"]
    vary = vary_text(record)
    # the variants made with the model (a run that makes it), as the settings ask
    step = VARIANT_STEP.format(count=count, vary=vary, last=count + 1) if count > 0 and kind != "floor" else ""
    made = f" (with its variants {', '.join(record['variant_props'])})" if record["variant_props"] else ""
    # a stage's words: which it is, what it's charged with; a character's assembly rigs it
    k = mr.STAGES.index(stage) + 1 if stage else 0
    # the research's researchers and its pass, the review's
    second = int(record.get("research_pass") or 1) >= 2
    fetching = (CLOUD_FETCH_HOW if cloud else FETCH_HOW).format(id=rid)
    charge = ROLE_CHARGE.get(role, "").format(id=rid, fetching=fetching, games=STYLE_GAMES)
    # the director's: the stage it judges; a stage it sent back: what it said
    j = record.get("judgment") or {}
    directed = ""
    if stage and not direct and j.get("to") == stage and str(j.get("notes") or "").strip():
        directed = DIRECTED.format(judged=j.get("stage", ""), grade=j.get("grade", 0), needed=mr.GRADE_NEEDED, notes=indented(str(j["notes"])))
    judged = stage if direct else ""
    files = ", ".join(mr.STAGE_FILES.get(judged, ())) or "the prop it added, its record and its .glb, and its renders"
    # what earlier requests taught the stage (the director: its own lessons, and its grades
    # against the user's ratings)
    taught = mr.stage_lessons(root, "director" if direct else stage) if root is not None and (stage or direct) else []
    stage_lessons = STAGE_LESSONS_READ.format(path=(mr.STAGE_LESSONS / f"{'director' if direct else stage}.md").as_posix(), lines="\n".join("  " + line for line in taught)) if taught else ""
    graded = mr.calibration(root) if root is not None and direct else []
    calibrated = CALIBRATED.format(path=mr.CALIBRATION.as_posix(), lines="\n".join("  " + line for line in graded)) if graded else ""
    staged = dict(stages=", ".join(mr.STAGES), STAGE=(stage or "").upper(), stage=stage or "", k=k, n=len(mr.STAGES), about=mr.STAGE_ABOUT.get(stage, ""),
                  assemble=RIGGED.format(rig=rig) if kind == "character" else "", staged=WAS_STAGED if record.get("stages") else "",
                  ROLE=(role or "").upper(), role=role or "", roles="the subject, the references, the methods", pass_=2 if second else 1, more=MORE if second else "",
                  fetching=fetching, charge=charge, review_end=REVIEW_LAST if second else REVIEW_FIRST.format(id=rid), directed=directed,
                  judged=judged, JUDGED=judged.upper(), files=files, needed=mr.GRADE_NEEDED, criteria=DIRECTOR_CRITERIA.get(judged, "").format(id=rid), games=STYLE_GAMES,
                  before=", ".join(mr.STAGES[:mr.STAGES.index(judged) + 1]) if judged else "",
                  folder=Path(root).resolve().as_posix() if root is not None else "the project folder", stage_lessons=stage_lessons, calibrated=calibrated)
    return template.format(id=rid, name=record["name"], what=what, height=height, notes=record["notes"].strip() or "(none)",
                           settings=settings_lines(record, root), checked=checked, noted=noted, bones=bones, rig=rig,
                           prop=record["prop"] or "(none)", reply=indented(record["reply"]) or "    (nothing)", earlier=earlier, said=said,
                           variants=step, count=len(shapes) if part == "split" and shapes else count, vary=vary, made=made,
                           outlined=outlines_said(record, shapes), lessons=lessons, **staged)


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


def unrunnable(root, rid, record, part, go_on, note, again, variants, say, direct=False):
    """What keeps a run from starting (the request's status then), or None: nothing to go on
    from, to make again from, to change or to vary yet, or split already; a stage without the
    files of the stages before it."""
    here = mr.folder(root, rid)
    if go_on and not (here / "lines.json").is_file():
        return mr.fail(root, rid, "nothing to go on from: no lines.json yet (Start draws the lines first)")["status"]
    if part in mr.STAGES:
        for before in mr.STAGES[:mr.STAGES.index(part) + (1 if direct else 0)]:
            for name in mr.STAGE_FILES[before]:
                if not (here / name).is_file():
                    return mr.fail(root, rid, f"nothing to {'judge' if direct else 'build on'}: no {name} yet (the {before} stage writes it: run it first)")["status"]
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


def announce(record, part, note, say, go_on=False, direct=False, retrospect=False):
    """What the run is, in its log: its part of the recipe (or its stage, or the director's
    judgment of one, or the retrospective), what the user said, their bones."""
    if retrospect:
        say("This run: the retrospective: what this request taught its stages, kept for the next requests (private/requests/.lessons/)")
    elif direct:
        say(f"This run: the director judges the {part} stage's work against the artwork, 0 to 100 (under {mr.GRADE_NEEDED} it goes back to the stage at fault)")
    elif part in mr.STAGES:
        k = mr.STAGES.index(part) + 1
        how = (" (the lines first, for the user to check)" if part == "study" and record["check_lines"] and not go_on
               else " (the shape planned on the lines the user checked)" if part == "study" and go_on else "")
        say(f"This run: the {part} stage, {k} of {len(mr.STAGES)}: {mr.STAGE_ABOUT[part]}{how}")
    elif part == "lines":
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


def read_verdict(text):
    """A director's file: its grade (0 to 100), the stage it sends the work back to ("" when it
    goes on), and its notes (the rest); None without a grade or a verdict line."""
    grade = re.search(r"^\s*grade:\s*(\d+)", text, re.IGNORECASE | re.MULTILINE)
    verdict = re.search(r"^\s*verdict:\s*(go on|back to\s+([a-z]+))", text, re.IGNORECASE | re.MULTILINE)
    if not grade or not verdict:
        return None
    to = (verdict.group(2) or "").lower()
    notes = "\n".join(line for line in text.splitlines() if not re.match(r"^\s*(grade|verdict):", line, re.IGNORECASE)).strip()
    return min(100, int(grade.group(1))), to, notes


def conclude(root, rid, part, began, again, note, result, returncode, error, claude, say, go_on=False, direct=False, retrospect=False):
    """A run's end, by what's on disk and what Claude said last (`result`: its result event, or
    {}; `error`: an error its messages had): done (split: its requests to make), waiting for the
    user's check of its lines, a stage done (its files written: the editor starts the next), or
    failed and why. The request's status."""
    here = mr.folder(root, rid)
    record = mr.load(root, rid)
    last = str(result.get("result") or "").strip()
    stage = part if part in mr.STAGES else None
    if retrospect:
        # over, whatever it kept (its lines are in the library and the log): the request done
        # again; what it said last, the console's
        if last:
            say(f"The retrospective says: {last}")
        return mr.retrospected(root, rid, last.splitlines()[0][:160] if last else "")["status"]
    if part == "split" and record["children"] and returncode == 0 and not result.get("is_error"):
        # split: done, its requests to make
        p = dict(record["progress"])
        took = (time.time() - p["started"]) / 60.0 if p["started"] > 0 else 0.0
        p.update(step=f"split into {len(record['children'])} requests", done=1.0, minutes=float(round(took, 1)))
        mr.update(root, rid, status="done", progress=p, **({"reply": last} if last else {}))
        say(f"Split into {len(record['children'])} requests: {', '.join(record['children'])}")
        return "done"
    if record["status"] == "done" and not direct:
        say(f"Done: the prop {record['prop']}" + (f", and its variants {', '.join(record['variant_props'])}" if record["variant_props"] else ""))
        if stage:
            # (the director judges the assembly too, when the request has one; then the
            # retrospective keeps what the request taught)
            following = f"direct:{stage}" if record.get("director", True) else "retrospect"
            mr.update(root, rid, stage=stage, stages=mr.stages_with(record, stage), next=following)
            say("The director judges the assembly next" if following.startswith("direct:") else "The retrospective next: what this request taught")
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
    elif stage == "study" and record["check_lines"] and not go_on:
        # where it was asked to stop: the user's turn (the stage isn't done: its plan comes
        # after their check)
        if drawn_since(here / "profiles.png", began):
            return mr.wait_for_check(root, rid)["status"]
        reason = "Claude ended without drawing the front and the side (profiles.png)"
    elif direct:
        # the director's word: its file, parsed; the work on, or back to the stage at fault
        path = here / "plan" / "director" / f"{stage}.md"
        verdict = read_verdict(path.read_text(encoding="utf-8")) if drawn_since(path, began) else None
        if verdict is not None:
            grade, to, notes = verdict
            judged = mr.directed(root, rid, stage, grade, to, notes)
            back = judged["judgment"]["to"]
            with open(here / "plan" / "director.log", "a", encoding="utf-8") as kept:
                kept.write(f"{mr.stamp()} {stage}: {grade}/100, {'back to ' + back if back else 'go on'}\n")
            return judged["status"]
        reason = f"the director ended without writing plan/director/{stage}.md" + (" with a grade and a verdict line" if path.is_file() else "")
    elif stage == "review" and int(record.get("research_pass") or 1) < 2 and drawn_since(here / "plan" / "research" / "more.md", began) and not drawn_since(here / "plan" / "research.md", began):
        # the lead asks for a second research pass: the researchers again, then this review
        return mr.review_more(root, rid)["status"]
    elif stage and stage != "assemble":
        # its files there, and one of them this run's (a stage run again changes what it must,
        # and may leave a file as it was)
        owned = mr.STAGE_FILES[stage]
        missing = [f for f in owned if not (here / f).is_file()]
        if not missing and any(drawn_since(here / f, began) for f in owned):
            return mr.stage_done(root, rid, stage)["status"]
        reason = (f"Claude ended without writing {', '.join(missing)} (the {stage} stage's)" if missing
                  else f"Claude ended without changing any of its files ({', '.join(owned)})")
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


def claude_once(root, rid, claude, env, args, text, say, stop, quiet, raw_name="run.jsonl", agent="Claude"):
    """One Claude Code on a prompt, followed to its end: what it did in the log (`say`), its
    whole events raw in `raw_name`. Its result event (or {}), its exit code, an error its
    messages had, what it cost, and how it was stopped: "cancelled" (the stop file), "quiet"
    (no word for `quiet` seconds), or None."""
    here = mr.folder(root, rid)
    extra = {} if os.name == "nt" else {"start_new_session": True}
    proc = subprocess.Popen(program(claude) + args, cwd=str(root), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace", bufsize=1, env=env, **extra)
    state = {"last": time.time(), "model": None, "result": None, "error": None}
    lock = threading.Lock()
    raw = open(here / raw_name, "a", encoding="utf-8", newline="\n")

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
                for said in describe(event, state):
                    say(said)
                # what it's doing now, for the editor's panel: its latest words
                if event.get("type") == "assistant":
                    words = [c["text"] for c in event.get("message", {}).get("content", []) if c.get("type") == "text" and c.get("text", "").strip()]
                    if words:
                        mr.now(root, rid, agent, words[-1])

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
    cost = (state.get("result") or {}).get("total_cost_usd")
    return {"result": state["result"] or {}, "returncode": proc.returncode, "error": state["error"], "cost": float(cost or 0.0), "ended": ended}


def run(root, rid, model=None, quiet=QUIET, go_on=False, note="", again=False, variants=False, stage=None, role=None, direct=False, retrospect=False):
    """The run, start to end; the request's status as it ended. `go_on`: on from the model, on
    the lines the user checked, `note` saying what's off in them. `again`: the model the last
    run made, made again as the user's comments say (`note`, else the record's). `variants`:
    variants of the model made, as many as the request says, the model itself kept. `stage`: a
    request made in stages, this stage of it (the editor runs them one after another); the
    research stage runs its three researchers at once, a Claude each (`role`: one alone);
    `direct`: the director's judgment of that stage's work."""
    record = mr.load(root, rid)
    here = mr.folder(root, rid)
    stop = here / "stop"
    if stop.exists():
        stop.unlink()
    began = time.time()
    part = part_of(record, go_on, again, variants, stage)
    if again:
        note = note.strip() or record["comments"].strip()

    def say(text):
        mr.log(root, rid, text)

    say("———————— the retrospective starts ————————" if retrospect else f"———————— the director judges the {stage} stage ————————" if direct else f"———————— the {stage} stage starts ————————" if stage else "———————— a run starts ————————")
    ended = unrunnable(root, rid, record, part, go_on, note, again, variants, say, direct)
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
    settings.write_text(json.dumps({"permissions": {"allow": allowed(rid, stage)}}, indent=2) + "\n", encoding="utf-8")
    args = ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-mode", "dontAsk", "--strict-mcp-config", "--no-session-persistence",
            "--settings", str(settings.relative_to(root))]
    # the agentic style's agents on Sonnet, its director on Opus, unless the caller says
    if (stage or retrospect) and not model:
        model = "opus" if direct else "sonnet"
    if model:
        args += ["--model", model]
    # the research's researchers at once, a prompt each; any other run, one prompt
    if stage == "research" and not role and not direct:
        texts = {r: prompt(record, root=root, stage=stage, role=r) for r in mr.ROLES}
    else:
        texts = {role: prompt(record, go_on, note, again, variants, root=root, stage=stage, role=role, direct=direct, retrospect=retrospect)}
    if retrospect:
        mr.retrospect_start(root, rid)
    elif direct:
        mr.direct_start(root, rid, stage)
    elif stage:
        mr.stage_start(root, rid, stage)
    else:
        mr.start(root, rid, mr.estimate(record, part))
    mr.step(root, rid, "Claude is starting", 0.05)
    say(f"Claude Code: {claude} ({version})")
    say(f"On: {on}")
    announce(record, part, note, say, go_on, direct, retrospect)
    say(f"Sent: claude {' '.join(args)}   (the prompt on its input; model: {model or 'its default'})")
    say("Allowed without asking: " + ", ".join(allowed(rid, stage)))
    for r, t in texts.items():
        say((f"Prompt for the {r} researcher:\n" if r else "Prompt:\n") + t)
    if len(texts) > 1:
        say("The three researchers at once, each its own Claude: their lines tagged [subject], [references], [methods]")
    outs = {}

    agent = "retrospective" if retrospect else "director" if direct else (stage or "Claude")

    def go(r, t):
        tagged = (lambda s, r=r: say(f"[{r}] {s}")) if r else say
        outs[r] = claude_once(root, rid, claude, env, args, t, tagged, stop, quiet, f"run_{r}.jsonl" if r else "run.jsonl", f"{agent}:{r}" if r else agent)

    workers = [threading.Thread(target=go, args=(r, t), daemon=True) for r, t in texts.items()]
    for w in workers:
        w.start()
    for w in workers:
        w.join()
    # what the run cost, on the request's current run: its stage's (the researchers' together;
    # the director's on the stage it judged; a plain run's by its part)
    cost = sum(o["cost"] for o in outs.values())
    if cost:
        named = {"all": "the model", "floor": "the tiles", "lines": "the lines", "model": "the model", "again": "made again", "variants": "the variants", "split": "the split"}
        mr.charge(root, rid, "retrospective" if retrospect else stage or named.get(part, part), cost, director=direct)
    endings = [o["ended"] for o in outs.values()]
    if "cancelled" in endings:
        stop.unlink(missing_ok=True)
        say("Cancelled from the editor: Claude stopped, the request waits again")
        mr.run_ended(root, rid, "waiting")
        return mr.update(root, rid, status="waiting", progress=dict(mr.FIELDS["progress"]))["status"]
    if "quiet" in endings:
        who = [r for r, o in outs.items() if o["ended"] == "quiet"]
        whom = "Claude" if who == [None] else "the " + " and the ".join(f"{w} researcher" for w in who)
        return mr.fail(root, rid, f"no word from {whom} for {max(1, round(quiet / 60))} min, so it was stopped")["status"]
    if None in outs:
        o = outs[None]
        return conclude(root, rid, part, began, again, note, o["result"], o["returncode"], o["error"], claude, say, go_on, direct, retrospect)
    # the researchers' ends together: their last words, the first error among them
    replies = [f"{r}: {(o['result'].get('result') or '').strip()}" for r, o in outs.items() if (o["result"].get("result") or "").strip()]
    broke = next((o for o in outs.values() if o["result"].get("is_error") or o["returncode"]), None)
    result = dict(broke["result"] if broke else {}, result="\n".join(replies))
    returncode = broke["returncode"] if broke else 0
    error = next((o["error"] for o in outs.values() if o["error"]), None)
    return conclude(root, rid, part, began, again, note, result, returncode, error, claude, say, go_on)


# ---------------------------------------------------------------- in the cloud: the web editor's runs

# the prompt's checks, which build the engine: in the cloud there's no Rust, the editor reads it
CHECK = "- check: cargo run -q -p engine-content --bin content -- check"
CLOUD_CHECK = "- check: none in the cloud (no Rust there): the web editor reads the model once it's back"
CLOUD = """In the cloud: this is a sandbox of Anthropic's, the web editor's run, on the user's own API
key: they pay for it, so no detours. Blender runs through run_request.py as below (on a virtual
display), and there's no cargo and no Rust: skip any cargo step of requests.md (the editor
reads the model once it's back).

"""


def cloud_text(text, rid=None):
    """A prompt as the cloud gets it: no cargo there (the research's, asked with `cloud`, brings no
    picture into the sandbox)."""
    return CLOUD + text.replace(CHECK, CLOUD_CHECK)


def prompt_file(rid):
    """Where --prompt writes the prompt, for the sandbox's Claude to read."""
    return Path(tempfile.gettempdir()) / f"bf-prompt-{rid}.txt"


def start_in_cloud(root, rid, go_on=False, note="", again=False, variants=False, stage=None, role=None, direct=False, retrospect=False):
    """--prompt: the run's start in the cloud, as Start's here (its checks, in progress from
    now, its log), and its prompt written for the sandbox's Claude to follow. The request's
    status."""
    record = mr.load(root, rid)
    part = part_of(record, go_on, again, variants, stage)
    if again:
        note = note.strip() or record["comments"].strip()

    def say(text):
        mr.log(root, rid, text)

    say("———————— the retrospective starts ————————" if retrospect else f"———————— the director judges the {stage} stage ————————" if direct else f"———————— the {stage} stage starts ————————" if stage else "———————— a run starts ————————")
    ended = unrunnable(root, rid, record, part, go_on, note, again, variants, say, direct)
    if ended is not None:
        print(f"Nothing to run: {mr.load(root, rid)['progress']['step'] or 'the request stays as it is'}")
        return ended
    text = cloud_text(prompt(record, go_on, note, again, variants, root=root, stage=stage, cloud=True, direct=direct, retrospect=retrospect), rid)
    if retrospect:
        mr.retrospect_start(root, rid)
    elif direct:
        mr.direct_start(root, rid, stage)
    elif stage:
        mr.stage_start(root, rid, stage)
    else:
        mr.start(root, rid, mr.estimate(record, part))
    mr.step(root, rid, "Claude is starting", 0.05)
    say("In the cloud: Claude in a sandbox of Anthropic's, on the user's own API key (the web editor's run)")
    announce(record, part, note, say, go_on, direct, retrospect)
    say("Prompt:\n" + text)
    path = prompt_file(rid)
    path.write_text(text, encoding="utf-8")
    print(f"Your instructions for {rid}: read {path} whole, then follow them, from {root}")
    return "in_progress"


def settle(root, rid, reply="", go_on=False, note="", again=False, variants=False, stage=None, role=None, direct=False, retrospect=False):
    """--settle: the run's end in the cloud, as a run's here ends, by what's on disk (`reply`:
    the sandbox's Claude's line on what it made). The request's status."""
    record = mr.load(root, rid)
    part = part_of(record, go_on, again, variants, stage)
    if again:
        note = note.strip() or record["comments"].strip()

    def say(text):
        mr.log(root, rid, text)

    began = record["progress"]["started"] or time.time()
    return conclude(root, rid, part, began, again, note, {"result": reply} if reply.strip() else {}, 0, None, "claude", say, go_on, direct, retrospect)


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
    stage = argv[argv.index("--stage") + 1] if "--stage" in argv else None
    if stage is not None and stage not in mr.STAGES:
        print(f"no such stage: {stage} (one of {', '.join(mr.STAGES)})")
        return 2
    role = argv[argv.index("--role") + 1] if "--role" in argv else None
    if role is not None and (role not in mr.ROLES or stage != "research"):
        print(f"no such researcher: {role} (one of {', '.join(mr.ROLES)}, of the research stage)")
        return 2
    direct = "--direct" in argv
    if direct and stage is None:
        print("--direct judges a stage: say which with --stage")
        return 2
    how = ("--go-on" in argv, note, "--again" in argv, "--variants" in argv, stage, role, direct, "--retrospect" in argv)
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
