"""run_claude.py against a stand-in for Claude Code, in a scratch project (nothing real is
asked of Claude, nothing in content/ is touched):

    python tools/blender/test_run_claude.py

The stand-in (written into the scratch project, CLAUDE points at it) says how it's signed in
(a Claude subscription, else none: FAKE_AUTH), checks how it was started, notes the API key
it was given (none, when the run goes on the subscription), then plays one ending each time: it finishes the request, quits without it, crashes,
isn't logged in, goes silent (the watchdog stops it), waits to be cancelled, or draws the
lines and the front and the side and stops there (a request that checks its lines first,
which a run with --go-on then finishes); a request made once is made again with the user's
comments on it (--again); a request whose picture has several drawings is split, each one a
request of its own; a set of floor tiles: laid by the engine's rules (floor_wfc.py), and its
run's own recipe; grass filling its cells, an environment asset, variants made with the model and
variants of a model made (--variants); a flower and a rock made finely (the rock's height from its
lines' proportions on its footprint), toon unless the user says, drawn by the thousand when asked;
a kind's best and worst rated teaching its next runs, and what they learn. Then the bones the user has drawn over the pictures: in the
record, the prompt, their drawing, what the run says each is, and the rig's chains from them.
Exits 1 when a check fails.
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
if args == ["auth", "status"]:
    auth = os.environ.get("FAKE_AUTH", "claude.ai")
    print(json.dumps({{"loggedIn": auth != "none", "authMethod": auth, "subscriptionType": "max"}}))
    sys.exit(0 if auth != "none" else 1)
open("key_seen.txt", "w", encoding="utf-8").write(os.environ.get("ANTHROPIC_API_KEY", ""))
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
if mode == "variants":
    mr.finish(os.getcwd(), rid, "test_prop", ["preview_front.png", "variant_2.png", "variant_3.png"], "made two variants", ["test_prop_variant_2", "test_prop_variant_3"])
if mode == "finish":
    for piece in ("model", ".py ", "being written"):
        say({{"type": "stream_event", "event": {{"type": "content_block_delta", "delta": {{"type": "input_json_delta", "partial_json": piece}}}}}})
    mr.finish(os.getcwd(), rid, "test_prop", ["preview_front.png"], "made a test prop")
last = "Made it."
if mode in ("stage", "more"):
    # a stage's run: its files written (a stand-in each; a researcher's its own; the review
    # asking more: more.md), a line in the notes log
    here = mr.folder(os.getcwd(), rid)
    stage = prompt.split("This run is its\\n")[1].split(" stage,")[0].lower()
    if mode == "more":
        names = ["plan/research/more.md"]
    elif "You are its " in prompt:
        names = ["plan/research/" + prompt.split("You are its ")[1].split(" researcher")[0].lower() + ".md"]
    else:
        names = list(mr.STAGE_FILES[stage])
    for name in names:
        path = here / name
        path.parent.mkdir(parents=True, exist_ok=True)
        if name == "lines.json":
            path.write_text(json.dumps({{"pictures": {{}}, "front": {{"lines": []}}, "side": {{"lines": []}}}}), encoding="utf-8")
        elif name.endswith(".png"):
            path.write_bytes(b"stand-in")
        else:
            path.write_text("# the " + stage + " stage's\\n", encoding="utf-8")
    (here / "plan").mkdir(exist_ok=True)
    with open(here / "plan" / "notes.log", "a", encoding="utf-8") as notes:
        notes.write("[" + stage + "] a guess\\n")
    last = "Planned the " + stage + "."
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
if mode == "split":
    # two drawings side by side on its picture, each made a request of its own
    mr.split(os.getcwd(), rid, "front.png", [0, 0, 100, 100], "Tall pine", 9.0, "the darker one")
    mr.split(os.getcwd(), rid, "front.png", [100, 20, 200, 100], "Low bush", 2.0)
    last = "Split into Tall pine (9 cells) and Low bush (2 cells)."
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


def run(rid, mode, *extra, stop_after=None, env_extra=None):
    env = {k: v for k, v in os.environ.items() if k not in ("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN")}
    env.update(CLAUDE=str(SCRATCH / "fake_claude.py"), FAKE_MODE=mode, **(env_extra or {}))
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
check("not logged in: failed, saying how to sign in, with the program's own path", rec["status"] == "failed" and "Claude sign-in" in rec["progress"]["step"]
      and str(SCRATCH / "fake_claude.py") + " auth login" in rec["progress"]["step"], rec["progress"]["step"])
# 4b. signed in with a Claude subscription: the run goes on it, an API key in the environment left out
fresh("test_four_b")
code, out, rec, log = run("test_four_b", "finish", env_extra={"ANTHROPIC_API_KEY": "sk-test-key"})
check("a subscription signed in: the run goes on it, the environment's API key left out", rec["status"] == "done" and "On: your Claude subscription (Max)" in log
      and (SCRATCH / "key_seen.txt").read_text(encoding="utf-8") == "", log[-400:])
# 4c. Claude Code not signed in, an API key in the environment: the run goes on the key
fresh("test_four_c")
code, out, rec, log = run("test_four_c", "finish", env_extra={"ANTHROPIC_API_KEY": "sk-test-key", "FAKE_AUTH": "none"})
check("no sign-in, an API key in the environment: the run goes on the key", rec["status"] == "done" and "On: the API key in ANTHROPIC_API_KEY" in log
      and (SCRATCH / "key_seen.txt").read_text(encoding="utf-8") == "sk-test-key", log[-400:])
# 4d. neither: failed before Claude's started, saying how to sign in
fresh("test_four_d")
(SCRATCH / "key_seen.txt").unlink(missing_ok=True)
code, out, rec, log = run("test_four_d", "finish", env_extra={"FAKE_AUTH": "none"})
check("no sign-in, no key: failed at once, saying how to sign in", rec["status"] == "failed" and "isn't signed in to Claude" in rec["progress"]["step"]
      and "Claude sign-in" in rec["progress"]["step"] and not (SCRATCH / "key_seen.txt").exists(), rec["progress"]["step"])
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
check("an accurate creature's prompt: the closest match, in one skin (block-out, wrap, sculpt, the detail as cards), its details, its pose, its shapes", rec["status"] == "done" and "Style: accurate" in log and "kit.fur" in log
      and "1. block it out" in log and "kit.wrap(blockout, name, colours)" in log and "3. sculpt that skin to the profiles" in log and "kit.cards for tufts of fur, feathers, scales, leaves" in log
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
          and "tinted on the drawing: crimson where the silhouette goes past the outline (move it out there), blue where the outline goes past the silhouette (move it in)" in asked
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

# 9j. made in stages (requests.md, "Made in stages"): a run per stage, each with one charge and
# its own files; a stage ended with its files written is done, the request in progress still
# (the editor starts the next); one without them failed, saying which; a stage without the
# stages' files before it refused at once; the study with the lines checked first stops at the
# lines, its run after the check planning the shape; the assembly's finish makes it done, every
# stage done; a stage run again takes the stages after it with it
fresh("test_stages", kind="creature", staged=True)
code, out, rec, log = run("test_stages", "stage", "--stage", "study")
check("the study stage: done with its files, the request in progress still (the editor's turn)", code == 0 and rec["status"] == "in_progress" and rec["stage"] == "study"
      and rec["stages"] == ["study"] and rec["progress"]["step"] == "study done" and "test_stages: in_progress" in out, f"{code} {rec['status']} {rec['stage']} {rec['stages']} {out[-200:]}")
check("  its log: the stage starting, which it is, done, the next the editor's", "the study stage starts" in log and "This run: the study stage, 1 of 9: the lines, the front and the side, and the shape planned" in log
      and "The study stage is done: the editor starts the research stage next" in log, log[-600:])
asked = rc.prompt(rec, stage="study")
check("  its prompt: the stages, its charge, its plan file, the notes log append-only, the lines' steps, nothing of the model", "This run is its\nSTUDY stage, 1 of 9: the lines, the front and the side, and the shape planned" in asked
      and "write private/requests/test_stages/plan/shape.md" in asked and "plan/notes.log is append-only" in asked and 'starting with "[study] "' in asked
      and "from its step 3 to its step 5" in asked and "at the top of plan/shape.md (below)" in asked and "write private/requests/test_stages/model.py" not in asked, asked[-2500:])
check("  the notes log appended, in the plan folder the stages share", (mr.folder(SCRATCH, "test_stages") / "plan" / "notes.log").read_text(encoding="utf-8") == "[study] a guess\n")
check("  each stage estimated at its share of the whole; the stages together longer", mr.estimate(rec, "study") < mr.estimate(rec, "all") < sum(mr.estimate(rec, s) for s in mr.STAGES),
      f"{[mr.estimate(rec, s) for s in mr.STAGES]} of {mr.estimate(rec, 'all')}")
# the research stage: three researchers at once (a Claude each, its own file under plan/research/),
# the one stage that reads the web; their reference pictures fetched into plan/refs/ by
# model_requests.py fetch, within the request's room for them; then the lead's review, which asks
# for a second pass (the researchers again, on its questions) and then compiles what they found
code, out, rec, log = run("test_stages", "stage", "--stage", "research")
here = mr.folder(SCRATCH, "test_stages")
allow = json.loads((here / "claude-settings.json").read_text(encoding="utf-8"))["permissions"]["allow"]
check("the research stage: three researchers at once, each its own file, the stage done with all three; the web's tools allowed to it alone", code == 0 and rec["stages"] == ["study", "research"]
      and rec["research_pass"] == 1 and all((here / "plan" / "research" / f"{r}.md").is_file() for r in mr.ROLES) and "WebSearch" in allow and "WebFetch" in allow
      and "WebSearch" not in rc.allowed("test_stages", "look") and "WebSearch" not in rc.allowed("test_stages"), f"{rec['stages']} {allow}")
check("  its log: each researcher's lines tagged, the three prompts, each raw stream its own", "[subject] Claude: Reading the request." in log and "[methods] Claude finished" in log
      and "Prompt for the references researcher:" in log and all((here / f"run_{r}.jsonl").is_file() for r in mr.ROLES), log[-800:])
asked = {r: rc.prompt(rec, stage="research", role=r) for r in mr.ROLES}
check("  each researcher's prompt: its charge, its file, its pass, the pictures' room", "You are its SUBJECT researcher, one of three working at once" in asked["subject"] and "Pass 1 of at most 2" in asked["subject"]
      and "the lead's doubts answered" in asked["subject"] and "write private/requests/test_stages/plan/research/references.md" in asked["references"]
      and "python tools/blender/model_requests.py fetch test_stages URL --name NAME" in asked["references"] and "16 in all" in asked["references"] and "up to eight this pass" in asked["references"]
      and "how a jaw, a wing or a tail is rigged" in asked["methods"] and "write private/requests/test_stages/plan/research/methods.md" in asked["methods"] and "read plan/research/more.md" not in asked["subject"],
      asked["references"][-1800:])
cloud = rc.cloud_text(rc.prompt(rec, stage="research", cloud=True), "test_stages")
check("  in the cloud: the three in turn in one session, no picture brought into the sandbox", "three researchers run one after another, in this one session" in cloud and "- as the methods researcher:" in cloud
      and "fetch test_stages URL" not in cloud and "note its address and what it shows" in cloud and cloud.startswith(rc.CLOUD), cloud[:600])
code, out, rec, log = run("test_stages", "more", "--stage", "review")
check("the review asking more: the research to run again, a second pass, the review done for now", rec["status"] == "in_progress" and rec["stages"] == ["study", "review"] and rec["research_more"] is True
      and rec["progress"]["step"] == "review done: a second research pass" and "(pass 1 of at most 2;" in rc.prompt(rec, stage="review") and "this being the first pass" in rc.prompt(rec, stage="review")
      and "a second research pass" in log, f"{rec['status']} {rec['stages']} {rec['progress']['step']}")
code, out, rec, log = run("test_stages", "stage", "--stage", "research")
check("  the researchers again: pass 2, on the lead's questions", rec["stages"] == ["study", "research"] and rec["research_pass"] == 2 and rec["research_more"] is False
      and "Pass 2 of at most 2" in rc.prompt(rec, stage="research", role="subject") and "read plan/research/more.md" in rc.prompt(rec, stage="research", role="subject") and ", pass 2:" in log,
      f"{rec['stages']} {rec['research_pass']} {log[-400:]}")
code, out, rec, log = run("test_stages", "stage", "--stage", "review")
check("  the review again: the research compiled, no third pass", rec["stages"] == ["study", "research", "review"] and "this being the second pass (the last" in rc.prompt(rec, stage="review")
      and (here / "plan" / "research.md").is_file(), f"{rec['stages']}")
check("  the stages after it read what the review compiled", all("plan/research.md" in rc.prompt(rec, stage=s) for s in ("look", "detail", "merge")) and "references plan.md names in plan/refs/" in rc.prompt(rec, stage="body")
      and "the research's facts that bear on the build" in rc.prompt(rec, stage="merge"))
check("  a researcher the runner doesn't know, or one outside the research: refused", run("test_stages", "stage", "--stage", "research", "--role", "gossip")[1].startswith("no such researcher: gossip")
      and run("test_stages", "stage", "--stage", "look", "--role", "subject")[1].startswith("no such researcher"))
import http.server  # noqa: E402
import threading  # noqa: E402
from functools import partial  # noqa: E402
served = SCRATCH / "served"
served.mkdir(exist_ok=True)
(served / "photo.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 64)
(served / "page.html").write_text("<html>a page</html>", encoding="utf-8")
(served / "huge.jpg").write_bytes(b"\xff\xd8" + b"\x00" * (mr.REF_MOST + 10))


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), partial(Quiet, directory=str(served)))
threading.Thread(target=server.serve_forever, daemon=True).start()
at = f"http://127.0.0.1:{server.server_address[1]}"
kit_cmd = lambda *a: subprocess.run([sys.executable, str(HERE / "model_requests.py"), *a], cwd=str(SCRATCH), capture_output=True, text=True, encoding="utf-8")  # noqa: E731
got = kit_cmd("fetch", "test_stages", f"{at}/photo.png", "--name", "Back view")
refs = mr.folder(SCRATCH, "test_stages") / "plan" / "refs"
check("a reference picture fetched into plan/refs/ by its name", got.returncode == 0 and (refs / "back_view.png").read_bytes().startswith(b"\x89PNG") and "plan/refs/back_view.png: 0 KB" in got.stdout, got.stdout + got.stderr)
again = kit_cmd("fetch", "test_stages", f"{at}/photo.png", "--name", "Back view")
check("  a second of the same name numbered beside it", again.returncode == 0 and (refs / "back_view_2.png").is_file(), again.stdout + again.stderr)
page = kit_cmd("fetch", "test_stages", f"{at}/page.html")
huge = kit_cmd("fetch", "test_stages", f"{at}/huge.jpg")
nowhere = kit_cmd("fetch", "test_stages", "file:///etc/passwd")
gone = kit_cmd("fetch", "test_stages", f"{at}/missing.png")
check("  a page, a picture too big, a file path and a missing one refused, each saying why", page.returncode == 1 and "not a picture" in page.stdout and huge.returncode == 1 and "too big" in huge.stdout
      and nowhere.returncode == 1 and "not a web address" in nowhere.stdout and gone.returncode == 1 and "couldn't fetch" in gone.stdout and sorted(p.name for p in refs.iterdir()) == ["back_view.png", "back_view_2.png"],
      f"{page.stdout} {huge.stdout} {nowhere.stdout} {gone.stdout}")
check("  the log says what was fetched, and how many the room holds", "Reference fetched: plan/refs/back_view.png (0 KB) from " + at in (mr.folder(SCRATCH, "test_stages") / "log.txt").read_text(encoding="utf-8")
      and f"(2 of {mr.REFS_MOST} pictures taken" in again.stdout, again.stdout)
for k in range(mr.REFS_MOST - 2):
    (refs / f"filler_{k}.png").write_bytes(b"x")
full = kit_cmd("fetch", "test_stages", f"{at}/photo.png", "--name", "one more")
check("  the room full: refused", full.returncode == 1 and "the references' room is full" in full.stdout and not (refs / "one_more.png").exists(), full.stdout)
for k in range(mr.REFS_MOST - 2):
    (refs / f"filler_{k}.png").unlink()
server.shutdown()
code, out, rec, log = run("test_stages", "quit", "--stage", "look")
check("a stage that ends without its files: failed, saying which, the stages before it kept", rec["status"] == "failed"
      and rec["progress"]["step"] == "failed: Claude ended without writing plan/look.md, materials.py (the look stage's)" and rec["stages"] == ["study", "research", "review"] and rec["stage"] == "look",
      f"{rec['status']} {rec['progress']['step']} {rec['stages']}")
code, out, rec, log = run("test_stages", "stage", "--stage", "body")
check("a stage without the stages' files before it: refused at once, saying which stage to run", rec["status"] == "failed"
      and rec["progress"]["step"] == "failed: nothing to build on: no plan/look.md yet (the look stage writes it: run it first)", rec["progress"]["step"])
for name in ("look", "detail", "merge", "body", "dress"):
    code, out, rec, log = run("test_stages", "stage", "--stage", name)
asked = rc.prompt(rec, stage="body")
check("the stages one after another, each done with its files; the body's prompt building on the plan and the materials, no detail, no rig",
      rec["status"] == "in_progress" and rec["stages"] == ["study", "research", "review", "look", "detail", "merge", "body", "dress"] and rec["stage"] == "dress"
      and 'kit.load("body")' in asked and "materials.py (its COLOURS and\n  PAINT: use them, change nothing in it)" in asked and "No\n  detached part, no card, no fur, no rig" in asked
      and "write private/requests/test_stages/model.py (requests.md, \"The kit\": the usual start)" in asked and "No finish yet: the assemble stage adds it" in asked, f"{rec['status']} {rec['stages']}")
asked = rc.prompt(rec, stage="dress")
check("  the dress's: the detached parts and the detail on the body, the body's file unchanged", 'kit.load("detail")' in asked and "def build(job, kit, materials, body)" in asked
      and "body.py (read them, change nothing in them)" in asked, asked[-1500:])
code, out, rec, log = run("test_stages", "finish", "--stage", "assemble")
asked = rc.prompt(rec, stage="assemble")
check("the assembly: a character rigged, finished; done, every stage done", code == 0 and rec["status"] == "done" and rec["prop"] == "test_prop" and rec["stages"] == list(mr.STAGES)
      and rec["stage"] == "assemble" and "rig it: every part on the\n  bones that move it, as plan.md's chains say" in asked and "run_request.py test_stages --finish" in asked
      and "Done: the prop test_prop" in log, f"{rec['status']} {rec['stages']} {log[-300:]}")
code, out, rec, log = run("test_stages", "stage", "--stage", "look")
check("a stage run again: the stages after it to do again, from it", rec["status"] == "in_progress" and rec["stages"] == ["study", "research", "review", "look"] and rec["stage"] == "look", f"{rec['stages']}")
check("  made again with comments: told it was made in stages, which file holds what", "it was made in stages" in rc.prompt(dict(rec, comments="bigger ears"), again=True))
check("  a stage the runner doesn't know: refused", run("test_stages", "stage", "--stage", "paint")[1].startswith("no such stage: paint"))
# the study with the lines checked first: stops at them; after the check, the shape planned on them
fresh("test_stages_check", kind="creature", staged=True, check_lines=True)
code, out, rec, log = run("test_stages_check", "lines", "--stage", "study")
check("the study stage with the lines checked first: stops at them, waiting for the user, the stage not done", rec["status"] == "waiting" and mr.lines_ready(rec) and rec["stages"] == []
      and "then stop: the user checks the lines before the stages go on" in log and "This run: the study stage, 1 of 9: the lines, the front and the side, and the shape planned (the lines first, for the user to check)" in log,
      f"{rec['status']} {rec['stages']} {log[-500:]}")
code, out, rec, log = run("test_stages_check", "stage", "--stage", "study", "--go-on", "--note", "the tail is longer")
check("  after the check: the shape planned on the lines, with the user's note fixed first, the stage done", rec["status"] == "in_progress" and rec["stages"] == ["study"]
      and "plan its shape on them" in log and "the tail is longer" in log and "Fix that in lines.json first" in log and "(the shape planned on the lines the user checked)" in log, f"{rec['status']} {rec['stages']} {log[-800:]}")

# 9b. made once, made again with the user's comments on it (the window's "Make it again with these
# comments"): from what the last run made, the comments it was given kept for the runs after it
fresh("test_again", status="done", prop="test_prop", reply="A redwood, mostly trunk.\nIts bark painted.", comments="the trunk thicker\nthe leaves lighter",
      earlier=["fewer branches low down"])
(mr.folder(SCRATCH, "test_again") / "model.py").write_text("# the last run's model\n", encoding="utf-8")
# (the prompt as the run makes it: the log has it, each line indented under its time)
asked = rc.prompt(mr.load(SCRATCH, "test_again"), again=True)
code, out, rec, log = run("test_again", "finish", "--again")
ran = log.split("a run starts")[-1]
check("made again with the user's comments: done, the comments told to the runs after it, the field emptied", code == 0 and rec["status"] == "done"
      and rec["earlier"] == ["fewer branches low down", "the trunk thicker\nthe leaves lighter"] and rec["comments"] == "", f"{code} {rec['status']} {rec['earlier']} {rec['comments']!r}")
check("  its prompt: from what the last run made, what it said back, the comments, those told before", "made again from what the last run made" in asked
      and "Made again, with the user's comments" in asked and "    A redwood, mostly trunk.\n    Its bark painted.\n" in asked and "it's in the\n  game as the prop test_prop" in asked
      and "says for this run:\n    the trunk thicker\n    the leaves lighter\n" in asked and "1. fewer branches low down" in asked
      and "never over from nothing" in asked and "change private/requests/test_again/model.py, the last run's" in asked
      and "run_request.py test_again --finish" in asked and "made again from what the last run made" in ran, asked[:3000])
check("  and not the lines drawn again from nothing, nor a stop", "draw the design's lines over each picture" not in asked and "stop there" not in asked
      and "This run makes it again from the last run's model, with the user's comments: the trunk thicker" in ran)
check("  in the record's layout: the comments for the next run after the reply, the rating after them", list(mr.FIELDS)[-7:-3] == ["reply", "comments", "earlier", "rating"])
fresh("test_again_note", status="done", prop="test_prop", comments="the old comment")
(mr.folder(SCRATCH, "test_again_note") / "model.py").write_text("# the last run's model\n", encoding="utf-8")
code, out, rec, log = run("test_again_note", "finish", "--again", "--note", "the leaves lighter")
check("  the window's --note over the record's comments; ones written meanwhile left be", "with the user's comments: the leaves lighter" in log
      and "the old comment" not in log and rec["earlier"] == ["the leaves lighter"] and rec["comments"] == "the old comment", f"{rec['earlier']} {rec['comments']!r}")
fresh("test_again_none", status="done", prop="test_prop")
code, out, rec, log = run("test_again_none", "finish", "--again", "--note", "the trunk thicker")
check("made again with no model.py yet: failed before Claude starts", rec["status"] == "failed" and "nothing to make again from: no model.py yet" in rec["progress"]["step"]
      and "Claude Code: " not in log, rec["progress"]["step"])
# 9d. several drawings on its picture ("Process multiple"): the first run makes each a request of its
# own, with the request's settings, its own name, height, picture and notes; the request done
try:
    from PIL import Image as _Image
except ImportError:
    _Image = None
if _Image is not None:
    fresh("test_split", several=True, height=9.0, images=["front.png"], notes="tall trees", style="accurate", details="the bark", mine=True)
    pic = _Image.new("RGB", (200, 100), (240, 240, 240))
    pic.paste((40, 90, 40), (20, 10, 80, 95))
    pic.paste((90, 140, 60), (120, 50, 180, 95))
    pic.save(mr.folder(SCRATCH, "test_split") / "front.png")
    asked = " ".join(rc.prompt(mr.load(SCRATCH, "test_split")).split())
    code, out, rec, log = run("test_split", "split")
    kids = [mr.load(SCRATCH, c) for c in rec["children"]]
    check("several drawings: split, each a request of its own, the request done", code == 0 and rec["status"] == "done"
          and rec["children"] == ["test_split_tall_pine", "test_split_low_bush"] and rec["progress"]["step"] == "split into 2 requests"
          and rec["reply"].startswith("Split into Tall pine"), f"{code} {rec['status']} {rec['children']} {rec['progress']['step']}")
    pine = kids[0] if kids else mr.normal({})
    with _Image.open(mr.folder(SCRATCH, pine["id"]) / pine["images"][0]) if kids else _Image.new("RGB", (1, 1)) as crop:
        size = crop.size
    check("  each with its drawing for a picture, its name and height, the request's settings, its notes and where it was", kids
          and pine["images"] == ["tall_pine.png"] and size == (100, 100) and pine["height"] == 9.0 and kids[1]["height"] == 2.0
          and pine["parent"] == "test_split" and pine["style"] == "accurate" and pine["details"] == "the bark" and pine["mine"] is True
          and pine["kind"] == "prop" and pine["status"] == "waiting" and not pine["several"]
          and pine["notes"].startswith("tall trees the darker one Tall pine is one of the drawings on Test Split's picture (private/requests/test_split/front.png"),
          json.dumps({k: pine[k] for k in ("images", "height", "parent", "notes")}) + f" {size}")
    check("  its prompt: find each drawing, a request each through split, no lines nor model", "Process multiple" in asked
          and "python tools/blender/model_requests.py split test_split PICTURE X0 Y0 X1 Y1" in asked and "the request's 9.0 is the tallest's" in asked
          and "no lines, no model" in asked and "draw the design's lines over each picture" not in asked, asked[:2500])
    check("  each of its requests made as any other, the whole recipe", rc.part_of(pine) == "all" and "Process multiple" not in rc.prompt(pine))
    code, out, rec, log = run("test_split", "split")
    check("  started again once split: nothing run, the request as it was", rec["status"] == "done" and len(rec["children"]) == 2
          and "Split already, into 2 requests" in log and log.count("Claude Code: ") == 1, rec["status"])
    fresh("test_split_none", several=True, images=["front.png"])
    pic.save(mr.folder(SCRATCH, "test_split_none") / "front.png")
    code, out, rec, log = run("test_split_none", "quit")
    check("  a split run that makes none: failed, and why", rec["status"] == "failed" and "without splitting it" in rec["progress"]["step"], rec["progress"]["step"])
    check("  in the record's layout: several, parent and children last", list(mr.FIELDS)[-3:] == ["several", "parent", "children"])
    # the drawings the user has outlined (the editor's Outlines window): those only, each cut to its
    # outline, as tall and on as many cells as the user said
    fresh("test_outlined", several=True, images=["front.png"], kind="environment", height=9.0,
          outlines={"front.png": [{"points": [[15, 5], [85, 5], [85, 98], [15, 98]], "height": 3.0, "cells": [2, 2]}]})
    pic.save(mr.folder(SCRATCH, "test_outlined") / "front.png")
    asked = " ".join(rc.prompt(mr.load(SCRATCH, "test_outlined")).split())
    check("outlined: the prompt makes those only, by their numbers, from their outlines", "the user has outlined the ones to make: 1 on front.png" in asked
          and "python tools/blender/model_requests.py outlines test_outlined" in asked and "split test_outlined --outline K --name" in asked
          and "of nothing else on the pictures: what isn't outlined is left out" in asked and "1 to 1:" in asked and "find each drawing" not in asked, asked[:2200])
    said = mr.draw_outlines(SCRATCH, "test_outlined")
    check("  drawn over its picture, each numbered, with its height and cells", (mr.folder(SCRATCH, "test_outlined") / "outlines_front.png").is_file()
          and said[0].startswith("1. on front.png") and "3 cells tall, on 2 x 2 cells" in said[0], str(said))
    cut = mr.split_outlined(SCRATCH, "test_outlined", 1, "Tall pine")
    with _Image.open(mr.folder(SCRATCH, cut["id"]) / cut["images"][0]) as img:
        corner, middle, mode = img.getpixel((1, 1)), img.getpixel((img.width // 2, img.height // 2)), img.mode
    check("  split by it: its picture cut to the outline, outside it clear, as tall and on the cells the user said", cut["height"] == 3.0 and cut["cells"] == [2, 2]
          and mode == "RGBA" and corner[3] == 0 and middle[3] == 255 and cut["kind"] == "environment" and "the user's outline round it" in cut["notes"],
          f"{corner} {middle} {mode} {cut['height']} {cut['cells']}")
    check("  its own run told the ground it covers", "Cells: it covers about 2 x 2 cells on the ground" in rc.prompt(cut))
    try:
        mr.split_outlined(SCRATCH, "test_outlined", 2, "Nothing")
        wrong = ""
    except SystemExit as e:
        wrong = str(e)
    check("  an outline that isn't there: said", "has 1 outline: there's no 2" in wrong, wrong)
    check("  none outlined: every drawing made", mr.draw_outlines(SCRATCH, "test_split")[0].startswith("The user has outlined no drawings"))
    check("  in the record's layout: the outlines after the bones", list(mr.FIELDS).index("outlines") == list(mr.FIELDS).index("bones") + 1, str(list(mr.FIELDS)))

def after(a, b):
    """Whether field `b` comes right after field `a` in the record's layout."""
    names = list(mr.FIELDS)
    return names.index(b) == names.index(a) + 1


