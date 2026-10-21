"""Model requests on disk: what the map editor's Requests window keeps in
private/requests/<id>/ (request.json and the reference pictures), read and written from plain
Python, and from Blender through request_kit.py. tools/blender/requests.md is the recipe.

    python tools/blender/model_requests.py list
    python tools/blender/model_requests.py start ID [--minutes N]    in progress: the bar starts
    python tools/blender/model_requests.py step ID "what it's doing" [--done 0.4]
    python tools/blender/model_requests.py wait ID [--note "why"]    back to waiting (to remake)
    python tools/blender/model_requests.py wait ID --check           waiting for the user to check
                                              its lines ("Let me check the lines first")
    python tools/blender/model_requests.py fail ID "why"             failed, and why
    python tools/blender/model_requests.py note ID "what happened"   a line in its log
    python tools/blender/model_requests.py zoom ID PICTURE [X0 Y0 X1 Y1] [--scale N] [--grid] [--lines]
                                              a part of a picture, enlarged (no box: its size);
                                              --grid: the picture's pixels marked on it,
                                              --lines: its lines from lines.json drawn on it
    python tools/blender/model_requests.py bones ID
                                              the bones the user has drawn over the pictures (the
                                              editor's Bones window): each picture's drawn
                                              (bones_PICTURE.png) and listed, with what lines.json
                                              says each is on (request_lines.py)
    python tools/blender/model_requests.py lines ID
                                              lines.json drawn: each picture's lines over it
                                              (how far its outline and the picture's silhouette
                                              overlap), the front and the side in cells
                                              (request_lines.py)
    python tools/blender/model_requests.py outlines ID
                                              the drawings the user has outlined on the pictures
                                              (Process multiple), each numbered over its picture
                                              (outlines_PICTURE.png), with its height and cells
    python tools/blender/model_requests.py split ID --outline K --name "NAME" [--notes "..."]
                                              outline K made a request of its own: its picture
                                              cut to the outline, its height and cells the user's
    python tools/blender/model_requests.py colours ID PICTURE [X0 Y0 X1 Y1] [--count 8]
                                              a picture's (or a box's) main colours: each one's
                                              share, hex, name and place
    python tools/blender/model_requests.py new "Name" PICTURE... [--kind prop|grass|environment]
                                              [--height 1.2] [--cells 1x1] [--style low_poly|accurate]
                                              [--details "..."] [--pose as_is] [--shapes 8-30|unlimited]
                                              [--notes "..."] [--variants 2] [--vary "size, colours"]
                                              [--mine] [--check-lines]

request.json is laid out as the project's records are (content::to_text): the editor and this
file write the same text for the same request. Each request's log.txt (what its runs did,
each line timed, the computer's own clock) is written here too: run_claude.py and the
editor's buttons go through `log`.

In the cloud (a sandbox of Anthropic's, which the web editor's runs go in: cloud_run.py), each
change to a request and each line of its log is printed too, as `[request] {...}` and
`[request-log] {...}`: the editor follows the run by them.
"""
import json
import os
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
    # grass: the cells it fills to their edges (across x, deep z)
    "cells": [1, 1],
    "style": "smooth",
    "details": "",
    "pose": "t_pose",
    "shapes": {"min": 8, "max": 30, "unlimited": False},
    "notes": "",
    # how many variants of it to make besides it, each a prop of its own (with several of them, of
    # each drawing), and how they differ ("": as Claude sees fit)
    "variants": 0,
    "vary": "",
    "mine": False,
    "check_lines": False,
    "status": "waiting",
    "progress": {"step": "", "done": 0.0, "started": 0.0, "minutes": 0.0},
    "images": [],
    "bones": {},
    # with several (the editor's "Process multiple"), the drawings the user has outlined on the
    # pictures, by picture: [{"points": [[x, y]...], "height": 1.2, "cells": [1, 1]}], only those
    # made; none: all of them
    "outlines": {},
    "lines": [],
    "previews": [],
    "prop": "",
    # once its variants are made, the props they became
    "variant_props": [],
    "reply": "",
    # what the user says of the model the last run made, for the next one (the editor's "Make it
    # again with these comments"), and what the runs made again before were told, oldest first
    "comments": "",
    "earlier": [],
    # the pictures hold several drawings of the same kind of asset (trees, bushes, rocks): the
    # first run splits it, each drawing a request of its own (`children`, each one's `parent`
    # this one), made one after another
    "several": False,
    "parent": "",
    "children": [],
}
# a creature (rigged), a prop, grass (filling its cells to their edges), an environment asset
# (rocks, bushes, flowers, logs, trees), or a set of floor tiles (laid by wave function collapse)
KINDS = ("creature", "prop", "grass", "environment", "floor")
# the kinds whose settings ask for variants from the first run on (any made one can have some)
VARYING = ("grass", "environment")
# the most variants a request asks for (the editor's VARIANTS_CAP)
VARIANTS_CAP = 12
# grass is grown over whole fields (the engine grows it through tall ground, one to a cell) and
# drawn thinner further off by leaving out whole pieces of it (world::grass): the most vertices
# it may have a cell as made, and at its thinnest (a piece, with any mat as wide as its cells),
# as the engine counts them (its .glb's: flat faces and seams split vertices); the engine's
# world::mesh::GROWN_HEAVIEST and GROWN_MOST: more and it doesn't grow
GRASS_MOST = 60000
GRASS_THINNEST = 1000
STYLES = ("smooth", "low_poly", "accurate")
POSES = ("t_pose", "as_is")
# "unlimited" shapes stop at about this many
SHAPES_CAP = 100
# in the editor's order: waiting first, failed ones above the done
STATUSES = ("waiting", "in_progress", "failed", "done")
# the step of a request waiting for the user to check its lines (the editor's LINES_READY)
LINES_READY = "lines ready: check them"
# about how much of a run the lines take (reading the pictures, the lines over them, the front
# and the side); the model takes the rest
LINES_PART = 0.4
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


