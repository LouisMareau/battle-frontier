"""Model requests on disk: what the map editor's Requests window keeps in
private/requests/<id>/ (request.json and the reference pictures), read and written from plain
Python, and from Blender through request_kit.py. tools/blender/requests.md is the recipe.

    python tools/blender/model_requests.py list
    python tools/blender/model_requests.py start ID [--minutes N]    in progress: the bar starts
    python tools/blender/model_requests.py step ID "what it's doing" [--done 0.4]
    python tools/blender/model_requests.py wait ID [--note "why"]    back to waiting (to remake)
    python tools/blender/model_requests.py fail ID "why"             failed, and why
    python tools/blender/model_requests.py note ID "what happened"   a line in its log
    python tools/blender/model_requests.py new "Name" PICTURE... [--kind prop] [--height 1.2]
                                              [--style low_poly|accurate] [--details "..."]
                                              [--pose as_is] [--shapes 8-30|unlimited]
                                              [--notes "..."] [--mine]

request.json is laid out as the project's records are (content::to_text): the editor and this
file write the same text for the same request. Each request's log.txt (what its runs did,
each line timed, the computer's own clock) is written here too: run_claude.py and the
editor's buttons go through `log`.
"""
import json
import re
import shutil
import sys
import time
from pathlib import Path

# the fields in the editor's order, with its defaults (crates/game/src/mapedit/requests.rs)
FIELDS = {
    "id": "",
    "name": "",
    "kind": "creature",
    "height": 1.0,
    "style": "smooth",
    "details": "",
    "pose": "t_pose",
    "shapes": {"min": 8, "max": 30, "unlimited": False},
    "notes": "",
    "mine": False,
    "status": "waiting",
    "progress": {"step": "", "done": 0.0, "started": 0.0, "minutes": 0.0},
    "images": [],
    "previews": [],
    "prop": "",
    "reply": "",
}
KINDS = ("creature", "prop")
STYLES = ("smooth", "low_poly", "accurate")
POSES = ("t_pose", "as_is")
# "unlimited" shapes stop at about this many
SHAPES_CAP = 100
# in the editor's order: waiting first, failed ones above the done
STATUSES = ("waiting", "in_progress", "failed", "done")
PICTURES = (".png", ".jpg", ".jpeg", ".webp")


# ---------------------------------------------------------------- the layout (content::to_text)

def _scalar(v):
    return json.dumps(v, ensure_ascii=False)


def _flat(x):
    return not isinstance(x, (list, dict)) or (isinstance(x, list) and all(not isinstance(y, (list, dict)) for y in x))


def _inline(v):
    if isinstance(v, list):
        return "[" + ", ".join(_inline(x) for x in v) + "]" if all(_flat(x) for x in v) else None
    if isinstance(v, dict):
        if all(_flat(x) for x in v.values()):
            items = [_scalar(k) + ": " + _inline(x) for k, x in v.items()]
            return "{ " + ", ".join(items) + " }" if items else "{}"
        return None
    return _scalar(v)


def _pretty(v, indent=0):
    pad, inner = "  " * indent, "  " * (indent + 1)
    if isinstance(v, dict) and v:
        return "{\n" + ",\n".join(inner + _scalar(k) + ": " + _pretty(x, indent + 1) for k, x in v.items()) + "\n" + pad + "}"
    if isinstance(v, list) and v:
        if all(not isinstance(x, (list, dict)) for x in v):
            line = _inline(v)
            if line is not None and len(line) <= 100:
                return line
        items = []
        for x in v:
            line = _inline(x)
            items.append(inner + (line if line is not None and len(line) <= 120 else _pretty(x, indent + 1)))
        return "[\n" + ",\n".join(items) + "\n" + pad + "]"
    return _inline(v)


def to_text(record):
    """A record laid out as the project's: what `content format` and the editor write."""
    return _pretty(record) + "\n"


# ---------------------------------------------------------------- the project and its requests

def is_project(p):
    p = Path(p)
    return (p / "Trunk.toml").is_file() and (p / "content" / "data" / "props").is_dir() and (p / "crates" / "content").is_dir()


def find_root(start):
    """The project a path is in."""
    start = Path(start).resolve()
    for p in [start, *start.parents]:
        if is_project(p):
            return p
    raise SystemExit(f"{start} isn't in a Battle Frontier project")


def folder(root, rid):
    return Path(root) / "private" / "requests" / rid


def slug(text):
    return re.sub(r"[^a-z0-9]+", "_", text.lower()).strip("_")