# 9f. grass (requests.md, "Grass"): it fills its cells to the edges, as a tile does; asked for
# variants (requests.md, "Variants"), they're made with it, each a prop of its own; an environment
# asset; a split one's requests keep its variants
fresh("test_grass", kind="grass", cells=[2, 1], height=0.4, variants=2, vary="one taller\none yellower")
grass = mr.load(SCRATCH, "test_grass")
asked = rc.prompt(grass)
check("grass: it fills its cells to the edges, fitted to them, a grass prop", "grass filling 2 x 1 cells to the edges, 0.4 cells tall" in asked
      and "kit.fit_cells(parts, (2, 1), job.height) in place of kit.fit" in asked and 'kit.finish\'s category "grass", never blocking the way' in asked, asked[:1800])
check("  in pieces the engine can thin: its most vertices at its thinnest and in all, by its cells", f"at most {2 * mr.GRASS_THINNEST} vertices, and at most "
      f"{2 * mr.GRASS_MOST} in all ({mr.GRASS_MOST} a cell" in asked and "each blade or leaf a piece of its own" in asked and "kit.finish refuses more" in asked
      and (mr.GRASS_MOST, mr.GRASS_THINNEST) == (60000, 1000), asked[:2600])
check("  its variants made with it, two besides it, as the user says they differ, each a prop of its own",
      "Variants: make 2 besides it, each a prop of its own" in asked and "- its variants, 2 besides it" in asked
      and "they differ as the user says:\n    one taller\n    one yellower\n  model.py builds the model from a function" in asked
      and "from 2 to 3" in asked and "kit.finish_variant(job, k, its parts" in asked and "kit.drop(its parts)" in asked
      and asked.index("- its variants") < asked.index("- add it to the project"), asked[asked.find("Variants:"):][:1500])
