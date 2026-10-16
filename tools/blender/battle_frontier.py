# Battle Frontier assets: a Blender add-on (Blender 3.1 and newer).
#
# Brings the game's props into Blender (their own glTF models, or their stand-ins rebuilt
# from their shapes) and the project's exported maps and buildings; and adds what you made
# to the project as a prop: the selection exported as content/assets/models/<id>.glb and a
# record content/data/props/<id>.json written next to it, footprint and height measured.
#
# Install: Edit > Preferences > Add-ons > Install... (Blender 4.2+: the menu at the top
# right, "Install from Disk...") and pick this file, then tick it. The panel is in the 3D
# view's sidebar (N), tab "Battle Frontier". The project is found by itself (the
# BATTLE_FRONTIER_ROOT variable, then your home folder and drives); you can also set it in
# the add-on's preferences.
#
# Conventions, the same as the engine's: one Blender unit is one cell (a metre), the asset
# stands on Z = 0 with the middle of its footprint at the origin, and its front faces the
# front view (-Y), which glTF and the engine call +Z.

bl_info = {
    "name": "Battle Frontier assets",
    "author": "the Battle Frontier contributors",
    "version": (1, 0, 0),
    "blender": (3, 1, 0),
    "location": "3D View > Sidebar (N) > Battle Frontier",
    "description": "The game's props, buildings and maps in Blender, and Blender models added to the game as props",
    "category": "Import-Export",
}

import json
import math
import os
import re
import shutil
import string
import subprocess
import time
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Vector

CATEGORIES = [
    ("decoration", "Decoration", ""),
    ("tree", "Tree", "fills tree areas; not scaled when placed"),
    ("flora", "Flora", "not scaled when placed"),
    ("rock", "Rock", ""),
    ("fence", "Fence", ""),
    ("furniture", "Furniture", "stands inside buildings"),
    ("stairs", "Stairs", ""),
    ("door", "Door", "its width in cells is the opening it fits"),
    ("window", "Window", "its width in cells is the opening it fits; sill: how high it starts"),
    ("wall", "Wall style", ""),
    ("pillar", "Pillar", "stretched to the building's height"),
    ("floor", "Floor tile", "laid on every cell of a floor"),
    ("roof", "Roof style", ""),
    ("wall_decoration", "Wall decoration", "anchored on outside walls"),
    ("character", "Character", ""),
]
PROP_KEYS = ["id", "name", "category", "footprint", "height", "sill", "pattern", "blocks", "unstackable", "parts", "model", "light", "sound", "folders"]
# a prop's light in Blender's units: 1000 W is an intensity of 1
WATTS_PER_INTENSITY = 1000.0
SKIP = {"node_modules", ".git", "build", "target", "appdata", "windows", "program files", "program files (x86)",
        "programdata", "$recycle.bin", "system volume information", "library", "temporary", "private"}

_found = {"root": None}


# ---------------------------------------------------------------- the project

def is_project(p):
    p = Path(p)
    return (p / "Trunk.toml").is_file() and (p / "content" / "data" / "props").is_dir() and (p / "crates" / "content").is_dir()


def search_roots():
    home = Path.home()
    roots = [home]
    for sub in ("Documents", "Desktop", "Projects", "source", "dev", "code", "git", "GitHub"):
        roots.append(home / sub)
    if os.name == "nt":
        roots += [Path(f"{d}:\\") for d in string.ascii_uppercase if os.path.isdir(f"{d}:\\")]
    else:
        roots += [Path("/mnt"), Path("/media"), Path("/opt")]
    return [r for r in roots if r.is_dir()]


def find_project(roots=None, depth=3, budget=6.0, quick=True):
    """The project folder: BATTLE_FRONTIER_ROOT, the folder this file is in (when it runs
    from the project), then a search a few folders deep."""
    if quick:
        env = os.environ.get("BATTLE_FRONTIER_ROOT")
        if env and is_project(env):
            return str(Path(env))
        for parent in Path(__file__).resolve().parents:
            if is_project(parent):
                return str(parent)
    start = time.time()
    queue = [(Path(r), 0) for r in (roots or search_roots())]
    seen = set()
    while queue and time.time() - start < budget:
        folder, d = queue.pop(0)
        key = str(folder).lower()
        if key in seen:
            continue
        seen.add(key)
        if is_project(folder):
            return str(folder)
        if d >= depth:
            continue
        try:
            for child in sorted(folder.iterdir()):
                if child.is_dir() and not child.name.startswith(".") and child.name.lower() not in SKIP:
                    queue.append((child, d + 1))
        except (PermissionError, OSError):
            continue
    return None


def prefs():
    addon = bpy.context.preferences.addons.get(__name__)
    return addon.preferences if addon else None


def project_root(search=True):
    p = prefs()
    for candidate in (p.project_root if p else "", os.environ.get("BATTLE_FRONTIER_ROOT", ""), _found["root"] or ""):
        if candidate and is_project(candidate):
            return Path(candidate)
    if search:
        found = find_project()
        if found:
            remember(found)
            return Path(found)
    return None


def remember(root):
    _found["root"] = root
    p = prefs()
    if p is not None and p.project_root != root:
        p.project_root = root


def props_dir(root):
    return Path(root) / "content" / "data" / "props"


def find_prop(root, pid):
    """A prop's record file and the space it's in ("content", or "temporary" for a
    placeholder), the game's own first; (None, None) when it's in neither."""
    for space in ("content", "temporary"):
        p = Path(root) / space / "data" / "props" / f"{pid}.json"
        if p.is_file():
            return p, space
    return None, None


def read_props(root):
    out = []
    for f in sorted(props_dir(root).glob("*.json")):
        try:
            out.append(json.loads(f.read_text(encoding="utf-8")))
        except (OSError, ValueError):
            continue
    return out


# ---------------------------------------------------------------- records, as the project writes them