def normal(record):
    """The record with every field, in the editor's order, numbers as the editor writes them."""
    out = {}
    for k, default in FIELDS.items():
        v = record.get(k, default)
        if isinstance(default, dict):
            # each part as the editor writes it (1.0 for a float, 8 for a count)
            kinds = {bool: bool, int: int, float: float}
            v = {pk: kinds.get(type(pd), lambda x: x)(v.get(pk, pd)) for pk, pd in default.items()}
        elif isinstance(default, float):
            v = float(v)
        out[k] = v
    for k, v in record.items():
        if k not in out:
            out[k] = v
    return out


def load(root, rid):
    path = folder(root, rid) / "request.json"
    record = json.loads(path.read_text(encoding="utf-8"))
    # the folder is the request's id, whatever the file says
    record["id"] = rid
    return normal(record)


def save(root, record):
    path = folder(root, record["id"]) / "request.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(to_text(normal(record)), encoding="utf-8", newline="\n")
    return record


def update(root, rid, **fields):
    """Read again, changed, written at once: what the editor changed meanwhile stays."""
    record = load(root, rid)
    record.update(fields)
    return save(root, normal(record))


def every(root):
    """Every request (waiting first, then in progress, then done; by name)."""
    base = Path(root) / "private" / "requests"
    out = []
    for d in sorted(base.iterdir()) if base.is_dir() else []:
        if (d / "request.json").is_file():
            try:
                out.append(load(root, d.name))
            except ValueError as e:
                print(f"{d / 'request.json'}: {e}", file=sys.stderr)
    return sorted(out, key=lambda r: (STATUSES.index(r["status"]) if r["status"] in STATUSES else 9, r["name"].lower(), r["id"]))


def waiting(root):
    return [r for r in every(root) if r["status"] == "waiting"]


# ---------------------------------------------------------------- the log: log.txt, each line timed

def stamp():
    return time.strftime("%Y-%m-%d %H:%M:%S")


def log(root, rid, text):
    """Lines added to the request's log.txt: `[2026-10-16 09:41:07] text`, a text of several
    lines indented under its time."""
    path = folder(root, rid) / "log.txt"
    path.parent.mkdir(parents=True, exist_ok=True)
    at = stamp()
    lines = str(text).splitlines() or [""]
    with open(path, "a", encoding="utf-8", newline="\n") as f:
        f.write(f"[{at}] {lines[0]}\n")
        for line in lines[1:]:
            f.write(" " * (len(at) + 3) + line + "\n")


# ---------------------------------------------------------------- progress: the editor's bar

def estimate(record):
    """Minutes a request takes to model, by default: a session's whole time on it (reading the
    pictures, the script, the tries and renders, the rig, adding it)."""
    # what the first runs took: a creature about 10 minutes, a small one 2
    minutes = 12.0 if record["kind"] == "creature" else 8.0
    minutes *= {"low_poly": 0.8, "accurate": 2.5}.get(record["style"], 1.0)
    # as many shapes as it needs: worked out first, written out long
    minutes *= 1.5 if record["shapes"]["unlimited"] else 1.0
    return float(round(minutes))


def start(root, rid, minutes=None):
    """In progress from now, with its estimate (the editor's bar runs on it)."""
    record = load(root, rid)
    minutes = float(minutes) if minutes else estimate(record)
    return update(root, rid, status="in_progress", progress={"step": "reading the pictures", "done": 0.05, "started": float(round(time.time())), "minutes": minutes})


def step(root, rid, text, done=None):
    """What it's doing now, and how far along (the bar never goes back)."""
    record = load(root, rid)
    p = dict(record["progress"])
    p["step"] = text
    if done is not None:
        p["done"] = round(max(p["done"], float(done)), 2)
    return update(root, rid, progress=p)


def fail(root, rid, reason):
    """Failed, and why (the editor shows it; Start again makes it once more)."""
    record = load(root, rid)
    p = dict(record["progress"], step=f"failed: {reason}")
    log(root, rid, f"FAILED: {reason}")
    return update(root, rid, status="failed", progress=p)


def finish(root, rid, prop, previews, reply=None):
    """Done: the prop it became, its renders, and how long it took."""
    record = load(root, rid)
    p = dict(record["progress"])
    took = (time.time() - p["started"]) / 60.0 if p["started"] > 0 else p["minutes"]
    p.update(step="done", done=1.0, minutes=float(round(took, 1)))
    fields = {"status": "done", "progress": p, "prop": prop, "previews": list(previews)}
    if reply is not None:
        fields["reply"] = reply
    return update(root, rid, **fields)