check("  and its estimate longer for them", mr.estimate(grass) > mr.estimate(mr.normal(dict(grass, variants=0))))
check("  in the record's layout: its cells after its height, its variants after the notes, the props they became after its own",
      all(after(a, b) for a, b in (("height", "cells"), ("notes", "variants"), ("variants", "vary"), ("prop", "variant_props"))), str(list(mr.FIELDS)))
fresh("test_env", kind="environment", height=1.2)
asked = rc.prompt(mr.load(SCRATCH, "test_env"))
check("environment: what dresses the outdoors, its category the one that fits; no variants unless asked", "an environment asset 1.2 cells tall" in asked
      and "Environment: an asset that dresses the outdoors" in asked and "(tree, flora, rock, grass, decoration)" in asked
      and "- its variants" not in asked and "Variants:" not in asked, asked[:1500])
if _Image is not None:
    fresh("test_split_vary", several=True, images=["front.png"], kind="grass", variants=3, vary="greener")
    pic.save(mr.folder(SCRATCH, "test_split_vary") / "front.png")
    split_asked = rc.prompt(mr.load(SCRATCH, "test_split_vary"))
    tuft = mr.split(SCRATCH, "test_split_vary", "front.png", [0, 0, 100, 100], "Tuft", 0.4)
    check("  split, its drawings' requests keep its kind, its cells and its variants", tuft["kind"] == "grass" and tuft["variants"] == 3 and tuft["vary"] == "greener"
          and "Variants: each of its drawings' requests makes 3 besides it" in split_asked and "- its variants" not in split_asked
          and "- its variants, 3 besides it" in rc.prompt(tuft), json.dumps({k: tuft[k] for k in ("kind", "cells", "variants", "vary")}))