def _scalar(v):
    return json.dumps(v, ensure_ascii=False)


def _flat(x):
    return not isinstance(x, (list, dict)) or (isinstance(x, list) and all(not isinstance(y, (list, dict)) for y in x))


def _inline(v):
    if isinstance(v, list):
        if all(_flat(x) for x in v):
            return "[" + ", ".join(_inline(x) for x in v) + "]"
        return None
    if isinstance(v, dict):
        if all(_flat(x) for x in v.values()):
            items = [_scalar(k) + ": " + _inline(x) for k, x in v.items()]
            return "{ " + ", ".join(items) + " }" if items else "{}"
        return None
    return _scalar(v)


def _pretty(v, indent=0):
    pad, inner = "  " * indent, "  " * (indent + 1)
    if isinstance(v, dict) and v:
        lines = [inner + _scalar(k) + ": " + _pretty(x, indent + 1) for k, x in v.items()]
        return "{\n" + ",\n".join(lines) + "\n" + pad + "}"
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
    """A record laid out as `content format` lays it out."""
    return _pretty(record) + "\n"


def ordered_prop(record):
    """The prop's fields in the engine's order, defaults left out as the engine leaves them."""
    out = {}
    for k in PROP_KEYS:
        if k not in record:
            continue
        v = record[k]
        if k == "sill" and not v:
            continue
        if k == "pattern" and not v:
            continue
        if k in ("blocks", "unstackable") and not v:
            continue
        if k == "folders" and not v:
            continue
        if k == "model" and not v:
            continue
        out[k] = v
    for k, v in record.items():
        if k not in out and k not in PROP_KEYS:
            out[k] = v
    out.setdefault("parts", [])
    return out


def slug(text):
    s = re.sub(r"[^a-z0-9]+", "_", text.lower()).strip("_")
    return s or "asset"


# ---------------------------------------------------------------- props into Blender

def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def colour_material(hex_colour):
    name = "BF " + hex_colour.lower()
    mat = bpy.data.materials.get(name)
    if mat:
        return mat
    v = int(hex_colour.lstrip("#"), 16)
    rgb = [srgb_to_linear(((v >> s) & 255) / 255.0) for s in (16, 8, 0)]
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*rgb, 1.0)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    if bsdf:
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.9
    return mat


class Rng:
    """The engine's generator (mulberry32), for the same rocks."""

    def __init__(self, seed):
        self.s = seed & 0xFFFFFFFF

    def next(self):
        self.s = (self.s + 0x6D2B79F5) & 0xFFFFFFFF
        t = self.s
        t = ((t ^ (t >> 15)) * (t | 1)) & 0xFFFFFFFF
        t ^= (t + (((t ^ (t >> 7)) * (t | 61)) & 0xFFFFFFFF)) & 0xFFFFFFFF
        return ((t ^ (t >> 14)) & 0xFFFFFFFF) / 4294967296.0

    def range(self, lo, hi):
        return lo + (hi - lo) * self.next()


def shape(kind, size, seed):
    """A part's geometry exactly as the engine makes it (crates/world/src/mesh.rs), in the
    engine's frame (x, up, south): (vertices, faces)."""
    sx, sy, sz = size
    if kind in ("cylinder", "cone"):
        segments, top = 10, (1.0 if kind == "cylinder" else 0.0)
        rx, rz = sx / 2.0, sz / 2.0
        ring = lambda k, y, r: (math.cos(k / segments * math.tau) * rx * r, y, math.sin(k / segments * math.tau) * rz * r)
        verts = [ring(k, 0.0, 1.0) for k in range(segments)]
        faces = []
        if top > 0.0:
            verts += [ring(k, sy, top) for k in range(segments)] + [(0.0, sy, 0.0)]
            centre = len(verts) - 1
            for k in range(segments):
                k1 = (k + 1) % segments
                faces.append((segments + k, segments + k1, k1, k))
                faces.append((centre, segments + k1, segments + k))
        else:
            verts.append((0.0, sy, 0.0))
            apex = len(verts) - 1
            for k in range(segments):
                faces.append((apex, (k + 1) % segments, k))
        return verts, faces
    if kind == "sphere":
        rings, segments = 6, 10
        r = (sx / 2.0, sy / 2.0, sz / 2.0)

        def point(i, k):
            v, a = i / rings * math.pi, k / segments * math.tau
            u = (math.sin(v) * math.cos(a), -math.cos(v), math.sin(v) * math.sin(a))
            return (u[0] * r[0], r[1] + u[1] * r[1], u[2] * r[2])
        verts = [point(0, 0)] + [point(i, k) for i in range(1, rings) for k in range(segments)] + [point(rings, 0)]
        at = lambda i, k: 0 if i == 0 else (len(verts) - 1 if i == rings else 1 + (i - 1) * segments + k % segments)
        faces = []
        for i in range(rings):
            for k in range(segments):
                a, b, c, d = at(i, k), at(i + 1, k), at(i + 1, k + 1), at(i, k + 1)
                if i == 0:
                    faces.append((a, b, c))
                elif i == rings - 1:
                    faces.append((a, b, d))
                else:
                    faces.append((a, b, c, d))
        return verts, faces
    if kind == "rock":
        t = (1.0 + math.sqrt(5.0)) / 2.0
        base = [(-1, t, 0), (1, t, 0), (-1, -t, 0), (1, -t, 0), (0, -1, t), (0, 1, t), (0, -1, -t), (0, 1, -t), (t, 0, -1), (t, 0, 1), (-t, 0, -1), (-t, 0, 1)]
        faces = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4), (11, 10, 2), (10, 7, 6), (7, 1, 8),
                 (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1)]
        rng = Rng(seed)
        verts = []
        for x, y, z in base:
            n = math.sqrt(x * x + y * y + z * z)
            k = rng.range(0.82, 1.08)
            px, py, pz = x / n * k, y / n * k, z / n * k
            py = max(py, -0.35)
            verts.append((px * sx / 2.0, (py + 0.35) / 1.35 * sy, pz * sz / 2.0))
        return verts, faces
    # a box: no bottom face, as in the engine
    x, y, z = sx / 2.0, sy, sz / 2.0
    verts = [(-x, 0, -z), (x, 0, -z), (x, 0, z), (-x, 0, z), (-x, y, -z), (x, y, -z), (x, y, z), (-x, y, z)]
    faces = [(7, 3, 2, 6), (5, 1, 0, 4), (6, 2, 1, 5), (4, 0, 3, 7), (4, 7, 6, 5)]
    return verts, faces


