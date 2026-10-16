"""One model request made by Claude Code, unattended, with its log: what the map editor's
Requests window starts with its Start button (and stops with Cancel), or by hand:

    python tools/blender/run_claude.py ID [--model opus|sonnet|haiku] [--go-on] [--note "what's off in the lines"]

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

A request that checks its lines first ("Let me check the lines first": "check_lines") has
Claude stop once it has drawn the lines and the front and the side (profiles.png): the run
ends with the request waiting for the user's check ("lines ready: check them"). --go-on (the
editor's "The lines look right: go on") runs again from the model, on those lines; --note is
what the user says is off in them, which Claude fixes first (going on, or starting again).
"""
import json
import os
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import model_requests as mr  # noqa: E402

# seconds without a word from Claude: the run is stopped, the request failed
QUIET = 30 * 60
# lines of a tool's answer kept in the log (run.jsonl has everything)
SHOWN = 30

# The prompt, in pieces: a whole run follows the recipe from reading the request to adding the
# model; a request that checks its lines first stops after the front and the side (LINES_RUN),
# and once the user has looked, --go-on goes on from the model (GO_ON_RUN).
HEAD = """You are modelling one request of the Battle Frontier map editor, unattended: nobody can
answer a question, so decide for yourself, and say what you guessed in the reply.

The request: "{name}", a {kind} {height} cells tall, in private/requests/{id}/ (its request.json
and its pictures). Notes from the user: {notes}
{settings}

"""
READ = """- read private/requests/{id}/request.json and look at every picture in that folder (to see a
  part closer: python tools/blender/model_requests.py zoom {id} PICTURE X0 Y0 X1 Y1, in pixels
  or fractions of the picture, with no box it gives the size; then look at the zoom it writes);
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
  draws them over the picture (lines_PICTURE.png, which the user sees too): look at each, and
  fix lines.json until every line sits on the design. To see a part closer, its pixels marked:
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
  each shape's size and place come from them. The kit is a start,
  not a limit: reshape whatever needs it with the tool that does it best for the style:
  kit.editing (bmesh by hand: extrude, inset, bevel, move faces, edges and vertices),
  kit.modifier (any of Blender's modifiers), kit.sculpt (grab, inflate, smooth, flatten,
  pinch, crease), kit.tube for curved parts, or any of Blender's Python;
- render it: python tools/blender/run_request.py {id} (it lays the lines on the model too)
- compare private/requests/{id}/compare.png and the previews with the pictures (the shapes,
  proportions and colours, not the light: the previews are lit evenly, on purpose); its last
  row has the lines on the model and how far each outline and the model overlap (aim for
  about 85% or more), and where one goes past the other (a model posed otherwise than a
  picture can't match it where the poses differ: judge those parts by the front and the
  side). Change model.py and render again until it matches (usually three to six tries);
- a creature: rig it and give it its idle (requests.md, step 9);
- add it to the project: python tools/blender/run_request.py {id} --finish
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
          + READ + "{noted}" + LINES + MODEL + RULES.replace("{end}", MADE))
LINES_RUN = (HEAD + "Follow tools/blender/requests.md for this one request, from its step 3 to its step 5 (it's\n"
             "started already), then stop: the user checks the lines before the model is built (the request\n"
             "says \"Let me check the lines first\"):\n"
             + READ + "{noted}" + LINES + STOP + RULES.replace("{end}", "End with a few lines for the user, who\n"
                                                           "checks the lines next: what you took for the drawing, what you guessed, and anything in the\n"
                                                           "lines you're unsure of."))
GO_ON_RUN = (HEAD + "Follow tools/blender/requests.md for this one request from its step 6, the model (it's started\n"
             "already):\n" + READ + CHECKED + MODEL + RULES.replace("{end}", MADE))


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
    "smooth": "Style: smooth: rounded shapes, smooth shaded (fuse with smooth=True, about 6000 triangles).",
    "low_poly": "Style: low-poly: few faces, flat shaded (fuse with smooth=False, about 800 triangles).",
    "accurate": ("Style: accurate: make it match the pictures as closely as you can, whatever that takes: every part, marking and colour "
                 "of the design (none of the drawing's light, shade or lines), the proportions measured off the pictures. Use the most fitting tool for each part (requests.md, \"Accurate\"): kit.fur for fur "
                 "and hair, kit.sculpt and kit.editing for what the shapes can't do, kit.tube for curved parts, painted colours for patterns, more shapes, "
                 "cuts and slabs for the details, a finer voxel and a bigger budget "
                 "(faces=20000 or more). It may take a long time: that's fine."),
}
LINES = {
    "smooth": "Lines: the outline and the main parts, over the pictures and in the front and the side.",
    "low_poly": "Lines: the outline and the main parts, over the pictures and in the front and the side.",
    "accurate": ("Lines: every part, marking and detail, each where the pictures put it, over the pictures and in the front and "
                 "the side too: the model is only as close as these are."),
}
POSES = {
    "t_pose": ("Pose: T-pose, whatever pose the pictures show (the rest pose a rigger wants): two legs: standing straight, arms straight out "
               "to the sides at shoulder height; four legs: standing straight on straight legs, head forward, tail straight back; more legs: each "
               "leg straight out to its side, then down to the ground, evenly spread; wings: spread straight out to the sides; no limbs (a grub, "
               "a snake, a fish): its body straight and level along its length, head forward, nothing curled. Zoom in to be sure which it has. "
               "Say in your reply which of these it got."),
    "as_is": "Pose: as-is: give it the pose the pictures show.",
}


def settings_lines(record):
    """What the request's settings ask of the model, a line each."""
    out = [STYLES.get(record["style"], STYLES["smooth"])]
    if record["style"] == "accurate" and record["details"].strip():
        out.append("Model details from the user: " + record["details"].strip())
    out.append(LINES.get(record["style"], LINES["smooth"]))
    if record["kind"] == "creature":
        out.append(POSES.get(record["pose"], POSES["t_pose"]))
    shapes = record["shapes"]
    if shapes["unlimited"]:
        out.append(f"Shapes: as many as the design needs, up to about {mr.SHAPES_CAP}. Before writing model.py, think it through: list the parts "
                   "the design is made of and the shape for each; a long, careful script is fine, and so is taking your time.")
    else:
        out.append(f"Shapes: build it from {shapes['min']} to {shapes['max']} shapes (spheres, capsules, boxes, cylinders, lathes, slabs, "
                   "a blob's elements, each counts as one).")
    return "\n".join(out)