# 9g. variants of a model made (the editor's "Make variants", --variants): from what the last run
# made, the model itself kept, as many as the request says; nothing run when it isn't made or asks
# for none
fresh("test_vary", status="done", prop="test_prop", reply="A flat grey rock.", variants=2, vary="smaller, mossier", variant_props=["test_prop_variant_2"],
      style="low_poly", notes="grey stone")
(mr.folder(SCRATCH, "test_vary") / "model.py").write_text("# the last run's model\n", encoding="utf-8")
asked = rc.prompt(mr.load(SCRATCH, "test_vary"), variants=True)
code, out, rec, log = run("test_vary", "variants", "--variants")
ran = log.split("a run starts")[-1]
check("variants of a model made: done, the props they became", code == 0 and rec["status"] == "done" and rec["variant_props"] == ["test_prop_variant_2", "test_prop_variant_3"]
      and "Done: the prop test_prop, and its variants test_prop_variant_2, test_prop_variant_3" in ran, f"{code} {rec['status']} {rec['variant_props']}")
check("  its prompt: from what the last run made, the model itself kept, how many and how they differ, the rest as before",
      "variants of the model made" in asked and "Variants of a model made" in asked and "game as the prop test_prop (with its variants test_prop_variant_2)" in asked
      and "    A flat grey rock." in asked and "make its variants now, 2 besides it" in asked and "they differ as the user says:\n    smaller, mossier" in asked
      and "Keep the model itself as the last run made it" in asked and "Style: low-poly" in asked and "Notes from the user: grey stone" in asked
      and "run_request.py test_vary --finish" in asked and "draw the design's lines over each picture" not in asked, asked[:3200])
check("  the log says so; its part and its estimate", "This run makes 2 variants of it from its model, the model itself kept: smaller, mossier" in ran
      and rc.part_of(rec, variants=True) == "variants" and 1 <= mr.estimate(rec, "variants") < mr.estimate(rec))
fresh("test_vary_none", variants=2)
code, out, rec, log = run("test_vary_none", "variants", "--variants")
check("  not made yet: nothing run, the request as it was", rec["status"] == "waiting" and "Nothing to make variants of" in log and "Claude Code: " not in log, rec["status"])
fresh("test_vary_zero", status="done", prop="test_prop")
(mr.folder(SCRATCH, "test_vary_zero") / "model.py").write_text("# the last run's model\n", encoding="utf-8")
code, out, rec, log = run("test_vary_zero", "variants", "--variants")
check("  no variants asked for: nothing run, still done", rec["status"] == "done" and "No variants asked for" in log and "Claude Code: " not in log, rec["status"])

