"""run_claude.py against a stand-in for Claude Code, in a scratch project (nothing real is
asked of Claude, nothing in content/ is touched):

    python tools/blender/test_run_claude.py

The stand-in (written into the scratch project, CLAUDE points at it) checks how it was
started, then plays one ending each time: it finishes the request, quits without it, crashes,
isn't logged in, goes silent (the watchdog stops it) or waits to be cancelled. Exits 1 when a
check fails.
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
need = ["-p", "--output-format", "stream-json", "--verbose", "--permission-mode", "dontAsk", "--settings"]
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
    mr.finish(os.getcwd(), rid, "test_prop", ["preview_front.png"], "made a test prop")
say({{"type": "result", "subtype": "success", "is_error": False, "num_turns": 3, "duration_ms": 61000, "total_cost_usd": 0.42, "result": "Made it.",
      "permission_denials": [{{"tool_name": "Bash", "tool_input": {{"command": "git status"}}}}]}})
'''

failed = []
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""), flush=True)
    if not ok:
        failed.append(what)


def fresh(rid):
    """A scratch project with one waiting request."""
    if (SCRATCH / "private" / "requests" / rid).exists():
        shutil.rmtree(SCRATCH / "private" / "requests" / rid)
    mr.save(SCRATCH, mr.normal(dict(mr.FIELDS, id=rid, name=rid.replace("_", " ").title(), kind="prop", notes="a test")))


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
check("  the raw stream kept", len(raw) == 4 and json.loads(raw[0])["model"] == "claude-test-1", f"{len(raw)} lines")

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

shutil.rmtree(SCRATCH, ignore_errors=True)
print("all good" if not failed else f"{len(failed)} failed", flush=True)
sys.exit(1 if failed else 0)
