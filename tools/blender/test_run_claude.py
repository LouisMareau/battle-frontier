"""run_claude.py against a stand-in for Claude Code, in a scratch project (nothing real is
asked of Claude, nothing in content/ is touched):

    python tools/blender/test_run_claude.py

The stand-in (written into the scratch project, CLAUDE points at it) checks how it was
started, then plays one ending each time: it finishes the request, quits without it, crashes,
isn't logged in, goes silent (the watchdog stops it), waits to be cancelled, or draws the
lines and the front and the side and stops there (a request that checks its lines first,
which a run with --go-on then finishes). Exits 1 when a check fails.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import model_requests as mr  # noqa: E402

SCRATCH = PROJECT / "build" / "run-claude-test"
FAKE = '''
import json, os, sys, time
sys.path.insert(0, {tools!r})
import model_requests as mr
sys.stdout.reconfigure(encoding="utf-8")
args = sys.argv[1:]
if args == ["--version"]:
    print("9.9.9 (stand-in)")
    sys.exit(0)
prompt = sys.stdin.read()
need = ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-mode", "dontAsk", "--settings"]
if not all(a in args for a in need) or "private/requests/" not in prompt:
    print("started wrongly: " + " ".join(args), file=sys.stderr)
    sys.exit(9)
settings = json.load(open(args[args.index("--settings") + 1], encoding="utf-8"))
rid = prompt.split("private/requests/")[1].split("/")[0]
def say(event):
    print(json.dumps(event), flush=True)
mode = os.environ["FAKE_MODE"]
say({{"type": "system", "subtype": "init", "model": "claude-test-1", "claude_code_version": "9.9.9", "permissionMode": "dontAsk", "session_id": "s1",
      "allowed": settings["permissions"]["allow"]}})
if mode == "login":
    say({{"type": "assistant", "error": "authentication_failed", "message": {{"content": [{{"type": "text", "text": "Not logged in · Please run /login"}}]}}}})
    say({{"type": "result", "subtype": "success", "is_error": True, "num_turns": 1, "duration_ms": 50, "result": "Not logged in · Please run /login"}})
    sys.exit(1)
say({{"type": "assistant", "message": {{"content": [{{"type": "text", "text": "Reading the request."}},
     {{"type": "tool_use", "name": "Bash", "input": {{"command": "python tools/blender/run_request.py " + rid, "description": "render it"}}}}]}}}})
say({{"type": "user", "message": {{"content": [{{"type": "tool_result", "content": [{{"type": "text", "text": "rendered\\n" * 40}}], "is_error": False}},
     {{"type": "tool_result", "content": [{{"type": "image", "source": "..."}}], "is_error": False}}]}}}})
if mode in ("hang", "cancel"):
    time.sleep(60)
if mode == "error":
    print("something broke", file=sys.stderr, flush=True)
    sys.exit(3)
if mode == "finish":
    for piece in ("model", ".py ", "being written"):
        say({{"type": "stream_event", "event": {{"type": "content_block_delta", "delta": {{"type": "input_json_delta", "partial_json": piece}}}}}})
    mr.finish(os.getcwd(), rid, "test_prop", ["preview_front.png"], "made a test prop")
last = "Made it."
if mode == "lines":
    # the lines (no pictures here: only the front and the side), drawn as the run would
    here = mr.folder(os.getcwd(), rid)
    box = lambda w, h: [[0, 0], [w, 0], [w, h], [0, h]]
    lines = {{"pictures": {{}}, "front": {{"lines": [{{"name": "outline", "closed": True, "points": box(60, 50)}}]}},
             "side": {{"lines": [{{"name": "outline", "closed": True, "points": box(100, 50)}}, {{"name": "head", "ellipse": [15, 20, 10, 10]}}]}}}}
    (here / "lines.json").write_text(json.dumps(lines), encoding="utf-8")
    try:
        import request_lines as rl
        rl.lines(os.getcwd(), rid)
    except SystemExit:  # no Pillow: a stand-in for the drawing
        (here / "profiles.png").write_bytes(b"stand-in")
        mr.update(os.getcwd(), rid, lines=["profiles.png"])
    last = "Lines ready: the shine on its back is the drawing's; how long its tail is, a guess."
say({{"type": "result", "subtype": "success", "is_error": False, "num_turns": 3, "duration_ms": 61000, "total_cost_usd": 0.42, "result": last,
      "permission_denials": [{{"tool_name": "Bash", "tool_input": {{"command": "git status"}}}}]}})
'''

failed = []
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""), flush=True)
    if not ok:
        failed.append(what)


def fresh(rid, **fields):
    """A scratch project with one waiting request."""
    if (SCRATCH / "private" / "requests" / rid).exists():
        shutil.rmtree(SCRATCH / "private" / "requests" / rid)
    record = dict(mr.FIELDS, id=rid, name=rid.replace("_", " ").title(), kind="prop", notes="a test")
    record.update(fields)
    mr.save(SCRATCH, mr.normal(record))


def run(rid, mode, *extra, stop_after=None):
    env = dict(os.environ, CLAUDE=str(SCRATCH / "fake_claude.py"), FAKE_MODE=mode)
    proc = subprocess.Popen([sys.executable, str(HERE / "run_claude.py"), rid, *extra], cwd=str(SCRATCH), env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    if stop_after is not None:
        time.sleep(stop_after)
        (mr.folder(SCRATCH, rid) / "stop").write_text("", encoding="utf-8")
    out, _ = proc.communicate(timeout=120)
    return proc.returncode, out, mr.load(SCRATCH, rid), (mr.folder(SCRATCH, rid) / "log.txt").read_text(encoding="utf-8")


if SCRATCH.exists():
    shutil.rmtree(SCRATCH)
(SCRATCH / "crates" / "content").mkdir(parents=True)
(SCRATCH / "content" / "data" / "props").mkdir(parents=True)
(SCRATCH / "Trunk.toml").write_text("# a scratch project for test_run_claude.py\n", encoding="utf-8")
(SCRATCH / "fake_claude.py").write_text(FAKE.format(tools=str(HERE)), encoding="utf-8")
stamped = re.compile(r"^\[\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\] ")

# 1. finished: done, every step in the log, timed
fresh("test_one")
code, out, rec, log = run("test_one", "finish", "--model", "opus")
check("a run that finishes: done, its prop", code == 0 and rec["status"] == "done" and rec["prop"] == "test_prop", f"{code} {rec['status']} {out[-300:]}")
lines = [line for line in log.splitlines() if not line.startswith(" ")]
check("  every line of the log timed", lines and all(stamped.match(line) for line in lines), log[:300])
for wanted in ("a run starts", "Claude Code: ", "(9.9.9 (stand-in))", "--model opus", "Allowed without asking: ", "Prompt:", 'The request: "Test One", a prop',
               "Claude reached: model claude-test-1", "Claude: Reading the request.", "→ Bash: python tools/blender/run_request.py test_one   (render it)",
               "← rendered", "... (10 more lines)", "← (a picture)", "Claude finished (success): 3 turns, 1 min 1 s, $0.42",
               "Refused, not allowed in this run: Bash git status", "Done: the prop test_prop"):
    check(f"  the log says {wanted!r}", wanted in log)
settings = json.loads((mr.folder(SCRATCH, "test_one") / "claude-settings.json").read_text(encoding="utf-8"))
allow = settings["permissions"]["allow"]
check("  its tools: reading, writing in its folder, Blender through run_request.py, nothing else", "Write(private/requests/test_one/**)" in allow
      and "Bash(python tools/blender/run_request.py test_one *)" in allow and not any(a in ("Write", "Edit", "Bash") or a.startswith("Bash(git") for a in allow), str(allow))
raw = (mr.folder(SCRATCH, "test_one") / "run.jsonl").read_text(encoding="utf-8").splitlines()
check("  the raw stream kept, without the pieces of what was being written", len(raw) == 4 and json.loads(raw[0])["model"] == "claude-test-1"
      and "stream_event" not in log and "partial_json" not in log, f"{len(raw)} lines")
check("  a smooth prop's prompt: its style and its shapes, no pose", "Style: smooth" in log and "Shapes: build it from 8 to 30 shapes" in log and "Pose:" not in log)

# 2. it ends without the model: failed, why, its last word kept
fresh("test_two")
code, out, rec, log = run("test_two", "quit")
check("a run that ends without the model: failed, and why", code == 1 and rec["status"] == "failed" and rec["progress"]["step"] == "failed: Claude ended without adding the model"
      and rec["reply"] == "Made it.", json.dumps(rec["progress"]))
check("  the log says so", "FAILED: Claude ended without adding the model" in log)
# 3. it crashes
fresh("test_three")
code, out, rec, log = run("test_three", "error")
check("a run that crashes: failed, its error in the log", rec["status"] == "failed" and "exit code 3" in rec["progress"]["step"] and "(Claude Code says) something broke" in log, rec["progress"]["step"])
# 4. not logged in
fresh("test_four")
code, out, rec, log = run("test_four", "login")
check("not logged in: failed, saying how to log in, with the program's own path", rec["status"] == "failed" and "/login" in rec["progress"]["step"]
      and str(SCRATCH / "fake_claude.py") in rec["progress"]["step"], rec["progress"]["step"])
# 5. silent: stopped after the quiet time, failed
fresh("test_five")
began = time.time()
code, out, rec, log = run("test_five", "hang", "--quiet", "2")
check("a silent run: stopped after the quiet time, failed", rec["status"] == "failed" and "no word from Claude" in rec["progress"]["step"] and time.time() - began < 30,
      f"{rec['progress']['step']} after {time.time() - began:.0f} s")
# 6. cancelled from the editor (its stop file): waiting again, the log kept
fresh("test_six")
code, out, rec, log = run("test_six", "cancel", stop_after=3)
check("cancelled: waiting again, the stop file gone", rec["status"] == "waiting" and not (mr.folder(SCRATCH, "test_six") / "stop").exists() and "Cancelled from the editor" in log, rec["status"])
# 7. a second run adds to the log
code, out, rec, log = run("test_six", "finish")
check("a second run: done, the first run's log still there", rec["status"] == "done" and log.count("a run starts") == 2 and "Cancelled from the editor" in log)

# 8. an accurate creature, as the pictures stand, as many shapes as it needs: all in its prompt
fresh("test_eight", kind="creature", style="accurate", details="long brown fur, short round ears", pose="as_is", shapes={"min": 1, "max": 100, "unlimited": True})
code, out, rec, log = run("test_eight", "finish")
check("an accurate creature's prompt: the closest match, its details, its pose, its shapes", rec["status"] == "done" and "Style: accurate" in log and "kit.fur" in log
      and "Model details from the user: long brown fur, short round ears" in log and "Pose: as-is" in log and "as many as the design needs, up to about 100" in log)
check("  and its estimate is longer", rec["progress"]["minutes"] >= 0 and mr.estimate(rec) > mr.estimate(mr.normal(dict(rec, style="smooth", shapes={"unlimited": False}))))
check("  every tool allowed, for every style: edits, modifiers, sculpting", "The kit is a start" in log and "not a limit" in log and "kit.sculpt (grab, inflate" in log
      and "kit.editing (bmesh by hand: extrude" in log and "kit.modifier" in log)
check("  a closer look at the pictures, by zooming", "python tools/blender/model_requests.py zoom test_eight PICTURE" in log)
check("  the design told from the drawing before building: light, the artist's ways, what's around it", "tell the design from the drawing" in log
      and "light and shade (shine spots" in log and "the artist's ways (outlines" in log and "around it (the background" in log
      and "reflections make a glossy material" in log and "model_requests.py colours test_eight PICTURE" in log
      and "Write both lists as comments" in log and "what you took for the drawing" in log, log[:200])
check("  compared on shapes and colours, not light; accurate copies none of the drawing", "not the light: the previews are lit evenly" in log
      and "none of the drawing's light, shade or lines" in log)
check("  lines first: over each picture, the front and the side drawn from them, the model built on those", "draw the design's lines over each picture" in log
      and "python tools/blender/model_requests.py lines test_eight\n" in log and "python tools/blender/model_requests.py zoom test_eight PICTURE X0 Y0 X1 Y1 --grid --lines" in log
      and "then draw the front and the side in lines.json" in log and "means drawing the design again, from the front and from the side" in log
      and "job.profiles() has each of their lines in cells" in log and "row has the lines on the model and how far each outline and the model overlap" in log)
sys.path.insert(0, str(HERE))
import run_claude as rc  # noqa: E402
tpose = rc.settings_lines(mr.normal({"kind": "creature", "pose": "t_pose"}))
check("a T-pose creature: the T-pose of its body plan, said in the reply", "no limbs (a grub" in tpose and "more legs: each" in tpose and "wings: spread" in tpose
      and "Say in your reply which of these it got" in tpose, tpose[:200])
check("  a prop: no pose at all", "Pose" not in rc.settings_lines(mr.normal({"kind": "prop", "pose": "t_pose"})))
check("accurate: every part and detail in the lines; else the outline and the main parts", "Lines: every part, marking and detail" in log
      and "Lines: the outline and the main parts" in rc.settings_lines(mr.normal({"style": "smooth"})))
for checked_first in (False, True):
    asked = " ".join(rc.prompt(mr.normal(dict(mr.load(SCRATCH, "test_eight"), check_lines=checked_first))).split())
    check(f"each outline on its picture's silhouette, about 90% or more{', before the check' if checked_first else ''}; another creature in the box said, a busy background by eye",
          "says how far each outline and the picture's silhouette overlap" in asked and "the background or the shadow under the creature" in asked
          and "each outline overlaps its picture's silhouette about 90% or more" in asked
          and "Another creature (or text) inside that box counts as silhouette too: keep the outline tight on the design, never stretched over it, and say so in the reply" in asked
          and "A busy background can't be scored: judge that outline by eye" in asked, asked[:400])

# 9. "Let me check the lines first": the run stops once the front and the side are drawn, the
# request waiting for the user; "The lines look right: go on" runs again from the model, with
# what the user says is off
fresh("test_check", kind="creature", check_lines=True)
code, out, rec, log = run("test_check", "lines")
check("checking the lines first: the run ends after them, waiting for the user's check, not failed", code == 0 and rec["status"] == "waiting"
      and rec["progress"]["step"] == mr.LINES_READY and mr.lines_ready(rec) and "FAILED" not in log and "lines ready: check them" in out,
      f"{code} {rec['status']} {json.dumps(rec['progress'])} {out[-200:]}")
check("  its prompt: from reading it to the front and the side, then stop; nothing of the model", "from its step 3 to its step 5" in log
      and "draw the design's lines over each picture" in log and "then draw the front and the side in lines.json" in log
      and "once profiles.png is drawn and every line sits on the design, stop there: no shapes, no" in log
      and "write private/requests/test_check/model.py" not in log and "run_request.py test_check --finish" not in log
      and "End with a few lines for the user, who" in log, log[-1500:])
check("  the log says what this run is, and that it waits", "This run: the lines, the front and the side, then it stops for the user to check them" in log
      and "Lines ready: waiting for the user to check them" in log)
check("  what it says for the check kept; the drawings listed for the editor; the bar's start kept", rec["reply"].startswith("Lines ready: the shine on its back")
      and "profiles.png" in rec["lines"] and rec["progress"]["started"] > 0, f"{rec['reply']!r} {rec['lines']}")
listed = subprocess.run([sys.executable, str(HERE / "model_requests.py"), "list"], cwd=str(SCRATCH), capture_output=True, text=True, encoding="utf-8").stdout
check("  model_requests.py list: the user's turn, not one to make", any("test_check" in line and "lines ready: check them (the user's turn)" in line
                                                                              and "lines checked first" in line for line in listed.splitlines()), listed)
code, out, rec, log = run("test_check", "finish", "--go-on", "--note", "the tail is longer\nthe ears sit higher")
second = log.split("a run starts")[-1]
check("the lines look right, go on: done, a run of its own in the log", code == 0 and rec["status"] == "done" and rec["prop"] == "test_prop"
      and log.count("a run starts") == 2, f"{code} {rec['status']} {out[-300:]}")
check("  its prompt: keep lines.json, fix what the user says first, on from the model", "from its step 6, the model" in second
      and "the user has checked them" in second and "Keep lines.json. What the user says is off in them:" in second
      and "    the tail is longer" in second and "    the ears sit higher" in second and "Fix that in lines.json first" in second
      and "python tools/blender/model_requests.py lines test_check again" in second and "write private/requests/test_check/model.py" in second
      and "run_request.py test_check --finish" in second, second[:2500])
check("  and not the lines drawn again, nor a stop", "draw the design's lines over each picture" not in second and "stop there" not in second
      and "This run goes on from the model, on the lines the user checked: what's off: the tail is longer" in second)
check("  with no note: they look right, lines.json as it is", "The user says they look right: leave lines.json as it is" in rc.prompt(rec, go_on=True)
      and "What the user says is off" not in rc.prompt(rec, go_on=True, note="  "))
again = rc.prompt(rec, note="the ears sit higher")
check("starting over with a note (the window's Start over): fixed first, then the lines checked again", "from its step 3 to its step 5" in again
      and "- the user looked at the lines the last run made (lines.json) and says this is off in them:\n    the ears sit higher\n" in again
      and "stop there" in again and "the user looked at the lines" not in rc.prompt(rec), again[:1500])
fresh("test_no_lines", check_lines=True)
code, out, rec, log = run("test_no_lines", "quit")
check("checking the lines first, but no front and side drawn: failed, and why", rec["status"] == "failed"
      and rec["progress"]["step"] == "failed: Claude ended without drawing the front and the side (profiles.png)", rec["progress"]["step"])
fresh("test_no_lines")
code, out, rec, log = run("test_no_lines", "finish", "--go-on")
check("going on with no lines yet: failed before Claude starts", rec["status"] == "failed" and "nothing to go on from: no lines.json yet" in rec["progress"]["step"]
      and "Claude Code: " not in log, rec["progress"]["step"])
rec = mr.normal({"kind": "creature", "style": "accurate"})
check("the bar's estimate split: the lines about two fifths, the model the rest", mr.estimate(rec, "lines") < mr.estimate(rec, "model") < mr.estimate(rec)
      and abs(mr.estimate(rec, "lines") + mr.estimate(rec, "model") - mr.estimate(rec)) <= 1, [mr.estimate(rec, p) for p in ("lines", "model", "all")])
fresh("test_by_hand", check_lines=True)
mr.start(SCRATCH, "test_by_hand")
waited = subprocess.run([sys.executable, str(HERE / "model_requests.py"), "wait", "test_by_hand", "--check"], cwd=str(SCRATCH), capture_output=True, text=True, encoding="utf-8")
check("a session by hand: wait ID --check leaves it for the user's check", mr.lines_ready(mr.load(SCRATCH, "test_by_hand")) and "lines ready" in waited.stdout, waited.stdout + waited.stderr)
made = subprocess.run([sys.executable, str(HERE / "model_requests.py"), "new", "Checked one", "--check-lines"], cwd=str(SCRATCH), capture_output=True, text=True, encoding="utf-8")
check("  and new --check-lines asks for it", mr.load(SCRATCH, "checked_one")["check_lines"] is True, made.stdout + made.stderr)

# 10. zooming into a picture: a box of it, enlarged into the request's folder
try:
    from PIL import Image
except ImportError:
    Image = None
if Image is not None:
    fresh("test_nine")
    Image.new("RGB", (200, 100), (240, 220, 150)).save(mr.folder(SCRATCH, "test_nine") / "front.png")
    _, said = mr.zoom(SCRATCH, "test_nine", "front.png")
    check("zoom with no box: the picture's size", said.startswith("front.png: 200 x 100 pixels"), said)
    out, said = mr.zoom(SCRATCH, "test_nine", "front.png", [0.5, 0.0, 1.0, 0.5])
    zoomed = Image.open(out)
    check("  a box of it, enlarged into the folder (not one of the request's pictures)", out.name == "zoom_front_1.png" and zoomed.size == (1000, 500)
          and mr.load(SCRATCH, "test_nine")["images"] == [], f"{out.name} {zoomed.size}: {said}")

    # 11. a picture's colours: a part's lit tone, its shade and its shine apart, the background
    # said, see-through pixels left out
    from PIL import ImageDraw
    pic = Image.new("RGBA", (200, 100), (240, 220, 150, 255))
    draw = ImageDraw.Draw(pic)
    draw.rectangle((60, 20, 140, 60), fill=(200, 190, 225, 255))
    draw.rectangle((60, 60, 140, 90), fill=(110, 100, 140, 255))
    draw.rectangle((70, 25, 84, 34), fill=(255, 255, 255, 255))
    draw.rectangle((0, 0, 30, 99), fill=(0, 0, 0, 0))
    pic.save(mr.folder(SCRATCH, "test_nine") / "side.png")
    said = mr.colours(SCRATCH, "test_nine", "side.png")
    text = "\n".join(said)
    check("colours: the main ones, the most covered first, the background said", said[0].startswith("side.png, from (0, 0) to (200, 100)")
          and "#f0dc96  light yellow, all over, along most of the box's edges: the background?" in said[1], text)
    check("  a part's lit tone, its shade underneath, its shine: each its own", "#c8bee1  light violet grey, middle" in text
          and "#6e648c  mid violet, bottom" in text and "#ffffff  near white, top" in text, text)
    check("  see-through pixels left out, and how to read it said", "#000000" not in text and "lit tone, neither its shine nor its shade" in text, text)
    boxed = mr.colours(SCRATCH, "test_nine", "side.png", [60, 20, 141, 60])
    check("  a box of it: only what's in the box", boxed[0].startswith("side.png, from (60, 20) to (141, 60)") and "#c8bee1" in boxed[1]
          and "#f0dc96" not in "\n".join(boxed), "\n".join(boxed))
    check("  colour names", mr.colour_name((200, 40, 30)) == "mid crimson" and mr.colour_name((90, 60, 30)) == "dark brown"
          and mr.colour_name((20, 20, 22)) == "near black" and mr.colour_name((128, 128, 128)) == "mid grey",
          [mr.colour_name(c) for c in ((200, 40, 30), (90, 60, 30), (20, 20, 22), (128, 128, 128))])

    # 12. lines: traced on a picture, the front and the side drawn from them, then laid on renders
    import request_lines as rl
    fresh("test_ten", kind="creature", height=1.0, images=["front.png"])
    ten = mr.folder(SCRATCH, "test_ten")
    pic = Image.new("RGB", (400, 300), (240, 230, 200))
    ImageDraw.Draw(pic).rectangle((150, 100, 250, 200), fill=(120, 90, 160))
    pic.save(ten / "front.png")
    square = [[150, 100], [250, 100], [250, 200], [150, 200]]
    lines_json = {"pictures": {"front.png": {"view": "front", "camera": {"turn": 0, "tilt": 0}, "lines": [{"name": "outline", "closed": True, "points": square},
                                                                                                    {"name": "eye", "ellipse": [200, 130, 10, 6]}]}},
                  "front": {"lines": [{"name": "outline", "closed": True, "points": square}, {"name": "eye", "mark": [180, 130]}]},
                  "side": {"lines": [{"name": "outline", "closed": True, "points": [[0, 0], [100, 0], [100, 50], [0, 50]]}, {"name": "head", "ellipse": [85, 20, 10, 10]}]}}
    (ten / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    said = rl.lines(SCRATCH, "test_ten")
    rec = mr.load(SCRATCH, "test_ten")
    check("lines: each picture's lines drawn over it, the front and the side in cells, listed for the editor", (ten / "lines_front.png").is_file() and (ten / "profiles.png").is_file()
          and rec["lines"] == ["lines_front.png", "profiles.png"] and any(s.startswith("lines_front.png: 2 lines on front.png (front, camera turn 0, tilt 0)") for s in said), "\n".join(said))
    check("  its outline on the picture's own silhouette: all of it", any(s.startswith("lines_front.png") and s.endswith("eye; the outline and the picture's silhouette overlap 100%")
                                                                          for s in said), "\n".join(said))
    check("  what's off said: a side facing the other way than it says", any("the side's head is on its right: say \"faces\": \"right\"" in s for s in said), "\n".join(said))
    cells = rl.profiles(SCRATCH, "test_ten")
    check("  in cells as the model stands: the height asked, on the ground, centred; a mark one point", sorted(p[0] for p in cells["front"]["outline"])[:1] == [-0.5]
          and max(p[1] for p in cells["front"]["outline"]) == 1.0 and cells["front"]["eye"] == [(-0.2, 0.7)] and max(p[0] for p in cells["side"]["outline"]) == 1.0, json.dumps(cells)[:300])
    lines_json["side"]["faces"] = "right"
    (ten / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    cells = rl.profiles(SCRATCH, "test_ten")
    check("  a side drawn facing right is turned to face left", min(p[0] for p in cells["side"]["head"]) < -0.5 and not any("faces" in s for s in rl.lines(SCRATCH, "test_ten")))
    bad = dict(lines_json, pictures={"back.png": {"lines": [{"name": "eye", "ellipse": [1, 2]}, {"points": [[0, 0]]}]}})
    (ten / "lines.json").write_text(json.dumps(bad), encoding="utf-8")
    try:
        rl.lines(SCRATCH, "test_ten")
        wrong = ""
    except rl.LinesError as e:
        wrong = str(e)
    check("  a wrong lines.json: all that's wrong said at once", "pictures.back.png: not one of the request's pictures (front.png)" in wrong
          and "(eye): an \"ellipse\" is [x, y, its half width, its half height]" in wrong and "lines[1]: it needs a \"name\"" in wrong, wrong)
    (ten / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    # renders as request_kit.py's previews leave them: the model a dark square on the grey
    for name, box in (("preview_front.png", (50, 50, 150, 150)), ("preview_side.png", (60, 20, 140, 150)), ("seen_front.png", (30, 30, 170, 170))):
        render = Image.new("RGB", (200, 200), (190, 190, 186))
        ImageDraw.Draw(render).rectangle((box[0], box[1], box[2] - 1, box[3] - 1), fill=(60, 50, 80))
        render.save(ten / name)
    views = {"preview_front.png": {"size": [200, 200], "centre": [0, 0, 0.5], "right": [1, 0, 0], "up": [0, 0, 1], "scale": 2.0, "box": [50, 50, 150, 150]},
             "preview_side.png": {"size": [200, 200], "centre": [0, 0, 0.5], "right": [0, 1, 0], "up": [0, 0, 1], "scale": 2.0, "box": [60, 20, 140, 150]},
             "seen_front.png": {"size": [200, 200], "centre": [0, 0, 0.5], "right": [1, 0, 0], "up": [0, 0, 1], "scale": 2.0, "box": [30, 30, 170, 170]}}
    (ten / "views.json").write_text(json.dumps(views), encoding="utf-8")
    sheet = Image.new("RGB", (400, 2 * (100 + 12) + 12), (237, 235, 230))
    sheet.save(ten / "compare.png")
    began = time.time() - 5
    said = rl.on_renders(SCRATCH, "test_ten", since=began)
    text = "\n".join(said)
    front_said = next((s for s in said if s.startswith("fit_front.png")), "")
    share = float(front_said.split("overlap ")[1].split("%")[0]) if "overlap " in front_said else 0
    check("on the renders: the front's lines where they fall in cells, how far the outline and the model overlap", (ten / "fit_front.png").is_file() and share >= 95, text)
    side_said = next((s for s in said if s.startswith("fit_side.png")), "")
    check("  the side's too: a model taller than its lines said, and where", "the model goes" in side_said and "past it (top)" in side_said, side_said)
    check("  each picture's lines on the model seen from its camera, lined up on the outline's box", (ten / "match_front.png").is_file()
          and "match_front.png: front.png's lines on the model, seen from its camera: the outline and the model overlap" in text, text)
    grown = Image.open(ten / "compare.png").size
    rec = mr.load(SCRATCH, "test_ten")
    check("  compare.png a row longer; the editor lists them all", grown == (400, 236 + 100 + 12) and rec["lines"] == ["lines_front.png", "profiles.png", "fit_front.png", "fit_side.png", "match_front.png"],
          f"{grown} {rec['lines']}")
    check("  an old render isn't drawn on again", rl.on_renders(SCRATCH, "test_ten", since=time.time() + 60) == [])
    zoomed, said = mr.zoom(SCRATCH, "test_ten", "front.png", [140, 90, 260, 210], grid=True, lines=True)
    check("zoom with the picture's pixels marked and its lines on it", zoomed.is_file() and Image.open(zoomed).size[0] > 1000, said)

    # 13. each outline on its own picture: how far it and the picture's silhouette overlap (all in
    # the outline's box that isn't the background, nor the creature's shadow on the ground under
    # it), and where one goes past the other; a busy background can't be scored; a picture with
    # see-through edges goes by its alpha
    import random
    pictures = ("flat.png", "busy.png", "clear.png")
    fresh("test_score", kind="creature", images=list(pictures))
    sc = mr.folder(SCRATCH, "test_score")

    def creature(pic, shadow):
        """A round creature, inked, its shadow on the ground under it (wider than it, and lower)."""
        ink = (lambda c: c + (255,)) if pic.mode == "RGBA" else (lambda c: c)
        draw = ImageDraw.Draw(pic)
        draw.ellipse((100, 214, 300, 242), fill=shadow)
        draw.ellipse((130, 90, 270, 230), fill=ink((120, 170, 220)), outline=ink((40, 30, 35)), width=3)
        return pic

    ground = (250, 236, 200)
    creature(Image.new("RGB", (400, 300), ground), tuple(round(c * 0.72) for c in ground)).save(sc / "flat.png")
    rng, busy = random.Random(7), Image.new("RGB", (400, 300))
    for y in range(0, 300, 10):
        for x in range(0, 400, 10):
            ImageDraw.Draw(busy).rectangle((x, y, x + 9, y + 9), fill=tuple(rng.randrange(256) for _ in range(3)))
    creature(busy.copy(), (90, 80, 70)).save(sc / "busy.png")
    clear = busy.convert("RGBA")
    clear.putalpha(0)
    creature(clear, (0, 0, 0, 90)).save(sc / "clear.png")
    close = {"name": "outline", "ellipse": [200, 160, 70, 70]}
    (sc / "lines.json").write_text(json.dumps({"pictures": {p: {"view": "front", "camera": {"turn": 0, "tilt": 0}, "lines": [close]} for p in pictures}}), encoding="utf-8")
    said = rl.lines(SCRATCH, "test_score")
    on = {p: next((s for s in said if s.startswith(f"lines_{Path(p).stem}.png")), "") for p in pictures}

    def scored(text):
        return float(text.split("silhouette overlap ")[1].split("%")[0]) if "silhouette overlap " in text else 0.0

    flat = Image.open(sc / "flat.png")
    snug, _ = rl.outline_score(flat, close)
    # (the shadow, were it the silhouette, would go more than a tenth past the outline)
    check("each outline on its picture: a close one on a flat background high, the shadow under the creature left out", scored(on["flat.png"]) >= 95
          and snug is not None and snug[1] < 0.03, f"{on['flat.png']} {snug}")
    wide, _ = rl.outline_score(flat, {"name": "outline", "ellipse": [225, 160, 95, 70]})
    wide_said = rl._overlap_text(wide, "the picture's silhouette", "the silhouette") if wide else ""
    check("  a loose one low, the side it goes past said", wide is not None and wide[0] < 0.9 and wide[3] >= 0.1 and wide[4] == "right"
          and "past the silhouette (right)" in wide_said, wide_said)
    low, _ = rl.outline_score(flat, {"name": "outline", "ellipse": [200, 185, 70, 45]})
    check("  one short of the creature's top: the silhouette past it there", low is not None and low[0] < 0.9 and low[1] >= 0.1 and low[2] == "top", str(low))
    check("  a busy background: it can't be scored, and why", "the outline can't be scored: the box's edges aren't mostly one colour (a busy background)" in on["busy.png"],
          on["busy.png"])
    check("  a picture with see-through edges: by its alpha, its see-through shadow left out", scored(on["clear.png"]) >= 95, on["clear.png"])
    drawn = Image.open(sc / "lines_flat.png").convert("RGB")
    check("  under the drawing, in a caption of its own", drawn.height > drawn.width and drawn.getpixel((2, drawn.height - 2)) == (30, 30, 34), str(drawn.size))
    check("  and the aim said", "each outline overlapping its picture's silhouette about 90% or more" in said[-1], said[-1])
else:
    print("(no Pillow here: zoom, colours and the lines not checked)")

shutil.rmtree(SCRATCH, ignore_errors=True)
print("all good" if not failed else f"{len(failed)} failed", flush=True)
sys.exit(1 if failed else 0)