# 9h. a flower and a rock made finely (requests.md, "A flower", "A rock"), every model toon
# ("The toon look"), drawn by the thousand when asked ("Drawn by the thousand"); in the
# record's layout as the editor writes it; a tree's trunk of mesh and foliage of leaf cards
# ("A tree")
import request_lines as rl  # noqa: E402
fresh("test_flower", kind="flower", height=0.3)
asked = rc.prompt(mr.load(SCRATCH, "test_flower"))
check("a flower: made finely, its stem, leaves, petals and heart (stamens), a meadow's, never in the way",
      "a flower 0.3 cells tall, made finely" in asked and "Flower: made finely, part by part" in asked and "stamens" in asked
      and 'category "flower", never blocking the way: a meadow can grow it' in asked, asked[:2500])
check("  toon, as every model is; not drawn by the thousand unless asked", "Toon: it's drawn with the game's toon look" in asked and "The prop is added toon" in asked
      and "Drawn by the thousand" not in asked)
check("  its kind asks for variants from the first run on, as grass's, a rock's and a tree's", "flower" in mr.VARYING and "rock" in mr.VARYING and "tree" in mr.VARYING and "flower" in mr.KINDS and "tree" in mr.KINDS)
fresh("test_oak", kind="tree", height=4.0)
asked = rc.prompt(mr.load(SCRATCH, "test_oak"))
check("a tree: its trunk a mesh, its foliage clusters of leaf cards crossing, cut to the leaves' silhouette, no billboards, painted for the wind, a tree prop",
      "a tree 4.0 cells tall, its trunk a mesh and its foliage clusters of leaf cards" in asked and "- trunk: its trunk and its main branches are meshes" in asked
      and "3 or 4 cards" in asked and "no camera-facing billboards" in asked and "cut to the leaves' silhouette" in asked
      and 'category "tree", blocking the way' in asked and "the trunk rooted" in asked, asked[:3000])
# the window's kinds (request_kinds.json): a kind's instructions in every run's prompt, a variety's
# added to them (one named the same replacing the kind's), a kind the user added made as one of
# the kit's and named so, and a project's own copy read before the standard one
import json  # noqa: E402
catalogue = mr.kinds()
environment = next(k for k in catalogue if k["id"] == "environment")
check("the standard categories: character (itself and a creature its varieties), prop, environment (itself, grass, a flower, a rock and a tree, each made its own way), floor",
      [k["id"] for k in catalogue] == ["character", "prop", "environment", "floor"] and all(k["made_as"] == k["id"] for k in catalogue)
      and [v["id"] for v in next(k for k in catalogue if k["id"] == "character")["varieties"]] == ["character", "creature"]
      and all(k["varieties"][0]["id"] == k["id"] and "made_as" not in k["varieties"][0] for k in catalogue)
      and [(v["id"], v.get("made_as", "environment")) for v in environment["varieties"]] == [("environment", "environment")] + [(v, v) for v in mr.ENVIRONMENT_VARIETIES]
      and [f["keyword"] for f in next(v for v in environment["varieties"] if v["id"] == "tree")["features"]] == ["trunk", "foliage", "colour", "wind"], str([k["id"] for k in catalogue]))
check("  a request of one of them from before: the environment's variety now, its lessons the making's still",
      (lambda r: (r["kind"], r["kind_id"], r["variety"], mr.kind_key(r), mr.lesson_key(r)))(mr.normal({"kind": "rock"})) == ("rock", "environment", "rock", "environment", "rock")
      and mr.normal({"kind": "character"})["variety"] == "character" and mr.normal({"kind": "environment", "kind_id": "bush"})["variety"] == "environment"
      and mr.lesson_key(mr.normal({"kind": "environment", "kind_id": "bush"})) == "bush" and mr.lesson_key(mr.normal({"kind": "creature"})) == "character")
# a request from before the character kind: a character, a creature of them (its prop a creature's)
old = mr.normal({"kind": "creature", "name": "Newt"})
check("a creature request from before: made as a character, its variety creature, its prop's category creature",
      (old["kind"], old["variety"], mr.category_of(old), mr.kind_key(old)) == ("character", "creature", "creature", "character")
      and mr.category_of(mr.normal({"kind": "character"})) == "character" and mr.category_of(mr.normal({"kind": "tree"})) == "decoration", str(old))
fresh("test_old", kind="creature", height=0.8)
asked = rc.prompt(mr.load(SCRATCH, "test_old"))
check("  its prompt: a creature, a character rigged", "a creature: a character 0.8 cells tall, rigged, with an idle animation" in asked
      and "a character (a creature too): rig it" in asked, asked[:1500])
own = [json.loads(json.dumps(k)) for k in catalogue]
env = next(k for k in own if k["id"] == "environment")
tree = next(v for v in env["varieties"] if v["id"] == "tree")
env["varieties"].append({"id": "pine", "name": "Pine", "made_as": "tree", "params": tree["params"], "features": [
    {"keyword": "Foliage", "text": "needles in tiers of drooping cards, darker green"}, {"keyword": "cones", "text": "a few brown cones hanging under the tiers"}]})
own.append({"id": "bush", "name": "Bush", "made_as": "environment", "params": ["height", "style"], "features": [{"keyword": "fur", "text": "no fur: leaves as small cards"}], "varieties": []})
(SCRATCH / "private" / "requests").mkdir(parents=True, exist_ok=True)
general = [{"keyword": "silhouette", "text": "read it from the side picture first"}, {"keyword": "wind", "text": "nothing moves in the wind"}]
(SCRATCH / mr.OWN_KINDS).write_text(json.dumps({"features": general, "kinds": own}), encoding="utf-8")
fresh("test_pine", kind="tree", kind_id="environment", variety="pine", height=6.0)
asked = rc.prompt(mr.load(SCRATCH, "test_pine"), root=SCRATCH)
check("a variety made its own way: named so, made as a tree, its kind's instructions with its own (the tree variety's are the tree's)",
      "a pine: a tree 6.0 cells tall, its trunk a mesh" in asked and "- trunk: its trunk and its main branches are meshes" not in asked and "- wind: " in asked
      and "- Foliage: needles in tiers of drooping cards" in asked and "each cluster a few curved planes" not in asked and "- cones: a few brown cones" in asked, asked[:3000])
lines = [line.strip() for line in asked.splitlines() if line.strip().startswith("- ")]
at = lambda start: next(i for i, line in enumerate(lines) if line.startswith(start))
check("  the instructions for every kind first, the environment's wind replacing theirs where they stood",
      "The user's instructions (for every kind, then its kind's and its variety's; a feature each: follow every one):" in asked
      and lines[at("- silhouette: read it from the side") + 1].startswith("- wind: ") and "nothing moves in the wind" not in asked
      and at("- silhouette: read it from the side") < at("- Foliage: needles"), str(lines[:8]))
fresh("test_lawn", kind="grass", cells=[2, 1], height=0.4)
asked = rc.prompt(mr.load(SCRATCH, "test_lawn"), root=SCRATCH)
lines = [line.strip() for line in asked.splitlines() if line.strip().startswith("- ")]
check("  grass, the environment's variety: named as itself, its own instructions after the environment's", "grass filling 2 x 1 cells to the edges, 0.4 cells tall" in asked
      and "a grass:" not in asked and "- edges: " in asked and "- silhouette" in asked and at("- silhouette") < at("- wind: ") < at("- edges: "), asked[:2500])
fresh("test_bush", kind="environment", kind_id="bush", height=1.2)
asked = rc.prompt(mr.load(SCRATCH, "test_bush"), root=SCRATCH)
check("a kind of the user's own: made as its kit kind, named so, its own instructions", "a bush: an environment asset 1.2 cells tall" in asked
      and "Environment: an asset that dresses the outdoors" in asked and "- fur: no fur: leaves as small cards" in asked and "- wind: what the wind moves" not in asked, asked[:2500])
check("  the project's own kinds read first, the standard without a project", any(k["id"] == "bush" for k in mr.kinds(SCRATCH)) and not any(k["id"] == "bush" for k in mr.kinds())
      and mr.kind_key(mr.load(SCRATCH, "test_bush")) == "bush" and mr.kind_key(mr.load(SCRATCH, "test_pine")) == "environment" and mr.lesson_key(mr.load(SCRATCH, "test_pine")) == "tree")
(SCRATCH / mr.OWN_KINDS).unlink()
fresh("test_boulder", kind="rock", cells=[2, 1], many=True)
asked = rc.prompt(mr.load(SCRATCH, "test_boulder"))
check("a rock: by its footprint, its height its pictures' proportions', fitted about its cells", "a rock on about 2 x 1 cells, as tall as its pictures' proportions make it" in asked
      and "Rock: made finely, on about 2 x 1 cells" in asked and "kit.fit_footprint(parts, job.cells) in place of kit.fit" in asked
      and "- moss: in patches where moss grows" in asked and 'category "rock", blocking the way' in asked, asked[:2500])