def part_of(record, go_on=False):
    """Which part of the recipe a run does: "all" of it; "lines", up to the front and the side,
    for a request that checks its lines first; "model", from the model on, once they're checked."""
    return "model" if go_on else "lines" if record["check_lines"] else "all"


def prompt(record, go_on=False, note=""):
    """What Claude is asked for the request: the recipe, or its part (part_of), with what the
    user says is off in the lines (`note`, from the window's check) fixed first."""
    rid = record["id"]
    note = note.strip()
    said = "\n".join("    " + line.rstrip() for line in note.splitlines())
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
    template = {"all": PROMPT, "lines": LINES_RUN, "model": GO_ON_RUN}[part_of(record, go_on)]
    return template.format(id=rid, name=record["name"], kind=record["kind"], height=record["height"], notes=record["notes"].strip() or "(none)",
                           settings=settings_lines(record), checked=checked, noted=noted)


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


def run(root, rid, model=None, quiet=QUIET, go_on=False, note=""):
    """The run, start to end; the request's status as it ended. `go_on`: on from the model, on
    the lines the user checked, `note` saying what's off in them."""
    record = mr.load(root, rid)
    here = mr.folder(root, rid)
    stop = here / "stop"
    if stop.exists():
        stop.unlink()
    began = time.time()
    part = part_of(record, go_on)

    def say(text):
        mr.log(root, rid, text)

    say("———————— a run starts ————————")
    if go_on and not (here / "lines.json").is_file():
        return mr.fail(root, rid, "nothing to go on from: no lines.json yet (Start draws the lines first)")["status"]
    claude = find_claude()
    if not claude:
        return mr.fail(root, rid, "Claude Code not found: install it, or set CLAUDE to the claude program")["status"]
    try:
        version = subprocess.run(program(claude) + ["--version"], capture_output=True, text=True, timeout=60).stdout.strip() or "?"
    except (OSError, subprocess.TimeoutExpired):
        version = "?"
    settings = here / "claude-settings.json"
    settings.write_text(json.dumps({"permissions": {"allow": allowed(rid)}}, indent=2) + "\n", encoding="utf-8")
    args = ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-mode", "dontAsk", "--strict-mcp-config", "--no-session-persistence",
            "--settings", str(settings.relative_to(root))]
    if model:
        args += ["--model", model]
    text = prompt(record, go_on, note)
    mr.start(root, rid, mr.estimate(record, part))
    mr.step(root, rid, "Claude is starting", 0.05)
    say(f"Claude Code: {claude} ({version})")
    if part == "lines":
        say("This run: the lines, the front and the side, then it stops for the user to check them (\"Let me check the lines first\")")
    elif part == "model":
        say("This run goes on from the model, on the lines the user checked: " + (f"what's off: {note.strip()}" if note.strip() else "they look right"))
    if part != "model" and note.strip():
        say(f"What the user says is off in the lines there: {note.strip()}")
    say(f"Sent: claude {' '.join(args)}   (the prompt on its input; model: {model or 'its default'})")
    say("Allowed without asking: " + ", ".join(allowed(rid)))
    say("Prompt:\n" + text)
    extra = {} if os.name == "nt" else {"start_new_session": True}
    proc = subprocess.Popen(program(claude) + args, cwd=str(root), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace", bufsize=1, **extra)
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
    if ended == "cancelled":
        stop.unlink(missing_ok=True)
        say("Cancelled from the editor: Claude stopped, the request waits again")
        return mr.update(root, rid, status="waiting", progress=dict(mr.FIELDS["progress"]))["status"]
    if ended == "quiet":
        return mr.fail(root, rid, f"no word from Claude for {max(1, round(quiet / 60))} min, so it was stopped")["status"]
    record = mr.load(root, rid)
    result = state["result"] or {}
    if record["status"] == "done":
        say(f"Done: the prop {record['prop']}")
        return "done"
    last = str(result.get("result") or "").strip()
    if last:
        # what it says back (for a check, what it guessed in the lines): the window shows it
        mr.update(root, rid, reply=last)
    if state["error"] == "authentication_failed" or "Not logged in" in last:
        # the program by its path: it's often not on the PATH (the editor's "Log in" opens it)
        reason = f'Claude Code isn\'t logged in: run "{claude}" in a terminal and type /login, once (or the button below)'
    elif proc.returncode != 0 or result.get("is_error"):
        reason = f"Claude stopped with an error ({result.get('subtype') or 'no result'}, exit code {proc.returncode})"
    elif part == "lines" and drawn_since(here / "profiles.png", began):
        # where it was asked to stop: the user's turn
        return mr.wait_for_check(root, rid)["status"]
    elif part == "lines":
        reason = "Claude ended without drawing the front and the side (profiles.png)"
    else:
        reason = "Claude ended without adding the model"
    return mr.fail(root, rid, reason)["status"]


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
    root = mr.find_root(Path.cwd())
    status = run(root, rid, model, quiet, "--go-on" in argv, note)
    print(f"{rid}: {status}" + (f", {mr.LINES_READY}" if status == "waiting" and mr.lines_ready(mr.load(root, rid)) else ""))
    return 0 if status in ("done", "waiting") else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
