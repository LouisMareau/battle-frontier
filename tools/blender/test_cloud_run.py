"""cloud_run.py and run_claude.py's cloud calls (--prompt, --settle), as a sandbox of
Anthropic's runs them for the web editor, in a scratch project (no Blender fetched: a stand-in
for it is there already; nothing in content/ is touched):

    python tools/blender/test_cloud_run.py

The request's files come from a stand-in for the session's uploads; setup copies them, writes
the cloud's note (the scripts print what changes, for the editor), and notes the files; --prompt
starts the run (in progress, its log, the prompt with no cargo in it); the run's model and the
request done as the kit would write them; --settle ends it; outputs sends back what's new or
changed (the prop, its model, the request's record and renders), not the pictures that came in
nor the log. Exits 1 when a check fails.
"""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import model_requests as mr  # noqa: E402

SCRATCH = PROJECT / "build" / "cloud-run-test"
failed = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""), flush=True)
    if not ok:
        failed.append(what)


def tool(*args):
    env = dict(os.environ, BF_UPLOADS=str(SCRATCH / "uploads"), BF_OUTPUTS=str(SCRATCH / "outputs"), BF_SANDBOX_TOOLS=str(SCRATCH / "sandbox"))
    run = subprocess.run([sys.executable, str(HERE / args[0]), *args[1:]], cwd=str(SCRATCH), env=env, capture_output=True, text=True, encoding="utf-8")
    return run.returncode, run.stdout + run.stderr


if SCRATCH.exists():
    shutil.rmtree(SCRATCH)
(SCRATCH / "crates" / "content").mkdir(parents=True)
(SCRATCH / "content" / "data" / "props").mkdir(parents=True)
(SCRATCH / "Trunk.toml").write_text("# a scratch project for test_cloud_run.py\n", encoding="utf-8")
# the session's uploads: the request's folder as the editor keeps it
up = SCRATCH / "uploads" / "request"
up.mkdir(parents=True)
record = mr.normal(dict(mr.FIELDS, id="cloud_lamp", name="Cloud lamp", kind="prop", notes="a test", images=["front.png"]))
(up / "request.json").write_text(mr.to_text(record), encoding="utf-8")
(up / "front.png").write_bytes(b"\x89PNG stand-in")
# a prop the request made before, kept in the user's browser: into the project's space
made = SCRATCH / "uploads" / "project" / "temporary" / "data" / "props"
made.mkdir(parents=True)
(made / "cloud_lamp_old.json").write_text('{ "id": "cloud_lamp_old" }\n', encoding="utf-8")
# what the ratings teach of its kind (requests.md, "What the ratings teach"): a prop rated best,
# its record and render, and the kind's lessons, as the editor gives them
taught = SCRATCH / "uploads" / "project" / "private" / "requests"
(taught / "rated_lamp").mkdir(parents=True)
rated = mr.normal(dict(mr.FIELDS, id="rated_lamp", name="Rated lamp", kind="prop", status="done", prop="rated_lamp", rating=88, previews=["preview_front.png"]))
(taught / "rated_lamp" / "request.json").write_text(mr.to_text(rated), encoding="utf-8")
(taught / "rated_lamp" / "preview_front.png").write_bytes(b"\x89PNG rated")
(taught / ".lessons").mkdir()
(taught / ".lessons" / "prop.md").write_text("Works:\nlanterns of few parts\n\nFails:\n(nothing yet)\n", encoding="utf-8")
# Blender there already (a stand-in): nothing fetched
(SCRATCH / "sandbox" / "blender-3.1.2-linux-x64").mkdir(parents=True)
(SCRATCH / "sandbox" / "blender-3.1.2-linux-x64" / "blender").write_text("stand-in", encoding="utf-8")

# 1. setup: the files in, the cloud's note, Blender's wrapper on a virtual display
code, out = tool("cloud_run.py", "setup", "cloud_lamp")
here = mr.folder(SCRATCH, "cloud_lamp")
check("setup: the request's files copied in", code == 0 and (here / "request.json").is_file() and (here / "front.png").read_bytes() == b"\x89PNG stand-in", out[-300:])
check("  the props it made before, into the project's space", (SCRATCH / "temporary" / "data" / "props" / "cloud_lamp_old.json").is_file())
check("  what the ratings teach of its kind, into the requests", mr.load(SCRATCH, "rated_lamp")["rating"] == 88
      and (mr.folder(SCRATCH, "rated_lamp") / "preview_front.png").is_file() and mr.lessons_path(SCRATCH, "prop").is_file())
note = json.loads((SCRATCH / mr.CLOUD).read_text(encoding="utf-8"))
wrapper = Path(note["blender"])
check("  the cloud's note names Blender's wrapper, which runs it on a virtual display", wrapper.is_file() and "xvfb-run -a" in wrapper.read_text(encoding="utf-8")
      and str(SCRATCH / "sandbox" / "blender-3.1.2-linux-x64" / "blender") in wrapper.read_text(encoding="utf-8"), str(note))