check("  toon whatever the record said before (no toggle any more), drawn by the thousand when asked: as light as it can be", "Not toon" not in asked and "Toon: it's drawn" in asked
      and "Drawn by the thousand" in asked and f"about {rc.MANY_VERTICES} in all" in asked and "each loose piece (a leaf, a petal, a blade) a piece of its own" in asked)
check("  in the record's layout: toon and drawn by the thousand after mine, the rating after the comments",
      all(after(a, b) for a, b in (("mine", "toon"), ("toon", "many"), ("many", "check_lines"), ("earlier", "rating"), ("rating", "several")))
      and mr.FIELDS["toon"] is True and mr.FIELDS["many"] is False and mr.FIELDS["rating"] is None, str(list(mr.FIELDS)))
check("  a rock's height by its views' proportions on its footprint: each view's height against its width, the mean",
      rl.rock_height({"front": {"lines": [{"name": "outline", "points": [[0, 0], [200, 0], [200, 100], [0, 100]], "closed": True}]},
                      "side": {"lines": [{"name": "outline", "points": [[0, 20], [100, 20], [100, 100], [0, 100]], "closed": True}]}}, [2, 1]) == 0.9
      and rl.rock_height({}, [2, 1]) is None)

# 9i. the ratings (requests.md, "What the ratings teach"): a kind's best (60 or more) and worst (40
# or less) rated, three of each, the one being made left out, teach its next runs; what they learn
# written again whole for the kind
for rid, rating in (("test_daisy", 95), ("test_poppy", 90), ("test_tulip", 72), ("test_iris", 66), ("test_aster", 50), ("test_lily", 20), ("test_rose", 35)):
    fresh(rid, kind="flower", status="done", prop=rid, rating=rating, notes=f"the {rid[5:]}'s notes", comments="the petals curl more" if rating < 40 else "",
          reply=f"made the {rid[5:]}", previews=["preview_front.png"])
    (mr.folder(SCRATCH, rid) / "preview_front.png").write_bytes(b"png")
    (mr.folder(SCRATCH, rid) / "model.py").write_text("# its model\n", encoding="utf-8")
fresh("test_moss", kind="rock", status="done", prop="test_moss", rating=99)
best, worst = mr.best_and_worst(mr.rated(SCRATCH, "flower", but="test_flower"))
check("the best rated (60 or more), three, best first; the worst (40 or less), worst first; of the kind only",
      [r["id"] for r in best] == ["test_daisy", "test_poppy", "test_tulip"] and [r["id"] for r in worst] == ["test_lily", "test_rose"],
      str(([r["id"] for r in best], [r["id"] for r in worst])))
said = "\n".join(mr.lessons(SCRATCH, "test_flower"))
check("  their lessons: each one's rating, notes, what the user said, what its run said, its renders and its script",
      "95  Test Daisy (test_daisy)" in said and "the daisy's notes" in said and "the user said of it: the petals curl more" in said
      and "its run said: made the lily" in said and "its renders: preview_front.png" in said and "built by: private/requests/test_daisy/model.py" in said
      and "Rated best (60 or more)" in said and "Rated worst (40 or less)" in said and "test_moss" not in said and "test_aster" not in said, said)
check("  a run that builds the model reads them first", "python tools/blender/model_requests.py lessons test_flower" in rc.prompt(mr.load(SCRATCH, "test_flower"), root=SCRATCH)
      and "lessons test_flower" not in rc.prompt(mr.load(SCRATCH, "test_flower")))
check("  none rated of its kind, nothing learnt: nothing to read", mr.lessons(SCRATCH, "test_grass") == []
      and "model_requests.py lessons" not in rc.prompt(mr.load(SCRATCH, "test_grass"), root=SCRATCH))
path = mr.learnt(SCRATCH, "test_flower", "petals cupped, curling at their tips", "flat discs for petals")
said = "\n".join(mr.lessons(SCRATCH, "test_flower"))
check("  what a run learns, for the kind's next runs: written again whole, in its lessons, in its log",
      path == SCRATCH / "private" / "requests" / ".lessons" / "flower.md" and "Works:\npetals cupped, curling at their tips" in path.read_text(encoding="utf-8")
      and "What the runs before learnt (private/requests/.lessons/flower.md):" in said and "flat discs for petals" in said
      and "learnt for the flower models" in (mr.folder(SCRATCH, "test_flower") / "log.txt").read_text(encoding="utf-8"), said)
fresh("test_newt", kind="creature")
mr.learnt(SCRATCH, "test_newt", "four legs splayed, its belly low", "")
check("  what was learnt, none rated: read too", "four legs splayed, its belly low" in "\n".join(mr.lessons(SCRATCH, "test_newt"))
      and "Rated best" not in "\n".join(mr.lessons(SCRATCH, "test_newt")))
check("the user's rating: 0 to 100, none taken off", mr.rate(SCRATCH, "test_iris", 130)["rating"] == 100 and mr.rate(SCRATCH, "test_iris", None)["rating"] is None)

# 9e. a set of floor tiles (requests.md, "A set of floor tiles"): laid as the engine lays them
# (floor_wfc.py, its rules in plain Python), every two cells side by side agreeing; its run the
# floor recipe, the lines' check or not
import floor_wfc as fw  # noqa: E402
nesw = ["n", "e", "s", "w"]
check("floor tiles: an edge as its tile lies turned, clockwise seen from above", fw.edge(nesw, 0, 0) == "n" and fw.edge(nesw, 1, 1) == "n"
      and fw.edge(nesw, 1, 0) == "w" and fw.edge(nesw, 2, 0) == "s" and fw.edge(nesw, 3, 3) == "n",
      [fw.edge(nesw, t, s) for t in range(4) for s in range(4)])
stones = [("small", ["joint"] * 4, 2.0, False), ("slab", ["joint"] * 4, 1.0, False),
          ("long_a", ["joint", "long", "joint", "joint"], 1.0, True), ("long_b", ["joint", "joint", "joint", "long"], 1.0, True)]
cells = [(x, z) for z in range(7) for x in range(9)]
laid = fw.lay(stones, cells, seed=3)
names = [n for n, _ in laid.values()]
check("  flagstones laid on every cell, every two side by side agreeing, only joints facing out", sorted(laid) == sorted(cells) and fw.agree(stones, laid) == [],
      fw.agree(stones, laid)[:3])
# (a half's long edge against another's: agree() says so; a half turned round is the other half)
check("  long slabs among them, each whole", names.count("long_a") + names.count("long_b") > 0, f"{names.count('long_a')} {names.count('long_b')}")
check("  the same seed the same floor, another seed another", fw.lay(stones, cells, seed=3) == laid and fw.lay(stones, cells, seed=4) != laid)
check("  mirrored (by hand), east and west swapped, then turned", [fw.edge(nesw, 0, s, True) for s in range(4)] == ["n", "w", "s", "e"]
      and [fw.edge(nesw, 1, s, True) for s in range(4)] == ["e", "n", "w", "s"])
mirrored = fw.lay(stones, cells, seed=3, fixed={(9, 3): ("long_a", 0, True)})
check("  a half laid mirrored by hand binding the cell beside it: the other half against its long edge, west now",
      fw.edge(dict(t[:2] for t in stones)[mirrored[(8, 3)][0]], mirrored[(8, 3)][1], 1) == "long" and fw.agree(stones, {**mirrored, (9, 3): ("long_a", 0, True)}) == [],
      mirrored[(8, 3)])
bound = fw.lay(stones, cells, seed=3, fixed={(9, 3): ("long_b", 0)})
check("  a cell laid already beside them binding them: a long slab's other half against its long edge",
      fw.edge(dict(t[:2] for t in stones)[bound[(8, 3)][0]], bound[(8, 3)][1], 1) == "long" and fw.agree(stones, {**bound, (9, 3): ("long_b", 0)}) == [],
      bound[(8, 3)])
marble = [("field", ["m"] * 4, 4.0, False), ("edge", ["rim", "b", "m", "b"], 1.0, True), ("corner", ["rim", "rim", "b", "b"], 1.0, True)]
square = [(x, z) for z in range(5) for x in range(6)]
framed = fw.lay(marble, square, seed=1)
middles = [c for c, (n, _) in framed.items() if n == "field"]
check("  a set with rims: a border all round, its corners, the field inside", fw.agree(marble, framed) == []
      and sorted(middles) == sorted((x, z) for z in range(1, 4) for x in range(1, 5)) and [framed[c][0] for c in ((0, 0), (5, 0), (0, 4), (5, 4))] == ["corner"] * 4,
      fw.agree(marble, framed)[:3])
fresh("test_floor", kind="floor", images=["floor.png"], check_lines=True, notes="old ruins, moss in the joints", style="smooth")
floor = mr.load(SCRATCH, "test_floor")
asked = " ".join(rc.prompt(floor).split())
check("a set of floor tiles: its own recipe, the lines' check or not, each tile light", rc.part_of(floor) == "floor"
      and '"Test Floor", a set of floor tiles, each a cell, laid to fit by wave function collapse' in asked
      and "kit.floor_tile(job" in asked and "files = kit.layout(job)" in asked and "kit.finish_floor_set(job, files" in asked
      and "draw the design's lines over each picture" not in asked and "Lines:" not in asked
      and "about 400 triangles a tile" in asked and "for the most detailed tile" in asked and "moss in the joints" in asked, asked[:2500])