# ---------------------------------------------------------------- in the cloud

# what cloud_run.py's setup says of the sandbox a run's in (Blender's program there): only there
CLOUD = Path("private") / "requests" / ".cloud.json"


def cloud(root):
    """The sandbox's setup, when the run's in the cloud (the web editor's); else None."""
    try:
        return json.loads((Path(root) / CLOUD).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def told(root, what, value):
    """In the cloud: a line the editor follows the run by (a request's fields as they're
    written, a line of its log), printed as it comes."""
    if cloud(root) is not None:
        print(f"[{what}] " + json.dumps(value, ensure_ascii=False), flush=True)


def slug(text):
    return re.sub(r"[^a-z0-9]+", "_", text.lower()).strip("_")


def normal(record):
    """The record with every field, in the editor's order, numbers as the editor writes them."""
    out = {}
    for k, default in FIELDS.items():
        v = record.get(k, default)
        if isinstance(default, dict) and default:
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


def save(root, record, said=True):
    path = folder(root, record["id"]) / "request.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(to_text(normal(record)), encoding="utf-8", newline="\n")
    if said:
        told(root, "request", normal(record))
    return record


def update(root, rid, **fields):
    """Read again, changed, written at once: what the editor changed meanwhile stays."""
    record = load(root, rid)
    record.update(fields)
    told(root, "request", dict(fields, id=rid))
    return save(root, normal(record), said=False)


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


def free(path):
    """A picture's name made free for the one about to be written there: what had it is moved
    aside first, hidden (.held_N_NAME), and taken away, unless a program still shows it (Windows'
    Photos keeps the picture it shows open, and nothing can write over it then; a moved one
    goes once it's let go, the next time a picture is written here). Returns the path."""
    path = Path(path)
    for old in path.parent.glob(".held_*"):
        try:
            old.unlink()
        except OSError:
            pass  # still shown
    if path.is_file():
        n = 1
        while path.with_name(f".held_{n}_{path.name}").exists():
            n += 1
        held = path.with_name(f".held_{n}_{path.name}")
        try:
            os.replace(path, held)
        except OSError:
            return path  # it can't be moved either: the write says why
        try:
            held.unlink()
        except OSError:
            pass
    return path


# ---------------------------------------------------------------- the log: log.txt, each line timed

def stamp():
    return time.strftime("%Y-%m-%d %H:%M:%S")


def log(root, rid, text):
    """Lines added to the request's log.txt: `[2026-10-16 09:41:07] text`, a text of several
    lines indented under its time."""
    path = folder(root, rid) / "log.txt"
    path.parent.mkdir(parents=True, exist_ok=True)
    told(root, "request-log", {"id": rid, "text": str(text)})
    at = stamp()
    lines = str(text).splitlines() or [""]
    with open(path, "a", encoding="utf-8", newline="\n") as f:
        f.write(f"[{at}] {lines[0]}\n")
        for line in lines[1:]:
            f.write(" " * (len(at) + 3) + line + "\n")


# ---------------------------------------------------------------- progress: the editor's bar

def estimate(record, part="all"):
    """Minutes a request takes to model, by default: a session's whole time on it (reading the
    pictures, the script, the tries and renders, the rig, adding it); with `part` "lines" only
    up to the front and the side (a run that stops for the user's check), "model" the rest,
    "again" a model made again from the last one, with the user's comments (about as long),
    "split" the drawings found and made requests of their own (a few minutes, whatever the
    style), "variants" variants of the model made (a part of the model's time each). A run
    that makes the model makes its variants too: a part more for each."""
    if part == "split":
        return 3.0
    # what the first runs took: a creature about 10 minutes, a small one 2
    minutes = 12.0 if record["kind"] == "creature" else 8.0
    minutes *= {"low_poly": 0.8, "accurate": 2.5}.get(record["style"], 1.0)
    # as many shapes as it needs: worked out first, written out long
    minutes *= 1.5 if record["shapes"]["unlimited"] else 1.0
    variants = int(record.get("variants") or 0) if record["kind"] != "floor" else 0
    if part == "variants":
        return float(max(1, round(minutes * (1.0 - LINES_PART) * (0.3 + 0.25 * max(variants, 1)))))
    minutes *= {"lines": LINES_PART, "model": 1.0 - LINES_PART, "again": 1.0 - LINES_PART}.get(part, 1.0)  # ("floor": the whole run)
    if part != "lines":
        minutes *= 1.0 + 0.25 * variants
    return float(max(1, round(minutes)))


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


def lines_ready(record):
    """Waiting for the user to check its lines: the editor shows them with "The lines look
    right: go on", which runs run_claude.py --go-on (on from the model, on those lines)."""
    return record["status"] == "waiting" and record["progress"]["step"] == LINES_READY


def wait_for_check(root, rid):
    """A request that checks its lines first, its lines and its front and side drawn: waiting
    for the user (the bar's start kept, its minutes how long the lines took)."""
    record = load(root, rid)
    p = dict(record["progress"])
    took = (time.time() - p["started"]) / 60.0 if p["started"] > 0 else 0.0
    p.update(step=LINES_READY, minutes=float(round(took, 1)))
    log(root, rid, "Lines ready: waiting for the user to check them (the editor's \"The lines look right: go on\" goes on from the model)")
    return update(root, rid, status="waiting", progress=p)


def fail(root, rid, reason):
    """Failed, and why (the editor shows it; Start again makes it once more)."""
    record = load(root, rid)
    p = dict(record["progress"], step=f"failed: {reason}")
    log(root, rid, f"FAILED: {reason}")
    return update(root, rid, status="failed", progress=p)


def finish(root, rid, prop, previews, reply=None, variant_props=None):
    """Done: the prop it became, its renders, how long it took, and (when the run made any) the
    props its variants became."""
    record = load(root, rid)
    p = dict(record["progress"])
    took = (time.time() - p["started"]) / 60.0 if p["started"] > 0 else p["minutes"]
    p.update(step="done", done=1.0, minutes=float(round(took, 1)))
    fields = {"status": "done", "progress": p, "prop": prop, "previews": list(previews)}
    if reply is not None:
        fields["reply"] = reply
    if variant_props is not None:
        fields["variant_props"] = list(variant_props)
    return update(root, rid, **fields)


def _glb(path):
    """A .glb's JSON and its binary chunk."""
    data = Path(path).read_bytes()
    size = int.from_bytes(data[12:16], "little")
    gltf = json.loads(data[20:20 + size].decode("utf-8"))
    rest = 20 + size
    blob = data[rest + 8:rest + 8 + int.from_bytes(data[rest:rest + 4], "little")] if len(data) > rest + 8 else b""
    return gltf, blob


def glb_vertices(path):
    """A .glb's vertices as the engine reads them: each node's mesh, every primitive of it."""
    gltf, _ = _glb(path)
    accessors, meshes = gltf.get("accessors", []), gltf.get("meshes", [])
    return sum(accessors[p["attributes"]["POSITION"]]["count"] for node in gltf.get("nodes", []) if "mesh" in node
               for p in meshes[node["mesh"]]["primitives"] if "POSITION" in p.get("attributes", {}))


def glb_pieces(path, cells=(1, 1)):
    """A .glb's pieces as the engine thins it (world::grass): each primitive's triangles joined
    through the points they share (welded where they stand), each piece's vertices and whether
    it's a mat (half its cells wide or more each way, the node's scale in: on every level)."""
    import numpy as np

    gltf, blob = _glb(path)
    accessors, views = gltf.get("accessors", []), gltf.get("bufferViews", [])
    kinds = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}

    def read(k, width):
        a = accessors[k]
        v = views[a["bufferView"]]
        dtype = np.dtype(kinds[a["componentType"]])
        start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        stride = v.get("byteStride") or dtype.itemsize * width
        rows = np.frombuffer(blob, dtype=np.uint8, count=stride * a["count"] - (stride - dtype.itemsize * width), offset=start)
        rows = np.lib.stride_tricks.as_strided(rows, shape=(a["count"], dtype.itemsize * width), strides=(stride, 1))
        return np.ascontiguousarray(rows).view(dtype).reshape(a["count"], width)

    out = []
    for node in (n for n in gltf.get("nodes", []) if "mesh" in n):
        scale = np.array(node.get("scale", [1.0, 1.0, 1.0]), dtype=np.float64)
        for p in gltf["meshes"][node["mesh"]]["primitives"]:
            if "POSITION" not in p.get("attributes", {}) or "indices" not in p:
                continue
            pos = read(p["attributes"]["POSITION"], 3).astype(np.float64) * scale
            tris = read(p["indices"], 1).reshape(-1, 3).astype(np.int64)
            _, weld = np.unique(np.round(pos * 1e4).astype(np.int64), axis=0, return_inverse=True)
            weld = weld.reshape(-1)
            parent = np.arange(weld.max() + 1)

            def root(i):
                while parent[i] != i:
                    parent[i] = parent[parent[i]]
                    i = parent[i]
                return i

            for a, b, c in weld[tris]:
                ra = root(a)
                for x in (b, c):
                    rx = root(x)
                    if rx != ra:
                        parent[rx] = ra
            used = np.unique(tris.reshape(-1))
            roots = np.array([root(w) for w in weld[used]])
            for r in np.unique(roots):
                mine = used[roots == r]
                # (glTF is y-up: x across, z deep)
                size = pos[mine].max(axis=0) - pos[mine].min(axis=0)
                out.append((int(len(mine)), bool(size[0] >= 0.5 * cells[0] and size[2] >= 0.5 * cells[1])))
    return out