def to_blender(p):
    """The engine's (x, up, south) is Blender's (x, -y, z)."""
    return (p[0], -p[2], p[1])


def to_engine(v):
    """Blender's (x, y, z up) is the engine's (x, z, -y)."""
    return (v[0], v[2], -v[1])


def hex_to_linear(text):
    """"#rrggbb" (sRGB, as the records write colours) as Blender's linear floats."""
    try:
        n = int(text.lstrip("#"), 16)
    except ValueError:
        n = 0xff00ff
    out = []
    for shift in (16, 8, 0):
        s = ((n >> shift) & 255) / 255.0
        out.append(s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4)
    return tuple(out)


def linear_to_hex(rgb):
    """Blender's linear floats as "#rrggbb"."""
    out = "#"
    for v in rgb[:3]:
        v = min(1.0, max(0.0, float(v)))
        s = v * 12.92 if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055
        out += f"{int(round(s * 255)):02x}"
    return out


def gltf_options(op, **wanted):
    """Only the options this Blender's glTF add-on knows (the lighting mode came with 3.6:
    "Raw" keeps a light's watts as they are, which is all Blender 3.1 to 3.5 ever did)."""
    known = set(op.get_rna_type().properties.keys())
    return {k: v for k, v in wanted.items() if k in known}


def make_light(name, light, collection, parent=None):
    """A prop's `light` as a light object, where the record puts it: a spot or a point,
    its watts from the intensity, its cone from the spot size, its range as the custom
    distance."""
    kind = "POINT" if light.get("kind") == "point" else "SPOT"
    data = bpy.data.lights.new(name, kind)
    data.color = hex_to_linear(light.get("color", "#ffd59a"))
    data.energy = float(light.get("intensity", 1.0)) * WATTS_PER_INTENSITY
    data.use_custom_distance = True
    data.cutoff_distance = float(light.get("range", 6.0))
    if kind == "SPOT":
        data.spot_size = math.radians(min(180.0, max(1.0, float(light.get("cone", 100.0)))))
        data.spot_blend = 0.34
    o = bpy.data.objects.new(name, data)
    o.location = to_blender(light.get("at", [0.0, 2.0, 0.0]))
    aim = Vector(to_blender(light.get("aim", [0.0, -1.0, 0.0])))
    if aim.length > 1e-6:
        o.rotation_euler = aim.to_track_quat("-Z", "Y").to_euler()
    o["bf_light"] = True
    if parent is not None:
        o.parent = parent
    collection.objects.link(o)
    return o


def light_record(o, offset, existing=None):
    """A spot or point light object as a prop's `light`, from the prop's origin (`offset`,
    in Blender): its watts over 1000, its colour, its spot size as the cone, its custom
    distance as the range (else what the record had, or the engine's 6)."""
    data = o.data
    at = o.matrix_world.translation - offset
    aim = o.matrix_world.to_3x3() @ Vector((0.0, 0.0, -1.0))
    old = existing or {}
    spot = data.type == "SPOT"
    return {
        "kind": "spot" if spot else "point",
        "color": linear_to_hex(data.color),
        "intensity": round(data.energy / WATTS_PER_INTENSITY, 2),
        "range": round(float(data.cutoff_distance), 2) if data.use_custom_distance else float(old.get("range", 6.0)),
        "cone": round(math.degrees(data.spot_size), 1) if spot else 180.0,
        "at": [round(v, 2) + 0.0 for v in to_engine(at)],  # + 0.0: never a -0.0
        "aim": [round(v, 2) + 0.0 for v in to_engine(aim)] if spot else [0.0, -1.0, 0.0],
        "night": bool(old.get("night", True)),
    }


def sound_record(o, offset, root, aid, space="content"):
    """A speaker object as a prop's `sound`: its file copied to content/assets/sounds/, its
    volume, its maximum distance as the range (the engine's 8 when it has none). None when
    it plays nothing."""
    data = o.data
    if data.sound is None:
        return None
    src = Path(bpy.path.abspath(data.sound.filepath, library=data.sound.library))
    name = f"{aid}_{slug(src.stem) or 'sound'}{src.suffix.lower() or '.ogg'}"
    dst = Path(root) / space / "assets" / "sounds" / name
    dst.parent.mkdir(parents=True, exist_ok=True)
    if data.sound.packed_file is not None:
        dst.write_bytes(data.sound.packed_file.data)
    elif src.is_file():
        shutil.copyfile(src, dst)
    else:
        return None
    at = o.matrix_world.translation - offset
    return {
        "file": f"sounds/{name}",
        "volume": round(float(data.volume), 2),
        "range": round(float(data.distance_max), 2) if data.distance_max < 1e6 else 8.0,
        "at": [round(v, 2) + 0.0 for v in to_engine(at)],
    }