check("  the scripts know they're in the cloud", mr.cloud(SCRATCH) is not None)

# 2. --prompt: in progress, its log, the prompt written with no cargo in it
code, out = tool("run_claude.py", "cloud_lamp", "--prompt")
rec = mr.load(SCRATCH, "cloud_lamp")
check("--prompt: the run started, the request in progress", code == 0 and rec["status"] == "in_progress" and rec["progress"]["step"] == "Claude is starting", out[-400:])
lines = [line for line in out.splitlines() if line.startswith("[request")]
changes = [json.loads(line.split(" ", 1)[1]) for line in lines if line.startswith("[request] ")]
said = [json.loads(line.split(" ", 1)[1]) for line in lines if line.startswith("[request-log] ")]
check("  what changed printed for the editor: the status, the bar's step", any(c.get("status") == "in_progress" for c in changes)
      and any(c.get("progress", {}).get("step") == "Claude is starting" for c in changes) and all(c["id"] == "cloud_lamp" for c in changes), str(changes)[:300])
check("  its log lines printed too", any("a run starts" in s["text"] for s in said) and any(s["text"].startswith("In the cloud:") for s in said), str(said)[:300])
told = out.split("Your instructions for cloud_lamp: read ", 1)[-1].split(" whole", 1)[0]
text = Path(told).read_text(encoding="utf-8") if Path(told).is_file() else ""
check("  the prompt written: the cloud's way, no cargo to run", text.startswith("In the cloud:") and 'The request: "Cloud lamp", a prop' in text
      and "cargo run" not in text and "none in the cloud (no Rust there)" in text, told)
check("  what the ratings teach of its kind read first", "python tools/blender/model_requests.py lessons cloud_lamp" in text
      and "Rated lamp (rated_lamp)" in "\n".join(mr.lessons(SCRATCH, "cloud_lamp")))

# 3. the run: its model made and added, as the kit writes them
(SCRATCH / "temporary" / "data" / "props").mkdir(parents=True, exist_ok=True)
(SCRATCH / "temporary" / "assets" / "models").mkdir(parents=True)
(SCRATCH / "temporary" / "data" / "props" / "cloud_lamp.json").write_text('{ "id": "cloud_lamp" }\n', encoding="utf-8")
(SCRATCH / "temporary" / "assets" / "models" / "cloud_lamp.glb").write_bytes(b"glTF stand-in")
(here / "preview_front.png").write_bytes(b"\x89PNG render")
mr.finish(SCRATCH, "cloud_lamp", "cloud_lamp", ["preview_front.png"], "made a lamp")
mr.learnt(SCRATCH, "cloud_lamp", "lanterns of few parts, the iron dark", "")

# 4. --settle: done, as a run here ends
code, out = tool("run_claude.py", "cloud_lamp", "--settle", "--reply", "A lamp of iron.")
rec = mr.load(SCRATCH, "cloud_lamp")
check("--settle: done by what's on disk", code == 0 and rec["status"] == "done" and rec["prop"] == "cloud_lamp", out[-300:])

# 5. outputs: what's new or changed since setup, and where each goes
code, out = tool("cloud_run.py", "outputs", "cloud_lamp")
manifest = json.loads((SCRATCH / "outputs" / "manifest.json").read_text(encoding="utf-8"))
paths = sorted(f["path"] for f in manifest["files"])
check("outputs: the prop, its model, the request's record and render, the kind's lessons written again", code == 0 and paths == [
    "private/requests/.lessons/prop.md", "private/requests/cloud_lamp/preview_front.png", "private/requests/cloud_lamp/request.json",
    "temporary/assets/models/cloud_lamp.glb", "temporary/data/props/cloud_lamp.json"], str(paths))
check("  not what it was given to learn from", not any("rated_lamp" in p for p in paths))
check("  not the picture that came in, nor the log, nor the cloud's note", not any(p.endswith(("front.png", "log.txt", ".cloud.json")) and "preview" not in p for p in paths))
sent = {f["path"]: f["name"] for f in manifest["files"]}
check("  each under a name of its own, its bytes as they are", (SCRATCH / "outputs" / sent["temporary/assets/models/cloud_lamp.glb"]).read_bytes() == b"glTF stand-in"
      and len(set(sent.values())) == len(sent))

# 6. a run that ends without the model: failed, why, its last word kept
mr.update(SCRATCH, "cloud_lamp", status="waiting", prop="", progress=dict(mr.FIELDS["progress"]))
tool("run_claude.py", "cloud_lamp", "--prompt")
code, out = tool("run_claude.py", "cloud_lamp", "--settle", "--reply", "Couldn't finish it.")
rec = mr.load(SCRATCH, "cloud_lamp")
check("--settle with no model made: failed, and why", code == 1 and rec["status"] == "failed" and rec["progress"]["step"] == "failed: Claude ended without adding the model"
      and rec["reply"] == "Couldn't finish it.", rec["progress"]["step"])

print("all good" if not failed else f"{len(failed)} failed")
sys.exit(1 if failed else 0)