def variant_id(record, k):
    """The prop the request's variant `k` (2 on: the model itself is the first) becomes: its own
    prop's id (else the request's), "_variant_k"."""
    return f"{record['prop'] or record['id']}_variant_{k}"


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


def outlined(record):
    """The drawings the user has outlined on a request's pictures, in turn: picture by picture in
    its pictures' order, each picture's in the order drawn (numbered 1 on, as the editor numbers
    them): [(picture, outline)]."""
    return [(pic, o) for pic in record["images"] for o in (record.get("outlines") or {}).get(pic, [])]


def _around(points, size, margin=0.04):
    """The box round an outline, with a little room (inside the picture)."""
    xs, ys = [q[0] for q in points], [q[1] for q in points]
    pad = max(4, round(margin * max(size)))
    return [max(0, min(xs) - pad), max(0, min(ys) - pad), min(size[0], max(xs) + pad), min(size[1], max(ys) + pad)]


def split(root, rid, picture, box, name, height=None, notes="", cells=None, outline=None):
    """One of the drawings on a request's picture (one whose pictures hold several of the same
    kind of asset: `several`) made a request of its own: that box of the picture its picture
    (in pixels, or fractions of the picture when all four are at most 1), named, `height`
    cells tall (else the request's), on `cells` (else the request's), with the request's
    settings and notes, `notes` of its own and where on the picture it was; its `parent` the
    request, whose `children` gets it. Cut to an `outline` (the user's: its points in the
    picture's pixels), the box is the outline's, with a little room, and what's outside the
    outline is left out (clear). Returns the new request."""
    Image = _pillow("split")
    parent = load(root, rid)
    img = _picture(root, rid, picture, Image)
    points = [tuple(q) for q in outline["points"]] if outline else None
    x0, y0, x1, y1 = _box(img.size, box if box is not None else _around(points, img.size))
    cid = free_id(root, f"{rid}_{slug(name)}")
    d = folder(root, cid)
    d.mkdir(parents=True)
    out = f"{slug(name) or 'drawing'}.png"
    crop = img.crop((x0, y0, x1, y1))
    if points:
        from PIL import ImageChops, ImageDraw
        crop = crop.convert("RGBA")
        inside = Image.new("L", crop.size, 0)
        ImageDraw.Draw(inside).polygon([(x - x0, y - y0) for x, y in points], fill=255)
        crop.putalpha(ImageChops.multiply(crop.getchannel("A"), inside))
    else:
        crop = crop.convert("RGBA" if img.mode in ("RGBA", "LA", "P") else "RGB")
    crop.save(d / out)
    whence = (f"{name} is one of the drawings on {parent['name']}'s picture (private/requests/{rid}/{picture}, from ({x0}, {y0}) to "
              f"({x1}, {y1})" + (", the user's outline round it: what's outside it left out" if points else "") + "): model this one only; the "
              "whole picture shows the style it shares with the others.")
    kept = {k: parent[k] for k in ("kind", "cells", "style", "details", "pose", "shapes", "variants", "vary", "mine", "check_lines")}
    if cells:
        kept["cells"] = [int(cells[0]), int(cells[1])]
    record = dict(FIELDS, id=cid, name=name, images=[out], parent=rid, height=float(height) if height else parent["height"],
                  notes=" ".join(t for t in (parent["notes"].strip(), notes.strip(), whence) if t), **kept)
    save(root, normal(record))
    update(root, rid, children=load(root, rid)["children"] + [cid])
    log(root, rid, f"Split off {cid}: {name}, {record['height']} cells, from {picture} ({x0}, {y0}) to ({x1}, {y1})")
    return load(root, cid)


