"""A model request's run in the cloud: the web editor's, in a sandbox of Anthropic's on the
user's own API key (crates/game/src/mapedit/requests/cloud.rs starts it and follows it). The
sandbox's Claude runs these around the recipe, from the project's folder (a checkout of the
commit the site was built from):

    python3 tools/blender/cloud_run.py setup ID      the request's files, Blender, what the
                                                     recipe needs
    (then run_claude.py ID --prompt ..., the recipe it writes, run_claude.py ID --settle ...)
    python3 tools/blender/cloud_run.py outputs ID    what the run made, for the editor

`setup` copies the request's files in (the editor uploads its folder, each at
uploads/request/NAME; the props it made and what the ratings teach of its kind, the kind's best
and worst rated models and its lessons, at uploads/project/PATH), fetches Blender 3.1.2 for Linux once (the version the requests are made
with, its download checked against Blender's own sum), writes a wrapper running it on a virtual
display (Xvfb: its Workbench renders need OpenGL, which a sandbox has only in software), makes
`python` the sandbox's Python 3 when there's none, and writes private/requests/.cloud.json,
which the request scripts read: the run's in the cloud (they print what changes for the editor
to follow, and leave the engine's checks to it). It notes every file of the project's spaces and
requests as they are then.

`outputs` puts what's new or changed since (a prop's record and model, the request's renders and
script, the requests a split made) in the session's outputs, each under a name of its own, with
manifest.json saying where each goes in the project.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import model_requests as mr  # noqa: E402

# the session's files (the tests' are elsewhere)
UPLOADS = Path(os.environ.get("BF_UPLOADS", "/mnt/session/uploads"))
OUTPUTS = Path(os.environ.get("BF_OUTPUTS", "/mnt/session/outputs"))
# Blender as the requests are made with it, and where it's kept in the sandbox
BLENDER = "blender-3.1.2-linux-x64"
BLENDER_URL = f"https://download.blender.org/release/Blender3.1/{BLENDER}.tar.xz"
BLENDER_SHA256 = "c1d345b25c6f83708b2681d354d70a3e6023c04bb73cc7943366c0c19e542958"
TOOLS = Path(os.environ.get("BF_SANDBOX_TOOLS", "/opt/battle-frontier"))
# where a run's changes are looked for, the files never sent back (the editor keeps its own log)
WATCHED = ("content", "temporary", "private/requests")
LEFT = {"log.txt", "run.jsonl", "claude-settings.json", "stop", ".cloud.json"}
# what the files were at setup
BEFORE = Path(tempfile.gettempdir()) / "bf-cloud-before.json"


def files(root):
    """Every file where a run's changes are looked for: its path in the project, its size and time."""
    out = {}
    for top in WATCHED:
        base = Path(root) / top
        for p in sorted(base.rglob("*")) if base.is_dir() else []:
            if p.is_file():
                st = p.stat()
                out[p.relative_to(root).as_posix()] = [st.st_size, st.st_mtime_ns]
    return out


def home():
    """Where Blender is kept: TOOLS, else a folder of the user's when that can't be written."""
    for place in (TOOLS, Path.home() / ".battle-frontier"):
        try:
            place.mkdir(parents=True, exist_ok=True)
            (place / ".written").write_text("", encoding="utf-8")
            return place
        except OSError:
            continue
    raise SystemExit(f"nowhere to keep Blender: {TOOLS} and ~/.battle-frontier can't be written")


def fetch_blender(where):
    """Blender 3.1.2 for Linux in `where` (fetched once, checked against Blender's sum); its program."""
    program = where / BLENDER / "blender"
    if program.is_file():
        return program
    archive = where / f"{BLENDER}.tar.xz"
    print(f"fetching {BLENDER_URL} …", flush=True)
    digest = hashlib.sha256()
    with urllib.request.urlopen(BLENDER_URL, timeout=120) as answer, open(archive, "wb") as out:
        while True:
            piece = answer.read(1 << 20)
            if not piece:
                break
            digest.update(piece)
            out.write(piece)
    if digest.hexdigest() != BLENDER_SHA256:
        archive.unlink(missing_ok=True)
        raise SystemExit(f"{BLENDER}.tar.xz isn't Blender's own (its sum is {digest.hexdigest()}): not used")
    subprocess.run(["tar", "-xJf", str(archive), "-C", str(where)], check=True)
    archive.unlink(missing_ok=True)
    return program