def build_stand_in(record, collection):
    """A prop's stand-in, exactly as the engine draws it, its parts in their colours."""
    bm = bmesh.new()
    mesh = bpy.data.meshes.new(record["id"])
    for i, part in enumerate(record.get("parts", [])):
        seed = (7 + i * 31 + len(record["id"])) & 0xFFFFFFFF
        verts, faces = shape(part.get("shape", "box"), part.get("size", [1, 1, 1]), seed)
        ax, ay, az = part.get("at", [0, 0, 0])
        turn = -math.radians(part.get("turn", 0.0))
        c, s_ = math.cos(turn), math.sin(turn)
        made = []
        for vx, vy, vz in verts:
            # turned about the vertical (the engine's rotation_y), then moved
            px, pz = c * vx + s_ * vz, -s_ * vx + c * vz
            made.append(bm.verts.new(to_blender((px + ax, vy + ay, pz + az))))
        mat = colour_material(part.get("color", "#888888"))
        if mat.name not in mesh.materials:
            mesh.materials.append(mat)
        index = list(mesh.materials).index(mat)
        for f in faces:
            try:
                face = bm.faces.new([made[k] for k in f])
                face.material_index = index
            except ValueError:
                continue  # a face the rock's flattening made twice
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(record["id"], mesh)
    collection.objects.link(obj)
    return [obj]


def make_bounds(name, footprint, height, collection, centre=(0.0, 0.0, 0.0)):
    """The cells an asset may cover and how high it goes, as a wireframe: the footprint's
    cells on the ground, the box above them. Never exported with the asset."""
    old = bpy.data.objects.get(name)
    if old is not None and old.get("bf_bounds"):
        bpy.data.objects.remove(old, do_unlink=True)
    w, d = max(1, int(footprint[0])), max(1, int(footprint[1]))
    h = max(0.05, float(height))
    cx, cy, cz = centre
    x0, y0 = cx - w / 2.0, cy - d / 2.0
    verts, edges = [], []

    def line(a, b):
        verts.extend([a, b])
        edges.append((len(verts) - 2, len(verts) - 1))
    for k in range(w + 1):
        line((x0 + k, y0, cz), (x0 + k, y0 + d, cz))
    for k in range(d + 1):
        line((x0, y0 + k, cz), (x0 + w, y0 + k, cz))
    for x, y in ((x0, y0), (x0 + w, y0), (x0 + w, y0 + d), (x0, y0 + d)):
        line((x, y, cz), (x, y, cz + h))
    for (ax_, ay_), (bx_, by_) in (((x0, y0), (x0 + w, y0)), ((x0 + w, y0), (x0 + w, y0 + d)), ((x0 + w, y0 + d), (x0, y0 + d)), ((x0, y0 + d), (x0, y0))):
        line((ax_, ay_, cz + h), (bx_, by_, cz + h))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, edges, [])
    obj = bpy.data.objects.new(name, mesh)
    obj["bf_bounds"] = True
    obj.display_type = "WIRE"
    obj.hide_render = True
    collection.objects.link(obj)
    return obj


def is_bounds(o):
    return bool(o.get("bf_bounds")) or o.name.endswith(" bounds")


def tag(objects, record):
    for o in objects:
        o["bf_prop"] = record["id"]
        o["bf_name"] = record.get("name") or record["id"]
        o["bf_category"] = record.get("category", "decoration")
        o["bf_blocks"] = bool(record.get("blocks", False))
        o["bf_unstackable"] = bool(record.get("unstackable", False))
        if record.get("sill"):
            o["bf_sill"] = record["sill"]


def import_glb(path):
    before = set(bpy.data.objects)
    # lights in their watts as the project writes them (1000 W = an intensity of 1)
    bpy.ops.import_scene.gltf(filepath=str(path), **gltf_options(bpy.ops.import_scene.gltf, export_import_convert_lighting_mode="RAW"))
    return [o for o in bpy.data.objects if o not in before]


# ---------------------------------------------------------------- operators

def prop_items(self, context):
    root = project_root(search=False)
    items = []
    if root:
        for r in read_props(root):
            label = f"{r.get('name') or r['id']} ({r.get('category', '')})"
            items.append((r["id"], label, "its own model" if r.get("model") else "a stand-in made of shapes"))
    _cache["props"] = items or [("", "(no props found)", "")]
    return _cache["props"]


def export_items(self, context):
    root = project_root(search=False)
    items = []
    if root:
        for f in sorted((Path(root) / "build" / "exports").glob("*.glb")):
            items.append((f.name, f.name, str(f)))
    _cache["exports"] = items or [("", "(nothing exported yet)", "")]
    return _cache["exports"]


def map_items(self, context):
    root = project_root(search=False)
    items = []
    if root:
        for f in sorted((Path(root) / "content" / "data" / "maps").glob("*.json")):
            items.append((f.stem, f.stem, ""))
    _cache["maps"] = items or [("", "(no maps)", "")]
    return _cache["maps"]


_cache = {}


class BF_OT_find_project(bpy.types.Operator):
    """Look for the Battle Frontier project on this computer"""
    bl_idname = "battle_frontier.find_project"
    bl_label = "Find the project"

    def execute(self, context):
        found = find_project()
        if not found:
            self.report({"WARNING"}, "No Battle Frontier project found: set its folder in the add-on's preferences")
            return {"CANCELLED"}
        remember(found)
        self.report({"INFO"}, f"Battle Frontier project: {found}")
        return {"FINISHED"}