def split_outlined(root, rid, k, name, notes=""):
    """The user's outline `k` (1 on, as `outlined` numbers them) made a request of its own: its
    picture cut to the outline, as tall and on the cells the user said."""
    shapes = outlined(load(root, rid))
    if not 1 <= k <= len(shapes):
        raise SystemExit(f"{rid} has {len(shapes)} outline{'' if len(shapes) == 1 else 's'}: there's no {k}")
    picture, o = shapes[k - 1]
    return split(root, rid, picture, None, name, o.get("height") or None, notes, cells=o.get("cells"), outline=o)


OUTLINE_COLOURS = [(255, 196, 64), (90, 200, 255), (255, 110, 160), (140, 230, 120), (200, 150, 255), (255, 150, 90), (120, 220, 210), (240, 240, 120)]


def draw_outlines(root, rid):
    """The command: the drawings the user has outlined over each picture, each numbered in its
    colour (outlines_<picture>.png). Returns what to say: each one's number, picture, box,
    height and cells."""
    Image = _pillow("outlines")
    from PIL import ImageDraw, ImageFont
    record = load(root, rid)
    shapes = outlined(record)
    if not shapes:
        return ["The user has outlined no drawings: every drawing on the pictures is made."]
    said = []
    for pic in record["images"]:
        mine = [(k, o) for k, (p, o) in enumerate(shapes, 1) if p == pic]
        if not mine:
            continue
        img = _picture(root, rid, pic, Image).convert("RGB")
        scale = max(1.0, min(4.0, 1100.0 / max(img.size)))
        drawing = img.resize((round(img.width * scale), round(img.height * scale)))
        pen = ImageDraw.Draw(drawing)
        font = ImageFont.load_default()
        for k, o in mine:
            colour = OUTLINE_COLOURS[(k - 1) % len(OUTLINE_COLOURS)]
            pts = [(x * scale, y * scale) for x, y in o["points"]]
            pen.line(pts + pts[:1], fill=(20, 20, 24), width=6)
            pen.line(pts + pts[:1], fill=colour, width=3)
            top = min(pts, key=lambda q: q[1])
            pen.rectangle([top[0] - 2, top[1] - 18, top[0] + 18, top[1] - 4], fill=(20, 20, 24))
            pen.text((top[0], top[1] - 17), str(k), fill=colour, font=font)
            x0, y0, x1, y1 = _around(o["points"], img.size)
            said.append(f"{k}. on {pic}, from ({x0}, {y0}) to ({x1}, {y1}): {o.get('height', record['height']):g} cells tall, "
                        f"on {o.get('cells', [1, 1])[0]} x {o.get('cells', [1, 1])[1]} cells")
        out = folder(root, rid) / f"outlines_{Path(pic).stem}.png"
        drawing.save(out)
        said.append(f"wrote {out.name}")
    return said