def bar_text(record, now=None):
    """What the editor's bar says (Progress::at); for a failed request, why."""
    p = record["progress"]
    if record["status"] == "failed":
        return p["step"] or "failed"
    if p["started"] <= 0:
        return p["step"] or "starting"
    gone = max(0.0, (now or time.time()) - p["started"]) / 60.0
    left = p["minutes"] - gone
    if p["minutes"] <= 0:
        when = f"{round(gone)} min so far"
    elif left >= 1.5:
        when = f"about {round(left)} min left"
    elif left > 0:
        when = "about a minute left"
    elif left > -3:
        when = "almost done"
    else:
        when = f"longer than the {round(p['minutes'])} min expected"
    return f"{p['step']} · {when}" if p["step"] else when


# ---------------------------------------------------------------- a request from pictures (no editor)

def free_id(root, name):
    base = slug(name) or "request"
    rid, n = base, 2
    while folder(root, rid).exists():
        rid, n = f"{base}_{n}", n + 1
    return rid


def new(root, name, pictures, **fields):
    """A request made here rather than dropped on the editor: the pictures copied in (WebP
    made PNG, which Blender 3.1 can't read), the record written."""
    rid = free_id(root, name)
    d = folder(root, rid)
    d.mkdir(parents=True)
    images = []
    for src in map(Path, pictures):
        if src.suffix.lower() not in PICTURES:
            raise SystemExit(f"{src}: not a PNG, JPG or WebP picture")
        out = f"{slug(src.stem) or 'picture'}{'.png' if src.suffix.lower() == '.webp' else src.suffix.lower().replace('.jpeg', '.jpg')}"
        k = 2
        while out in images:
            stem, ext = out.rsplit(".", 1)
            out, k = f"{stem}_{k}.{ext}", k + 1
        if src.suffix.lower() == ".webp":
            from PIL import Image  # only for WebP
            Image.open(src).save(d / out)
        else:
            shutil.copyfile(src, d / out)
        images.append(out)
    record = dict(FIELDS, id=rid, name=name, images=images, **fields)
    return save(root, normal(record))


# ---------------------------------------------------------------- the command line

def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    root = find_root(Path.cwd())
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0

    def option(name, default=None):
        if name in argv:
            i = argv.index(name)
            value = argv[i + 1]
            del argv[i:i + 2]
            return value
        return default

    verb = argv.pop(0)
    if verb == "list":
        for r in every(root):
            extra = f" · {bar_text(r)}" if r["status"] in ("in_progress", "failed") else (f" → {r['prop']}" if r["prop"] else "")
            print(f"{r['status']:<12} {r['id']:<24} {r['name']} ({r['kind']}, {r['height']} cells, {r['style']}{', mine' if r['mine'] else ''}), {len(r['images'])} picture{'' if len(r['images']) == 1 else 's'}{extra}")
    elif verb == "start":
        minutes = option("--minutes")
        r = start(root, argv[0], minutes)
        print(f"{r['id']}: in progress, about {round(r['progress']['minutes'])} min")
    elif verb == "step":
        done = option("--done")
        r = step(root, argv[0], argv[1], float(done) if done else None)
        print(f"{r['id']}: {bar_text(r)}")
    elif verb == "wait":
        note = option("--note")
        if note:
            log(root, argv[0], note)
        r = update(root, argv[0], status="waiting", progress=dict(FIELDS["progress"]))
        print(f"{r['id']}: waiting")
    elif verb == "fail":
        r = fail(root, argv[0], argv[1])
        print(f"{r['id']}: {r['progress']['step']}")
    elif verb == "note":
        log(root, argv[0], argv[1])
    elif verb == "new":
        fields = {"kind": option("--kind", "creature"), "height": float(option("--height", "1.0")), "style": option("--style", "smooth"), "notes": option("--notes", ""),
                  "details": option("--details", ""), "pose": option("--pose", "t_pose")}
        shapes = option("--shapes", "8-30")
        lo, _, hi = shapes.partition("-")
        fields["shapes"] = {"min": 1, "max": SHAPES_CAP, "unlimited": True} if shapes == "unlimited" else {"min": int(lo), "max": int(hi or lo), "unlimited": False}
        if "--mine" in argv:
            argv.remove("--mine")
            fields["mine"] = True
        if fields["kind"] not in KINDS or fields["style"] not in STYLES or fields["pose"] not in POSES:
            raise SystemExit(f"kind: {' or '.join(KINDS)}; style: {' or '.join(STYLES)}; pose: {' or '.join(POSES)}")
        r = new(root, argv[0], argv[1:], **fields)
        print(f"{r['id']}: a new request, {len(r['images'])} picture{'' if len(r['images']) == 1 else 's'} (private/requests/{r['id']}/)")
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