class BF_OT_import_prop(bpy.types.Operator):
    """Bring a prop of the project into the scene, at the origin"""
    bl_idname = "battle_frontier.import_prop"
    bl_label = "Import the prop"
    bl_options = {"REGISTER", "UNDO"}

    prop_id: bpy.props.StringProperty(name="Prop")

    def execute(self, context):
        root = project_root()
        if not root:
            self.report({"ERROR"}, "No Battle Frontier project found")
            return {"CANCELLED"}
        pid = self.prop_id or context.window_manager.bf_prop
        path, space = find_prop(root, pid)
        if path is None:
            self.report({"ERROR"}, f"No prop {pid}")
            return {"CANCELLED"}
        record = json.loads(path.read_text(encoding="utf-8"))
        model = record.get("model")
        if model and (Path(root) / space / "assets" / model).is_file():
            objects = [o for o in import_glb(Path(root) / space / "assets" / model) if not is_bounds(o)]
        else:
            objects = build_stand_in(record, context.collection)
        # its light, unless its model brought one along
        if record.get("light") and not any(o.type == "LIGHT" for o in objects):
            objects.append(make_light(f"{pid} light", record["light"], context.collection, parent=next((o for o in objects if o.type == "MESH"), None)))
        tag([o for o in objects if o.type != "LIGHT"], record)
        make_bounds(f"{pid} bounds", record.get("footprint", [1, 1]), record.get("height", 1.0), context.collection)
        for o in context.view_layer.objects:
            o.select_set(False)
        for o in objects:
            o.select_set(True)
        if objects:
            context.view_layer.objects.active = objects[0]
        self.report({"INFO"}, f"{pid}: {'its model' if model else 'its stand-in'}")
        return {"FINISHED"}


class BF_OT_import_export(bpy.types.Operator):
    """Bring a map, building or prop exported by the game (build/exports) into the scene"""
    bl_idname = "battle_frontier.import_export"
    bl_label = "Import the export"
    bl_options = {"REGISTER", "UNDO"}

    file: bpy.props.StringProperty(name="File")

    def execute(self, context):
        root = project_root()
        name = self.file or context.window_manager.bf_export
        path = root / "build" / "exports" / name if root and name else None
        if not path or not path.is_file():
            self.report({"ERROR"}, "Nothing to import: export a map, building or prop from the game first")
            return {"CANCELLED"}
        objects = import_glb(path)
        self.report({"INFO"}, f"{name}: {len(objects)} objects")
        return {"FINISHED"}