# ---------------------------------------------------------------- a closer look at a picture

def _pillow(what):
    try:
        from PIL import Image
    except ImportError:
        raise SystemExit(f"{what} needs Pillow: pip install pillow")
    return Image


def _picture(root, rid, picture, Image):
    src = folder(root, rid) / picture
    if not src.is_file():
        raise SystemExit(f"{src} isn't there: the request's pictures are {', '.join(load(root, rid)['images'])}")
    return Image.open(src)


def _box(size, box):
    """A box of a picture in whole pixels: given in pixels (x0 y0 x1 y1), or in fractions of
    the picture when all four are at most 1."""
    w, h = size
    x0, y0, x1, y1 = box
    if max(box) <= 1.0:
        x0, y0, x1, y1 = x0 * w, y0 * h, x1 * w, y1 * h
    x0, x1 = sorted((max(0, int(x0)), min(w, int(round(x1)))))
    y0, y1 = sorted((max(0, int(y0)), min(h, int(round(y1)))))
    if x1 - x0 < 2 or y1 - y0 < 2:
        raise SystemExit(f"the box {box} is empty in a {w} x {h} picture")
    return x0, y0, x1, y1


def zoom(root, rid, picture, box=None, scale=None, grid=False, lines=False):
    """A part of a request's picture, enlarged into its folder (zoom_<picture>_<n>.png, to look
    at closely; never one of the request's pictures): the box in pixels (x0 y0 x1 y1), or in
    fractions of the picture when all four are at most 1; `scale` times larger, else its longer
    side about 1000 pixels. With `grid`, the picture's own pixels marked along it (to read
    where things are); with `lines`, the picture's lines from lines.json drawn on it. With no
    box, only the picture's size. Returns (file or None, what to say)."""
    Image = _pillow("zoom")
    d = folder(root, rid)
    img = _picture(root, rid, picture, Image)
    w, h = img.size
    if box is None:
        return None, f"{picture}: {w} x {h} pixels (zoom ID {picture} X0 Y0 X1 Y1 enlarges a box of it)"
    x0, y0, x1, y1 = _box(img.size, box)
    k = float(scale) if scale else max(1.0, 1000.0 / max(x1 - x0, y1 - y0))
    if grid or lines:
        import request_lines as rl
        data = rl.read(root, rid) if lines else {}
        items = data.get("pictures", {}).get(picture, {}).get("lines", [])
        part = rl.draw_picture(img, (x0, y0, x1, y1), k, items, rl.pens(data, load(root, rid)["images"]), grid=grid)
    else:
        part = img.crop((x0, y0, x1, y1)).resize((max(1, round((x1 - x0) * k)), max(1, round((y1 - y0) * k))), Image.LANCZOS)
    stem = Path(picture).stem
    n = 1
    while (d / f"zoom_{stem}_{n}.png").exists():
        n += 1
    out = d / f"zoom_{stem}_{n}.png"
    part.convert("RGB").save(out)
    return out, f"{out.name}: {picture} from ({x0}, {y0}) to ({x1}, {y1}), {k:.1f} times, {part.size[0]} x {part.size[1]} pixels"