check("  as many shapes as it needs: each tile's", "each tile as many as its design needs" in rc.settings_lines(mr.normal(dict(floor, shapes={"min": 1, "max": 9, "unlimited": True}))))
mr.update(SCRATCH, "test_floor", status="done", prop="test_floor_slab", reply="Four tiles.", comments="the joints darker")
again = " ".join(rc.prompt(mr.load(SCRATCH, "test_floor"), again=True).split())
check("  made again: the last run's model.py changed, a set of floor tiles still, with the comments", rc.part_of(floor, again=True) == "again"
      and "a set of floor tiles, made again" in again and "the joints darker" in again and "change the last run's model.py: each tile built with" in again
      and "kit.finish_floor_set(job, files" in again, again[:2500])
check("  a prop's run is a prop's still", rc.part_of(mr.normal({"kind": "prop"})) == "all" and "a prop 1.0 cells tall" in rc.prompt(mr.normal({"id": "x", "name": "X", "kind": "prop"})))

# 9c. a picture written over while a viewer shows it (Windows' Photos keeps what it shows mapped,
# letting it be renamed but not written): the old one moved aside, the new one in its place
if os.name == "nt":
    import ctypes
    from ctypes import wintypes
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    k32.CreateFileW.restype = k32.CreateFileMappingW.restype = wintypes.HANDLE
    k32.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    k32.CreateFileMappingW.argtypes = [wintypes.HANDLE, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, wintypes.LPCWSTR]
    k32.MapViewOfFile.restype = wintypes.LPVOID
    k32.MapViewOfFile.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, ctypes.c_size_t]
    k32.UnmapViewOfFile.argtypes = [wintypes.LPCVOID]
    k32.CloseHandle.argtypes = [wintypes.HANDLE]
    d = mr.folder(SCRATCH, "test_again")
    shown = d / "preview_front.png"
    shown.write_bytes(b"the last render")
    # opened as Photos opens it: shared for reading, writing and deleting, then mapped
    handle = k32.CreateFileW(str(shown), 0x80000000, 0x7, None, 3, 0x80, None)
    mapping = k32.CreateFileMappingW(handle, None, 0x02, 0, 0, None)
    view = k32.MapViewOfFile(mapping, 0x4, 0, 0, 0)
    try:
        open(shown, "wb").close()
        held = False
    except OSError:
        held = True
    with open(mr.free(shown), "wb") as f:
        f.write(b"the new render")
    # (the old one gone from the folder at once, its pixels the viewer's until it lets go, or
    # hidden there till then where the file system can't do that)
    check("a picture a viewer shows, written over: the new one in its place, the old moved aside", held and shown.read_bytes() == b"the new render"
          and all(p.name == ".held_1_preview_front.png" for p in d.glob(".held_*")), f"held {held}, {[p.name for p in d.glob('.held_*')]}")
    k32.UnmapViewOfFile(view)
    k32.CloseHandle(mapping)
    k32.CloseHandle(handle)
    mr.free(d / "compare.png")
    check("  and taken away once it's let go, the next time a picture is written there", not list(d.glob(".held_*")), str(list(d.glob(".held_*"))))
fresh("test_again_quiet", status="done", prop="test_prop")
(mr.folder(SCRATCH, "test_again_quiet") / "model.py").write_text("# the last run's model\n", encoding="utf-8")
code, out, rec, log = run("test_again_quiet", "finish", "--again")
check("made again with nothing said: failed before Claude starts", rec["status"] == "failed" and "nothing to change: say what in the comments first" in rec["progress"]["step"]
      and "Claude Code: " not in log, rec["progress"]["step"])

# 10. the bones the user has drawn (the editor's Bones window): kept in the record as the editor writes
# them, and a run reads the pictures by them and rigs the creature on them
BONES = {"front.png": [{"part": "leg", "points": [[1, 2], [3, 4]]}, {"part": "misc", "name": "crest", "points": [[5, 6], [7, 8]]}]}
# the same text as the editor's test (crates/game/src/mapedit/requests.rs, BONES_TEXT)
BONES_TEXT = ('  "bones": {\n    "front.png": [\n      {\n        "part": "leg",\n        "points": [\n          [1, 2],\n          [3, 4]\n        ]\n      },\n'
              '      {\n        "part": "misc",\n        "name": "crest",\n        "points": [\n          [5, 6],\n          [7, 8]\n        ]\n      }\n    ]\n  },\n')
text = mr.to_text(mr.normal({"id": "b", "name": "B", "images": ["front.png"], "bones": BONES}))
check("the user's bones in request.json: kept, laid out as the editor writes them, between the pictures and the drawings", BONES_TEXT in text
      and text.index('"images"') < text.index('"bones"') < text.index('"lines"') and mr.normal({"bones": BONES})["bones"] == BONES, text)
fresh("test_bones", kind="creature", images=["front.png"], bones=BONES)
code, out, rec, log = run("test_bones", "finish")
asked = " ".join(rc.prompt(rec).split())
check("a creature with bones: the run reads the pictures by them", "the user has drawn the creature's bones over its pictures in the editor's Bones window, on front.png (2 lines)" in asked
      and "python tools/blender/model_requests.py bones test_bones" in asked and "work out which part of the creature in the picture each line is on" in asked
      and "take it over your own" in asked and '{"is": "its front left leg", "chain": "leg.L"}' in asked
      and 'carry them over as lines named "bone CHAIN"' in asked, asked[:600])
check("  and rigs it on them; the log says the bones are there", "rig it on the user's bones (kit.rig(kit.parts(), chains) with chains = job.chains()" in asked
      and "The user has drawn the creature's bones over front.png (2 lines): Claude reads the pictures by them" in log, log[-800:])
checked_first = " ".join(rc.prompt(dict(rec, check_lines=True)).split())
going_on = " ".join(rc.prompt(dict(rec, check_lines=True), go_on=True).split())
check("  the lines' run reads them too; the run after the check rigs on them, the lines kept", "python tools/blender/model_requests.py bones test_bones" in checked_first
      and "model_requests.py bones" not in going_on and "kit.rig(kit.parts(), chains) with chains = job.chains()" in going_on)
plain = rc.prompt(mr.normal(dict(rec, bones={})))
check("  none drawn: no word of them", "Bones window" not in plain and "job.chains()" not in plain and "- a character (a creature too): rig it: every part on the bones that move it" in plain)
check("built in parts: the jaw closed on its bone, the tongue on its own, the roar an action", all(w in " ".join(plain.split()) for w in (
    "Build it in parts", "never one lump fused and sculpted", "block each part in from a few shapes with kit.part", "The mouth is built closed",
    "the tongue on a chain of its own", "becomes the \"roar\" action", "preview_roar.png")), plain[:300])
listed = subprocess.run([sys.executable, str(HERE / "model_requests.py"), "list"], cwd=str(SCRATCH), capture_output=True, text=True, encoding="utf-8").stdout
check("  model_requests.py list says how many lines the user has drawn", any("test_bones" in line and "2 bone lines drawn by the user" in line for line in listed.splitlines()), listed)
import request_lines as rl  # noqa: E402
gone = {"front.png": BONES["front.png"] + [{"part": "tail", "points": [[1, 1]]}], "back.png": BONES["front.png"]}
check("  a picture the request doesn't have, or a line of one joint, left out", rl.user_bones(dict(rec, bones=gone)) == BONES)
check("  a request made a prop since: no bones, nor a word of them", rl.user_bones(dict(rec, kind="prop")) == {} and "Bones window" not in rc.prompt(mr.normal(dict(rec, kind="prop"))))
check("  each line named as the editor names it", rl.bone_names([{"part": "leg"}, {"part": "tail"}, {"part": "leg"}, {"part": "misc", "name": "crest"}, {"part": "misc"}, {"part": "misc", "name": " "}])
      == ["leg 1", "tail", "leg 2", "crest", "misc 1", "misc 2"])
views = {"front": {"outline": [(0, 0)], "bone leg.L": [(0.2, 0.5), (0.25, 0.2), (0.25, 0.0)], "bone tail": [(0.0, 0.3), (0.0, 0.5)], "bone spine": [(0.0, 0.4), (0.0, 0.8)],
                   "bone arm.R": [(-0.3, 0.7), (-0.6, 0.7)]},
         "side": {"outline": [(0, 0)], "bone leg.L": [(0.1, 0.52), (0.12, 0.2), (0.05, 0.0)], "bone tail": [(0.3, 0.3), (0.5, 0.45), (0.6, 0.6)], "bone spine": [(0.1, 0.4), (-0.1, 0.8)],
                  "bone ear.L": [(-0.2, 0.9), (-0.2, 1.0)]}}