def wrapper(where, program):
    """A program running Blender on a virtual display (its path): what run_request.py starts."""
    w = where / "blender-xvfb"
    w.write_text(f'#!/bin/sh\nexec xvfb-run -a -s "-screen 0 1600x1200x24" "{program}" "$@"\n', encoding="utf-8")
    w.chmod(0o755)
    return w


def python_too():
    """`python` as the sandbox's Python 3 when it has none (the recipe says `python`): where, or
    None when it has one already, or it couldn't be made."""
    if shutil.which("python"):
        return None
    for bin_dir in (Path("/usr/local/bin"), Path.home() / ".local" / "bin"):
        try:
            bin_dir.mkdir(parents=True, exist_ok=True)
            (bin_dir / "python").symlink_to(sys.executable)
            return bin_dir / "python"
        except OSError:
            continue
    return None


def setup(root, rid):
    if sys.version_info < (3, 10):
        print(f"Python {sys.version.split()[0]} is too old for the recipe: run this with a Python 3.10 or later")
        return 1
    here = mr.folder(root, rid)
    here.mkdir(parents=True, exist_ok=True)
    given = UPLOADS / "request"
    copied = 0
    for f in sorted(given.iterdir()) if given.is_dir() else []:
        if f.is_file():
            shutil.copyfile(f, here / f.name)
            copied += 1
    if not (here / "request.json").is_file():
        print(f"no request.json among the uploads ({given}): nothing to make")
        return 1
    # the props the request made already (kept in the user's browser, not in the checkout): into
    # the project's spaces, where its runs made them; and what the ratings teach of its kind (its
    # best and worst rated models' records, renders and scripts, its lessons): into the requests
    project = UPLOADS / "project"
    for f in sorted(project.rglob("*")) if project.is_dir() else []:
        rel = f.relative_to(project).as_posix()
        if f.is_file() and (rel.split("/", 1)[0] in ("content", "temporary") or rel.startswith("private/requests/")) and ".." not in rel:
            to = Path(root) / rel
            to.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(f, to)
            copied += 1
    where = home()
    blender = wrapper(where, fetch_blender(where))
    if not shutil.which("xvfb-run"):
        print("xvfb-run isn't here (the environment's packages give it): Blender's renders will fail")
    made = python_too()
    (Path(root) / mr.CLOUD).write_text(json.dumps({"blender": str(blender), "request": rid, "since": time.time()}) + "\n", encoding="utf-8")
    BEFORE.write_text(json.dumps(files(root)), encoding="utf-8")
    print(f"{rid}: its {copied} files in {here.relative_to(root).as_posix()}/; Blender: {blender}"
          + (f"; python: {made}" if made else "") + ". Next: python3 tools/blender/run_claude.py " + rid + " --prompt (and the run's flags)")
    return 0


def outputs(root, rid):
    """What's new or changed since setup, in the session's outputs with manifest.json."""
    before = json.loads(BEFORE.read_text(encoding="utf-8")) if BEFORE.is_file() else {}
    OUTPUTS.mkdir(parents=True, exist_ok=True)
    sent = []
    for path, now in files(root).items():
        name = Path(path).name
        if before.get(path) == now or name in LEFT or name.startswith(".held_"):
            continue
        out = f"{len(sent):03d}_{name}"
        shutil.copyfile(Path(root) / path, OUTPUTS / out)
        sent.append({"name": out, "path": path, "bytes": now[0]})
    (OUTPUTS / "manifest.json").write_text(json.dumps({"request": rid, "files": sent}, indent=1) + "\n", encoding="utf-8")
    print(f"{len(sent)} files for the editor (outputs/manifest.json): " + ", ".join(f["path"] for f in sent))
    return 0


def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if len(argv) < 2 or argv[0] not in ("setup", "outputs"):
        print(__doc__)
        return 2
    root = mr.find_root(Path.cwd())
    return setup(root, argv[1]) if argv[0] == "setup" else outputs(root, argv[1])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