# ---------------------------------------------------------------- a picture's colours, measured

# each hue's name, up to its angle
HUES = ((15, "crimson"), (40, "orange"), (65, "yellow"), (160, "green"), (195, "cyan"), (255, "blue"), (290, "violet"), (335, "pink"), (360, "crimson"))
# what colours says under them: how to read them
READING = ("Light and shade change a colour's lightness (and an artist's shade often leans toward blue or purple): tones of one\n"
           "colour on one part are that part in light and shade, so give it its lit tone, neither its shine nor its shade; another\n"
           "colour with an edge of its own is a marking or another part.")


def colour_name(rgb):
    """A colour said plainly: "light violet grey", "dark brown", "near white"."""
    import colorsys
    r, g, b = (c / 255 for c in rgb)
    hue, lightness, _ = colorsys.rgb_to_hls(r, g, b)
    chroma = max(r, g, b) - min(r, g, b)
    tone = ("near black" if lightness < 0.12 else "dark" if lightness < 0.35 else "mid" if lightness < 0.62
            else "light" if lightness < 0.9 else "near white")
    name = next(n for angle, n in HUES if hue * 360 < angle)
    if name in ("orange", "yellow") and lightness < 0.5 and chroma >= 0.15:
        name = "brown"
    if tone.startswith("near"):
        return tone if chroma < 0.05 else f"{tone}, {name} tinted"
    if chroma < 0.05:
        return f"{tone} grey"
    return f"{tone} {name} grey" if chroma < 0.15 else f"{tone} {name}"


