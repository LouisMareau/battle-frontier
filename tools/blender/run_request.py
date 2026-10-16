"""Blender on a request's model.py, in the background: its renders and compare.png, or with
--finish its model added to the project too (requests.md, steps 5 and 8).

    python tools/blender/run_request.py ID [--finish]

Blender is BLENDER (the program's path), else the newest under Program Files, else `blender`
on the PATH. Its output is passed on without the render's frame-by-frame lines; the exit code
is Blender's (1 when model.py fails).
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import model_requests as mr  # noqa: E402

NOISE = ("Fra:", "Saved:", " Time:", "TBBmalloc")


def find_blender():
    env = os.environ.get("BLENDER")
    if env and Path(env).is_file():
        return env
    found = sorted(Path("C:/Program Files/Blender Foundation").glob("Blender */blender.exe")) if os.name == "nt" else []
    if found:
        return str(found[-1])
    return shutil.which("blender")


def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not argv or argv[0].startswith("-"):
        print(__doc__)
        return 2
    rid, finishing = argv[0], "--finish" in argv[1:]
    root = mr.find_root(Path.cwd())
    script = mr.folder(root, rid) / "model.py"
    if not script.is_file():
        print(f"{script} isn't there yet: write it first (requests.md, step 4)")
        return 2
    blender = find_blender()
    if not blender:
        print("Blender not found: set BLENDER to blender.exe")
        return 2
    command = [blender, "--background", "--factory-startup", "--python-exit-code", "1", "--python", str(script)]
    if finishing:
        command += ["--", "--finish"]
    run = subprocess.Popen(command, cwd=str(root), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
    for line in run.stdout:
        if line.strip() and not line.startswith(NOISE):
            print(line.rstrip(), flush=True)
    return run.wait()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