class BF_OT_export_map(bpy.types.Operator):
    """Have the project export a map (it needs Rust's cargo), then import it"""
    bl_idname = "battle_frontier.export_map"
    bl_label = "Export and import the map"
    bl_options = {"REGISTER", "UNDO"}

    map_id: bpy.props.StringProperty(name="Map")

    def execute(self, context):
        root = project_root()
        mid = self.map_id or context.window_manager.bf_map
        if not root or not mid:
            self.report({"ERROR"}, "No project or no map")
            return {"CANCELLED"}
        cargo = shutil.which("cargo")
        if not cargo:
            self.report({"ERROR"}, "Rust's cargo isn't installed here: use the map editor's \"To Blender\" button, then import from the exports")
            return {"CANCELLED"}
        out = root / "build" / "exports"
        run = subprocess.run([cargo, "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "map", mid, "--out", str(out)],
                             cwd=str(root), capture_output=True, text=True)
        path = out / f"{mid}.glb"
        if run.returncode != 0 or not path.is_file():
            self.report({"ERROR"}, (run.stderr or run.stdout or "the export failed").strip()[-300:])
            return {"CANCELLED"}
        objects = import_glb(path)
        self.report({"INFO"}, f"{mid}: {len(objects)} objects")
        return {"FINISHED"}


class BF_OT_export_props(bpy.types.Operator):
    """Have the project export every prop in one file (it needs Rust's cargo), then import it: a row per category, each prop with its cells and its light"""
    bl_idname = "battle_frontier.export_props"
    bl_label = "Export and import every prop"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        root = project_root()
        if not root:
            self.report({"ERROR"}, "No Battle Frontier project found")
            return {"CANCELLED"}
        cargo = shutil.which("cargo")
        if not cargo:
            self.report({"ERROR"}, "Rust's cargo isn't installed here: use the asset browser's \"All props to Blender\" button, then import props.glb from the exports")
            return {"CANCELLED"}
        out = root / "build" / "exports"
        run = subprocess.run([cargo, "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "props", "--out", str(out)],
                             cwd=str(root), capture_output=True, text=True)
        path = out / "props.glb"
        if run.returncode != 0 or not path.is_file():
            self.report({"ERROR"}, (run.stderr or run.stdout or "the export failed").strip()[-300:])
            return {"CANCELLED"}
        objects = import_glb(path)
        props = sum(1 for o in objects if o.get("bf_prop"))
        self.report({"INFO"}, f"{props} props, by category; select one and \"Add to the project\" sends it back")
        return {"FINISHED"}


class BF_OT_show_bounds(bpy.types.Operator):
    """Show the cells the selection would cover in the game, and its height"""
    bl_idname = "battle_frontier.show_bounds"
    bl_label = "Show its cells"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        meshes = [o for o in context.selected_objects if o.type == "MESH" and not is_bounds(o)]
        if not meshes:
            self.report({"ERROR"}, "Select the objects that make the asset")
            return {"CANCELLED"}
        corners = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
        lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
        hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
        size = hi - lo
        footprint = [max(1, math.ceil(size.x - 0.05)), max(1, math.ceil(size.y - 0.05))]
        name = (context.active_object or meshes[0]).name
        make_bounds(f"{name} bounds", footprint, size.z, context.collection, centre=((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z))
        self.report({"INFO"}, f"{footprint[0]} x {footprint[1]} cells, {size.z:.2f} high")
        return {"FINISHED"}


ASSET_TYPES = {"MESH", "EMPTY", "CURVE", "ARMATURE", "LIGHT", "SPEAKER"}


def write_prop(context, root, space, aid, objects, facts, recentre=True, apply_modifiers=True):
    """`objects` as a prop of the project: its .glb under the space's assets/models/, its
    record under data/props/ (keeping what the record had: its folders, its stand-in...),
    measured from them; `facts` are the record's fields to set (name, category, blocks,
    unstackable, sill). Returns (the model's path, the record's path, the record, notes)."""
    root = Path(root)
    meshes = [o for o in objects if o.type == "MESH"]
    if not meshes:
        raise ValueError("there's no mesh to make it from")
    lights = [o for o in objects if o.type == "LIGHT" and o.data.type in {"SPOT", "POINT"}]
    speakers = [o for o in objects if o.type == "SPEAKER"]
    notes = []
    if any(o.type == "LIGHT" and o.data.type not in {"SPOT", "POINT"} for o in objects):
        notes.append("only a spot or a point light can be a prop's")
    if len(lights) > 1:
        notes.append(f"only the first of its {len(lights)} lights is kept")
    model_rel = f"models/{aid}.glb"
    glb = root / space / "assets" / model_rel
    record_path = root / space / "data" / "props" / f"{aid}.json"
    # its size, in the world, at rest: the model is written at rest (a rig's animation poses
    # it), so the pose it's in now doesn't count
    posed = [m for o in meshes for m in o.modifiers if m.type == "ARMATURE" and m.show_viewport]
    for m in posed:
        m.show_viewport = False
    context.view_layer.update()
    corners = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    for m in posed:
        m.show_viewport = True
    context.view_layer.update()
    lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
    hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
    offset = Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z)) if recentre else Vector((0, 0, 0))
    tops = [o for o in objects if o.parent not in objects]
    for o in tops:
        o.location -= offset
    # only the asset goes out: its bounds and anything else stay behind
    selected = list(context.selected_objects)
    active = context.view_layer.objects.active
    for o in context.view_layer.objects:
        o.select_set(o in objects)
    context.view_layer.update()
    try:
        glb.parent.mkdir(parents=True, exist_ok=True)
        # materials, textures, vertex colours, rigs, animations and shape keys as Blender
        # writes them; the lights in watts (1000 W = 1); never a camera
        bpy.ops.export_scene.gltf(filepath=str(glb), export_format="GLB", use_selection=True, export_apply=apply_modifiers, export_yup=True,
                                  **gltf_options(bpy.ops.export_scene.gltf, export_cameras=False, export_lights=True, export_import_convert_lighting_mode="RAW"))
    finally:
        for o in tops:
            o.location += offset
        for o in context.view_layer.objects:
            o.select_set(o in selected)
        context.view_layer.objects.active = active
        context.view_layer.update()
    size = hi - lo
    record = {}
    if record_path.is_file():
        try:
            record = json.loads(record_path.read_text(encoding="utf-8"))
        except ValueError:
            record = {}
    record.update({
        "id": aid,
        "name": facts.get("name") or record.get("name") or meshes[0].name,
        "category": facts.get("category") or record.get("category") or "decoration",
        # cells it covers along the engine's x and z (Blender's x and y)
        "footprint": [max(1, math.ceil(size.x - 0.05)), max(1, math.ceil(size.y - 0.05))],
        "height": round(float(size.z), 2),
        "sill": round(float(facts.get("sill", record.get("sill", 0.0))), 2),
        "blocks": bool(facts.get("blocks", record.get("blocks", True))),
        "unstackable": bool(facts.get("unstackable", record.get("unstackable", False))),
        "model": model_rel,
    })
    # its light and its sound, from the prop's origin (a record's own stay when there's none)
    if lights:
        record["light"] = light_record(lights[0], offset, record.get("light"))
    for s in speakers:
        sound = sound_record(s, offset, root, aid, space)
        if sound:
            record["sound"] = sound
            break
    record.setdefault("parts", [])
    record_path.parent.mkdir(parents=True, exist_ok=True)
    record_path.write_text(to_text(ordered_prop(record)), encoding="utf-8", newline="\n")
    tag(meshes, record)
    make_bounds(f"{aid} bounds", record["footprint"], record["height"], context.collection,
                centre=((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z) if not recentre else (offset.x, offset.y, offset.z))
    extras = ", with its light" if lights else ""
    extras += ", with its sound" if record.get("sound") and speakers else ""
    if extras:
        notes.insert(0, extras.lstrip(", "))
    return glb, record_path, record, notes


# ---------------------------------------------------------------- an edit session

EDIT_KEY = "bf_editing"
_edit = {"changed": 0.0, "saved_at": 0.0, "starting": False, "last": "", "timer": False, "print": None}


def _fingerprint(scene):
    """What the prop is made of, roughly: a change to it changes this (so an untouched prop
    is never written back, and a stand-in never becomes a model by itself)."""
    parts = []
    for o in sorted((o for o in scene.objects if o.type in ASSET_TYPES and not is_bounds(o)), key=lambda o: o.name):
        parts.append((o.name, o.type, tuple(round(v, 4) for row in o.matrix_world for v in row)))
        if o.type == "MESH":
            me = o.data
            co = [0.0] * (len(me.vertices) * 3)
            me.vertices.foreach_get("co", co)
            parts.append((len(me.vertices), len(me.polygons), round(sum(co), 4), round(sum(abs(c) for c in co), 4)))
            parts.append(tuple((md.type, md.show_viewport) for md in o.modifiers))
            for slot in o.material_slots:
                mat = slot.material
                if mat is None:
                    continue
                parts.append(mat.name)
                if mat.use_nodes:
                    for n in mat.node_tree.nodes:
                        if n.type == "BSDF_PRINCIPLED":
                            parts.append(tuple(round(v, 4) for v in n.inputs["Base Color"].default_value))
                        elif n.type == "TEX_IMAGE" and n.image:
                            parts.append(n.image.name)
        elif o.type == "LIGHT":
            d = o.data
            parts.append((d.type, round(d.energy, 3), tuple(round(c, 4) for c in d.color), round(getattr(d, "spot_size", 0.0), 4), round(d.cutoff_distance, 3)))
    return hash(repr(parts))


def save_edited(context, force=False):
    """The prop being edited (opened by the map editor's "Edit in Blender"), written back
    to the project where it came from: its space (a placeholder stays in temporary/), its
    record's facts and folders kept, its model and size from the scene. Returns what was
    written."""
    scene = context.scene
    pid = scene.get(EDIT_KEY)
    root = project_root()
    if not pid or not root:
        return ""
    # untouched since the last save (or since it was opened): nothing to write, unless asked
    if not force and _fingerprint(scene) == _edit["print"]:
        _edit["changed"] = 0.0
        return ""
    record_path, space = find_prop(root, pid)
    record = json.loads(record_path.read_text(encoding="utf-8")) if record_path else {}
    space = space or scene.get("bf_space", "content")
    objects = [o for o in scene.objects if o.type in ASSET_TYPES and not is_bounds(o)]
    facts = {k: record[k] for k in ("name", "category", "blocks", "unstackable", "sill") if k in record}
    try:
        glb, path, record, notes = write_prop(context, root, space, pid, objects, facts, recentre=False)
    except ValueError as e:
        _edit["last"] = f"Not saved: {e}"
        return _edit["last"]
    finally:
        _edit["saved_at"] = time.time()
        _edit["changed"] = 0.0
    _edit["print"] = _fingerprint(scene)
    _edit["last"] = f"Saved {Path(glb).relative_to(root)} and {Path(path).relative_to(root)}" + (f" ({'; '.join(notes)})" if notes else "")
    print(_edit["last"], flush=True)
    return _edit["last"]


@bpy.app.handlers.persistent
def _on_save(*_args):
    if not _edit["starting"] and bpy.context.scene.get(EDIT_KEY):
        save_edited(bpy.context)


@bpy.app.handlers.persistent
def _on_change(scene, depsgraph=None):
    """A change to the prop (not its bounds, not our own export): saved after a pause."""
    if not scene.get(EDIT_KEY) or time.time() - _edit["saved_at"] < 0.5 or depsgraph is None:
        return
    for u in depsgraph.updates:
        thing = u.id
        name = getattr(thing, "name", "")
        if name.endswith(" bounds") or (isinstance(thing, bpy.types.Object) and is_bounds(thing)):
            continue
        if u.is_updated_geometry or u.is_updated_transform or u.is_updated_shading:
            _edit["changed"] = time.time()
            return


def _autosave():
    """Two seconds after the last change, back in object mode: the prop goes to the project."""
    try:
        context = bpy.context
        if _edit["changed"] and time.time() - _edit["changed"] > 2.0 and context.mode == "OBJECT" and context.scene.get(EDIT_KEY):
            save_edited(context)
    except Exception as e:  # noqa: BLE001 - a timer must keep running
        print(f"Battle Frontier: not saved: {e}", flush=True)
    return 1.0


def start_editing(context, pid):
    """An edit session on a prop already imported: the scene remembers which, the .blend
    lives in build/blender/ (Ctrl+S saves without asking where), and every save, or a pause
    after a change, writes the prop back to the project."""
    root = project_root()
    scene = context.scene
    scene[EDIT_KEY] = pid
    _, space = find_prop(root, pid)
    scene["bf_space"] = space or "content"
    blend = Path(root) / "build" / "blender" / f"{pid}.blend"
    blend.parent.mkdir(parents=True, exist_ok=True)
    _edit["starting"] = True
    try:
        bpy.ops.wm.save_as_mainfile(filepath=str(blend), check_existing=False)
    finally:
        _edit["starting"] = False
    if _on_save not in bpy.app.handlers.save_post:
        bpy.app.handlers.save_post.append(_on_save)
    if _on_change not in bpy.app.handlers.depsgraph_update_post:
        bpy.app.handlers.depsgraph_update_post.append(_on_change)
    if not bpy.app.background and not _edit["timer"]:
        bpy.app.timers.register(_autosave, first_interval=1.0, persistent=True)
        _edit["timer"] = True
    _edit["changed"] = 0.0
    _edit["saved_at"] = time.time()
    _edit["print"] = _fingerprint(scene)
    _edit["last"] = f"Editing {pid}: saved to the project as you go"


class BF_OT_save_edited(bpy.types.Operator):
    """Write the prop being edited back to the project now (it's saved by itself too)"""
    bl_idname = "battle_frontier.save_edited"
    bl_label = "Save it to the project now"
    bl_options = {"REGISTER"}

    def execute(self, context):
        message = save_edited(context, force=True)
        self.report({"INFO"} if message.startswith("Saved") else {"ERROR"}, message or "Nothing being edited")
        return {"FINISHED"}


class BF_OT_add_to_project(bpy.types.Operator):
    """Add the selected objects to the project as a prop: a .glb model and its record"""
    bl_idname = "battle_frontier.add_to_project"
    bl_label = "Add to the project"
    bl_options = {"REGISTER"}

    asset_id: bpy.props.StringProperty(name="Id", description="lowercase letters, digits and _")
    asset_name: bpy.props.StringProperty(name="Name")
    category: bpy.props.EnumProperty(name="Category", items=CATEGORIES)
    blocks: bpy.props.BoolProperty(name="Blocks the way", default=True)
    unstackable: bpy.props.BoolProperty(name="Unstackable", default=False,
                                        description="Nothing can go on it or overlap its cells (stairs, shelves, beds...)")
    sill: bpy.props.FloatProperty(name="Sill (windows)", default=0.0, min=0.0, max=4.0)
    recentre: bpy.props.BoolProperty(name="Stand it on the origin", default=True,
                                     description="Move it so its footprint's middle is at the origin and it stands on Z = 0")
    apply_modifiers: bpy.props.BoolProperty(name="Apply modifiers", default=True,
                                            description="Bake modifiers (not armatures) into the model; off keeps shape keys, which applying drops")
    replace: bpy.props.BoolProperty(name="Replace if it exists", default=False)

    def invoke(self, context, event):
        obj = context.active_object
        if obj is not None:
            self.asset_id = obj.get("bf_prop") or slug(obj.name)
            self.asset_name = obj.get("bf_name") or obj.name
            if obj.get("bf_category") in {c[0] for c in CATEGORIES}:
                self.category = obj["bf_category"]
            self.blocks = bool(obj.get("bf_blocks", True))
            self.unstackable = bool(obj.get("bf_unstackable", False))
            self.sill = float(obj.get("bf_sill", 0.0))
            self.replace = "bf_prop" in obj
        return context.window_manager.invoke_props_dialog(self)

    def execute(self, context):
        root = project_root()
        if not root:
            self.report({"ERROR"}, "No Battle Frontier project found")
            return {"CANCELLED"}
        # the meshes, their rig, the lights and speakers on them; never a camera (the game
        # has one of its own) nor the bounds
        objects = [o for o in context.selected_objects if o.type in ASSET_TYPES and not is_bounds(o)]
        meshes = [o for o in objects if o.type == "MESH"]
        if not meshes:
            self.report({"ERROR"}, "Select the objects that make the asset")
            return {"CANCELLED"}
        aid = slug(self.asset_id or meshes[0].name)
        glb = Path(root) / "content" / "assets" / "models" / f"{aid}.glb"
        record_path = props_dir(root) / f"{aid}.json"
        if (glb.exists() or record_path.exists()) and not self.replace:
            self.report({"ERROR"}, f"{aid} is already in the project: tick 'Replace if it exists'")
            return {"CANCELLED"}
        facts = {"name": self.asset_name, "category": self.category, "blocks": self.blocks, "unstackable": self.unstackable, "sill": self.sill}
        try:
            glb, record_path, record, notes = write_prop(context, root, "content", aid, objects, facts, self.recentre, self.apply_modifiers)
        except ValueError as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        if any(o.type == "CAMERA" for o in context.selected_objects):
            notes.append("the camera stays behind (the game has one)")
        self.report({"INFO"}, f"{aid}: {Path(glb).relative_to(root)} and {Path(record_path).relative_to(root)}; check with `content check`" + ("; " + "; ".join(notes) if notes else ""))
        return {"FINISHED"}


# ---------------------------------------------------------------- the panel and preferences

class BF_Preferences(bpy.types.AddonPreferences):
    bl_idname = __name__
    project_root: bpy.props.StringProperty(name="Project folder", subtype="DIR_PATH",
                                           description="The Battle Frontier folder (found by itself when empty)")

    def draw(self, context):
        self.layout.prop(self, "project_root")
        self.layout.operator(BF_OT_find_project.bl_idname)


class BF_PT_panel(bpy.types.Panel):
    bl_label = "Battle Frontier"
    bl_idname = "BF_PT_panel"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Battle Frontier"

    def draw(self, context):
        layout = self.layout
        wm = context.window_manager
        root = project_root(search=False)
        box = layout.box()
        if root:
            box.label(text=f"Project: {root.name}", icon="CHECKMARK")
            box.label(text=str(root))
        else:
            box.label(text="Project not found yet", icon="ERROR")
            box.operator(BF_OT_find_project.bl_idname, icon="VIEWZOOM")
            return
        editing = context.scene.get(EDIT_KEY)
        if editing:
            box = layout.box()
            box.label(text=f"Editing {editing}", icon="GREASEPENCIL")
            box.label(text="Saved to the project as you go, and with Ctrl+S")
            box.operator(BF_OT_save_edited.bl_idname, icon="FILE_TICK")
            if _edit["last"]:
                box.label(text=_edit["last"][:70])
        col = layout.column(align=True)
        col.label(text="Props", icon="MESH_MONKEY")
        col.prop(wm, "bf_prop", text="")
        op = col.operator(BF_OT_import_prop.bl_idname, icon="IMPORT")
        op.prop_id = wm.bf_prop
        col.operator(BF_OT_export_props.bl_idname, icon="FILE_REFRESH")
        layout.separator()
        col = layout.column(align=True)
        col.label(text="Your model", icon="EXPORT")
        col.operator(BF_OT_show_bounds.bl_idname, icon="MESH_GRID")
        col.operator(BF_OT_add_to_project.bl_idname, icon="ADD")
        if context.scene.unit_settings.scale_length != 1.0:
            col.label(text="Unit scale isn't 1: sizes won't match", icon="ERROR")
        col.label(text="Stand it at the origin, front to -Y, 1 unit = 1 cell")
        col.label(text="Select its rig, lights and speakers too; no camera")
        layout.separator()
        col = layout.column(align=True)
        col.label(text="Maps and buildings", icon="WORLD")
        col.prop(wm, "bf_map", text="")
        col.operator(BF_OT_export_map.bl_idname, icon="FILE_REFRESH").map_id = wm.bf_map
        col.prop(wm, "bf_export", text="")
        col.operator(BF_OT_import_export.bl_idname, icon="IMPORT").file = wm.bf_export


CLASSES = (BF_Preferences, BF_OT_find_project, BF_OT_import_prop, BF_OT_import_export, BF_OT_export_map, BF_OT_export_props, BF_OT_show_bounds, BF_OT_save_edited, BF_OT_add_to_project, BF_PT_panel)


def _auto_find():
    if project_root(search=False) is None:
        found = find_project(budget=4.0)
        if found:
            remember(found)
    return None


def register():
    for c in CLASSES:
        bpy.utils.register_class(c)
    bpy.types.WindowManager.bf_prop = bpy.props.EnumProperty(name="Prop", items=prop_items)
    bpy.types.WindowManager.bf_export = bpy.props.EnumProperty(name="Export", items=export_items)
    bpy.types.WindowManager.bf_map = bpy.props.EnumProperty(name="Map", items=map_items)
    if not bpy.app.background:
        bpy.app.timers.register(_auto_find, first_interval=1.0)


def unregister():
    for name in ("bf_prop", "bf_export", "bf_map"):
        if hasattr(bpy.types.WindowManager, name):
            delattr(bpy.types.WindowManager, name)
    for c in reversed(CLASSES):
        bpy.utils.unregister_class(c)


if __name__ == "__main__":
    register()