def _where(n, sx, sy, sxx, syy):
    """Where a colour's pixels mostly are, in a box measured 0 to 1 each way."""
    mx, my = sx / n, sy / n
    if sxx / n - mx * mx > 0.24 ** 2 and syy / n - my * my > 0.24 ** 2:
        return "all over"
    row = "top" if my < 1 / 3 else "bottom" if my > 2 / 3 else ""
    col = "left" if mx < 1 / 3 else "right" if mx > 2 / 3 else ""
    return " ".join(w for w in (row, col) if w) or "middle"


def palette(part, count=8):
    """A box of a picture (an RGBA one) in its main colours, the most covered first: [[rgb,
    [pixels, sum u, sum v, sum u², sum v², pixels on the box's edges]], ...] (u, v: where, 0 to
    1 across the box), and how many pixels were seen (see-through ones don't count)."""
    Image = _pillow("colours")
    # a sample of at most 300 x 300, nearest: the picture's own colours, not blends of them
    k = min(1.0, 300.0 / max(part.size))
    if k < 1.0:
        part = part.resize((max(1, round(part.width * k)), max(1, round(part.height * k))), Image.NEAREST)
    pw, ph = part.size
    data = part.tobytes()
    seen = [(i % pw, i // pw, data[j:j + 3]) for i, j in enumerate(range(0, len(data), 4)) if data[j + 3] >= 128]
    if not seen:
        return [], 0
    strip = Image.frombytes("RGB", (len(seen), 1), b"".join(p for _, _, p in seen))
    q = strip.quantize(colors=max(2, min(16, int(count))), method=Image.Quantize.MEDIANCUT, kmeans=4)
    table = q.getpalette()
    found = {}
    for (x, y, _), i in zip(seen, q.tobytes()):
        u, v = (x + 0.5) / pw, (y + 0.5) / ph
        f = found.setdefault(i, [0, 0.0, 0.0, 0.0, 0.0, 0])
        f[0] += 1
        f[1] += u
        f[2] += v
        f[3] += u * u
        f[4] += v * v
        f[5] += x in (0, pw - 1) or y in (0, ph - 1)
    # the quantizer can split one colour in two: those join again
    kept = []
    for i, f in sorted(found.items(), key=lambda kv: -kv[1][0]):
        rgb = table[i * 3:i * 3 + 3]
        same = next((c for c in kept if sum((a - b) ** 2 for a, b in zip(c[0], rgb)) < 14 ** 2), None)
        if same is None:
            kept.append([list(rgb), f])
        else:
            n = same[1][0] + f[0]
            same[0] = [round((a * same[1][0] + b * f[0]) / n) for a, b in zip(same[0], rgb)]
            same[1] = [a + b for a, b in zip(same[1], f)]
    return kept, len(seen)


def background(kept):
    """Of a palette, the colour along most of the box's edges (the likely background), or None
    when no colour has as much as two fifths of them."""
    edges = sum(f[5] for _, f in kept)
    found = max(kept, key=lambda c: c[1][5]) if edges else None
    return found[0] if found is not None and found[1][5] >= 0.4 * edges else None


def colours(root, rid, picture, box=None, count=8):
    """The main colours of a request's picture, or of a box of it (as zoom's), to take a part's
    colour from the picture rather than guess it: the most covered first, each with its share,
    its hex (as the kit's colours take it), its name and where it mostly is; the colour along
    most of the box's edges said to be the likely background. See-through pixels don't count.
    Returns the lines to say."""
    Image = _pillow("colours")
    img = _picture(root, rid, picture, Image).convert("RGBA")
    x0, y0, x1, y1 = _box(img.size, box) if box else (0, 0, *img.size)
    kept, seen = palette(img.crop((x0, y0, x1, y1)), count)
    if not seen:
        raise SystemExit(f"{picture}: nothing but see-through pixels there")
    bg = background(kept)
    out = [f"{picture}, from ({x0}, {y0}) to ({x1}, {y1}): its main colours, the most covered first"]
    for rgb, f in kept:
        share = f[0] / seen
        if share < 0.005:
            continue
        hexa = "#%02x%02x%02x" % tuple(rgb)
        extra = ", along most of the box's edges: the background?" if rgb == bg else ""
        out.append(f"  {share * 100:4.1f}%  {hexa}  {colour_name(rgb)}, {_where(*f[:5])}{extra}")
    return out + [READING]


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
            extra = (f" · {LINES_READY} (the user's turn)" if lines_ready(r) else f" · {bar_text(r)}" if r["status"] in ("in_progress", "failed")
                     else (f" → {r['prop']}" if r["prop"] else ""))
            settings = r["style"] + (", mine" if r["mine"] else "") + (", lines checked first" if r["check_lines"] else "")
            drawn = sum(len(b) for b in (r.get("bones") or {}).values())
            bones = f", {drawn} bone line{'' if drawn == 1 else 's'} drawn by the user" if drawn else ""
            print(f"{r['status']:<12} {r['id']:<24} {r['name']} ({r['kind']}, {r['height']} cells, {settings}), {len(r['images'])} picture{'' if len(r['images']) == 1 else 's'}{bones}{extra}")
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
        check = "--check" in argv
        argv = [a for a in argv if a != "--check"]
        if note:
            log(root, argv[0], note)
        if check:
            r = wait_for_check(root, argv[0])
            print(f"{r['id']}: waiting, {LINES_READY}")
        else:
            r = update(root, argv[0], status="waiting", progress=dict(FIELDS["progress"]))
            print(f"{r['id']}: waiting")
    elif verb == "fail":
        r = fail(root, argv[0], argv[1])
        print(f"{r['id']}: {r['progress']['step']}")
    elif verb == "note":
        log(root, argv[0], argv[1])
    elif verb == "zoom":
        scale = option("--scale")
        flags = {f: f"--{f}" in argv for f in ("grid", "lines")}
        argv = [a for a in argv if a not in ("--grid", "--lines")]
        box = [float(v) for v in argv[2:6]] if len(argv) >= 6 else None
        try:
            _, said = zoom(root, argv[0], argv[1], box, scale, **flags)
        except ValueError as e:
            print(e)
            return 1
        print(said)
    elif verb == "split":
        height = option("--height")
        name = option("--name")
        notes = option("--notes", "")
        which = option("--outline")
        if name and which and argv:
            r = split_outlined(root, argv[0], int(which), name, notes)
        elif not name or len(argv) < 6:
            print("split ID PICTURE X0 Y0 X1 Y1 --name NAME [--height H] [--notes \"what's only its\"]\n"
                  "split ID --outline K --name NAME [--notes \"what's only its\"]")
            return 1
        else:
            r = split(root, argv[0], argv[1], [float(v) for v in argv[2:6]], name, height, notes)
        print(f"{r['id']}: a new request, {r['name']}, {r['height']} cells, its picture {r['images'][0]} (private/requests/{r['id']}/)")
    elif verb == "lines":
        import request_lines as rl
        try:
            print("\n".join(rl.lines(root, argv[0])))
        except rl.LinesError as e:
            print(e)
            return 1
    elif verb == "bones":
        import request_lines as rl
        try:
            print("\n".join(rl.bones(root, argv[0])))
        except rl.LinesError as e:
            print(e)
            return 1
    elif verb == "outlines":
        print("\n".join(draw_outlines(root, argv[0])))
    elif verb == "colours":
        count = option("--count", "8")
        box = [float(v) for v in argv[2:6]] if len(argv) >= 6 else None
        print("\n".join(colours(root, argv[0], argv[1], box, int(count))))
    elif verb == "new":
        cells = option("--cells", "1x1").lower().split("x")
        fields = {"kind": option("--kind", "creature"), "height": float(option("--height", "1.0")), "cells": [int(cells[0]), int(cells[-1])], "style": option("--style", "smooth"),
                  "notes": option("--notes", ""), "details": option("--details", ""), "pose": option("--pose", "t_pose"),
                  "variants": max(0, min(VARIANTS_CAP, int(option("--variants", "0")))), "vary": option("--vary", "")}
        shapes = option("--shapes", "8-30")
        lo, _, hi = shapes.partition("-")
        fields["shapes"] = {"min": 1, "max": SHAPES_CAP, "unlimited": True} if shapes == "unlimited" else {"min": int(lo), "max": int(hi or lo), "unlimited": False}
        for flag, field in (("--mine", "mine"), ("--check-lines", "check_lines")):
            if flag in argv:
                argv.remove(flag)
                fields[field] = True
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