found, wrong = rl.chains(views)
check("the rig's chains from the front's and the side's bone lines: x from the front, y from the side, the height from both, the body's first",
      list(found) == ["spine", "leg.L", "arm.R", "ear.L"] and found["leg.L"][0] == (0.2, 0.1, 0.51) and found["arm.R"][1] == (-0.6, 0.0, 0.7)
      and found["ear.L"][0] == (0.0, -0.2, 0.9), str(found))
check("  what's wrong said: other joints in each view, a side's chain only in the side", "tail" not in found
      and any(w.startswith("bone tail: 2 joints in the front, 3 in the side") for w in wrong) and any(w.startswith("bone ear.L: only in the side") for w in wrong), str(wrong))

# 11. zooming into a picture: a box of it, enlarged into the request's folder
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

    # 12. a picture's colours: a part's lit tone, its shade and its shine apart, the background
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

    # 13. lines: traced on a picture, the front and the side drawn from them, then laid on renders
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

    # 14. each outline on its own picture: how far it and the picture's silhouette overlap (all in
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

    # where they part, tinted on the drawing under the lines: blue where the outline goes past the
    # silhouette, crimson where the silhouette goes past it, nothing where they agree (nor where
    # they part by a pixel); the caption's squares the key
    def drawn_with(line):
        (sc / "lines.json").write_text(json.dumps({"pictures": {"flat.png": {"view": "front", "camera": {"turn": 0, "tilt": 0}, "lines": [line]}}}), encoding="utf-8")
        rl.lines(SCRATCH, "test_score")
        return Image.open(sc / "lines_flat.png").convert("RGB")

    def over(drawing, line, xy):
        """The drawing's pixel over a pixel of the picture (its box from 36 pixels in, enlarged)."""
        box = rl.around([line], flat.size)
        k = (drawing.width - 36) / (box[2] - box[0])
        return drawing.getpixel((round(36 + (xy[0] + 0.5 - box[0]) * k), round(36 + (xy[1] + 0.5 - box[1]) * k)))

    def like(got, want, by=12):
        return all(abs(a - b) <= by for a, b in zip(got, want))

    body, ground_tone = (120, 170, 220), tuple(round(c * 0.72) for c in ground)
    wide_line = {"name": "outline", "ellipse": [225, 160, 95, 70]}
    seen = drawn_with(wide_line)
    beside, inside, around_it = over(seen, wide_line, (296, 167)), over(seen, wide_line, (203, 163)), over(seen, wide_line, (110, 60))
    check("  the drawing tints where they part: blue where the outline goes past the silhouette, nothing where they agree",
          beside[2] - beside[0] > 40 and like(inside, body) and like(around_it, ground), f"beside {beside}, inside {inside}, around {around_it}")
    key = {c for _, c in seen.crop((0, seen.height - 30, seen.width, seen.height)).getcolors(1 << 16)}
    check("  the caption's squares: the key", rl.PAST in key and rl.SHORT in key, f"{len(key)} colours")
    low_line = {"name": "outline", "ellipse": [200, 185, 70, 45]}
    top = over(drawn_with(low_line), low_line, (203, 112))
    check("  crimson where the silhouette goes past the outline", top[0] - top[2] > 20, str(top))
    snug_drawn = drawn_with(close)
    under_it, tinted = over(snug_drawn, close, (205, 236)), rl._tint(*rl._masks(flat, close)[:2]).getchannel("A").getbbox()
    check("  a close outline: no tint at all (they part by a pixel at most), nor on the shadow", tinted is None and like(under_it, ground_tone), f"{tinted} {under_it}")

    # 15. the user's bones drawn over the picture for the run, and what it says each is on
    drawn_bones = [{"part": "leg", "points": [[150, 200], [140, 250], [140, 290]]}, {"part": "leg", "points": [[250, 200], [260, 250], [260, 290]]},
                   {"part": "misc", "name": "crest", "points": [[200, 90], [200, 40]]}]
    fresh("test_drawn", kind="creature", height=1.0, images=["front.png", "side.png"], bones={"front.png": drawn_bones})
    dr = mr.folder(SCRATCH, "test_drawn")
    pic = Image.new("RGB", (400, 300), (240, 230, 200))
    ImageDraw.Draw(pic).ellipse((130, 60, 270, 290), fill=(120, 90, 160))
    pic.save(dr / "front.png")
    pic.save(dr / "side.png")
    said = rl.bones(SCRATCH, "test_drawn")
    text = "\n".join(said)
    rec = mr.load(SCRATCH, "test_drawn")
    check("bones: the user's lines drawn over the picture they're on, listed for the editor", (dr / "bones_front.png").is_file() and not (dr / "bones_side.png").exists()
          and rec["lines"] == ["bones_front.png"] and said[0].startswith("bones_front.png: the user's 3 bone lines on front.png (400 x 300 pixels)"), text)
    check("  each line's joints, the root first, named as the editor names them", "1. leg 1: 3 joints (150, 200) (140, 250) (140, 290), 2 bones, 91 pixels along" in text
          and "3. crest (a piece the user named): 2 joints (200, 90) (200, 40), 1 bone" in text, text)
    check("  what's not said yet: what each is on", "missing: front.png: say what each of the user's 3 bone lines is on" in text and "(leg 1, leg 2, crest)" in text, text)
    seen = Image.open(dr / "bones_front.png").convert("RGB")
    colours = {c for _, c in seen.getcolors(1 << 20)}
    check("  in each part's colour, the parts' key under it", any(like(c, rl._rgb(rl.BONE_PARTS["leg"]), 6) for c in colours) and any(like(c, rl._rgb(rl.BONE_PARTS["misc"]), 6) for c in colours)
          and seen.height > seen.width, f"{seen.size}, {len(colours)} colours")
    line = {"name": "outline", "ellipse": [200, 175, 70, 115]}
    said_bones = [{"is": "its front left leg", "chain": "leg.L"}, {"is": "its front right leg", "chain": "leg.R"}, {"is": "the crest on its head", "chain": "crest"}]
    tall = [[0, 0], [100, 0], [100, 300], [0, 300]]
    lines_json = {"pictures": {"front.png": {"view": "front", "camera": {"turn": 0, "tilt": 0}, "bones": said_bones, "lines": [line]},
                               "side.png": {"view": "side", "camera": {"turn": 90, "tilt": 0}, "lines": [line]}},
                  "front": {"lines": [{"name": "outline", "closed": True, "points": tall}, {"name": "bone leg.L", "points": [[60, 150], [60, 300]]}]},
                  "side": {"lines": [{"name": "outline", "closed": True, "points": tall}, {"name": "bone leg.L", "points": [[50, 150], [50, 300]]},
                                     {"name": "head", "ellipse": [20, 40, 10, 10]}]}}
    (dr / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    said = rl.lines(SCRATCH, "test_drawn")
    text = "\n".join(said)
    check("lines: the bones drawn again with what the run took each for, nothing missing of them", "1. leg 1: 3 joints (150, 200) (140, 250) (140, 290), 2 bones, 91 pixels along; you took it for its front left leg, chain leg.L" in text
          and "say what each of the user's" not in text and mr.load(SCRATCH, "test_drawn")["lines"][:2] == ["bones_front.png", "lines_front.png"], text)
    check("  the rig's chains it finds in the front and the side", "the rig's chains, from the front's and the side's bone lines: leg.L (2 joints)" in text, text)
    lines_json["pictures"]["front.png"]["bones"] = said_bones[:2]
    del lines_json["front"]["lines"][1]
    (dr / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    text = "\n".join(rl.lines(SCRATCH, "test_drawn"))
    check("  a line not said: the count said; bones only in the side: said too", "lines.json says 2 of the user's 3 bone lines: one for each, in their order" in text
          and "bone leg.L: only in the side" in text, text)
    del lines_json["side"]["lines"][1]
    (dr / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    text = "\n".join(rl.lines(SCRATCH, "test_drawn"))
    check("  none carried into the front and the side: said", "the user's bones aren't in the front and the side yet" in text, text)
    lines_json["pictures"]["front.png"]["bones"] = [{"chain": "leg.L"}]
    (dr / "lines.json").write_text(json.dumps(lines_json), encoding="utf-8")
    try:
        rl.lines(SCRATCH, "test_drawn")
        wrong = ""
    except rl.LinesError as e:
        wrong = str(e)
    check("  what's wrong with what it says of them: said", "pictures.front.png.bones: one for each of the user's bone lines on it" in wrong, wrong)
    fresh("test_boneless", kind="creature", images=["front.png"])
    check("bones with none drawn: read the pictures as they are", rl.bones(SCRATCH, "test_boneless")[0].startswith("The user has drawn no bones"))
else:
    print("(no Pillow here: zoom, colours and the lines not checked)")

shutil.rmtree(SCRATCH, ignore_errors=True)
print("all good" if not failed else f"{len(failed)} failed", flush=True)
sys.exit(1 if failed else 0)
