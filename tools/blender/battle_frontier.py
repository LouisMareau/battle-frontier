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
PROP_KEYS = ["id", "name", "category", "footprint", "height", "sill", "pattern", "blocks", "parts", "model"]
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
        if k == "blocks" and not v:
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


def build_stand_in(record, collection):
    """A prop's stand-in, its shapes in their colours, as one object."""
    bm = bmesh.new()
    mesh = bpy.data.meshes.new(record["id"])
    for part in record.get("parts", []):
        before = set(bm.verts)
        shape = part.get("shape", "box")
        if shape == "cylinder":
            bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=16, radius1=0.5, radius2=0.5, depth=1.0)
        elif shape == "cone":
            bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=16, radius1=0.5, radius2=0.0, depth=1.0)
        elif shape == "sphere":
            bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=8, radius=0.5)
        elif shape == "rock":
            bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.5)
        else:
            bmesh.ops.create_cube(bm, size=1.0)
        new = [v for v in bm.verts if v not in before]
        sx, sy, sz = part.get("size", [1, 1, 1])
        ax, ay, az = part.get("at", [0, 0, 0])
        # the engine's (x, up, south) is Blender's (x, -y, z)
        m = (Matrix.Translation(Vector((ax, -az, ay + sy / 2.0)))
             @ Matrix.Rotation(-math.radians(part.get("turn", 0.0)), 4, "Z")
             @ Matrix.Diagonal(Vector((sx, sz, sy, 1.0))))
        bmesh.ops.transform(bm, matrix=m, verts=new)
        mat = colour_material(part.get("color", "#888888"))
        if mat.name not in mesh.materials:
            mesh.materials.append(mat)
        index = list(mesh.materials).index(mat)
        for f in {f for v in new for f in v.link_faces}:
            f.material_index = index
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(record["id"], mesh)
    collection.objects.link(obj)
    return [obj]


def tag(objects, record):
    for o in objects:
        o["bf_prop"] = record["id"]
        o["bf_name"] = record.get("name") or record["id"]
        o["bf_category"] = record.get("category", "decoration")
        o["bf_blocks"] = bool(record.get("blocks", False))
        if record.get("sill"):
            o["bf_sill"] = record["sill"]


def import_glb(path):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
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
        path = props_dir(root) / f"{pid}.json"
        if not path.is_file():
            self.report({"ERROR"}, f"No prop {pid}")
            return {"CANCELLED"}
        record = json.loads(path.read_text(encoding="utf-8"))
        model = record.get("model")
        if model and (root / "content" / "assets" / model).is_file():
            objects = import_glb(root / "content" / "assets" / model)
        else:
            objects = build_stand_in(record, context.collection)
        tag(objects, record)
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


class BF_OT_add_to_project(bpy.types.Operator):
    """Add the selected objects to the project as a prop: a .glb model and its record"""
    bl_idname = "battle_frontier.add_to_project"
    bl_label = "Add to the project"
    bl_options = {"REGISTER"}

    asset_id: bpy.props.StringProperty(name="Id", description="lowercase letters, digits and _")
    asset_name: bpy.props.StringProperty(name="Name")
    category: bpy.props.EnumProperty(name="Category", items=CATEGORIES)
    blocks: bpy.props.BoolProperty(name="Blocks the way", default=True)
    sill: bpy.props.FloatProperty(name="Sill (windows)", default=0.0, min=0.0, max=4.0)
    recentre: bpy.props.BoolProperty(name="Stand it on the origin", default=True,
                                     description="Move it so its footprint's middle is at the origin and it stands on Z = 0")
    replace: bpy.props.BoolProperty(name="Replace if it exists", default=False)

    def invoke(self, context, event):
        obj = context.active_object
        if obj is not None:
            self.asset_id = obj.get("bf_prop") or slug(obj.name)
            self.asset_name = obj.get("bf_name") or obj.name
            if obj.get("bf_category") in {c[0] for c in CATEGORIES}:
                self.category = obj["bf_category"]
            self.blocks = bool(obj.get("bf_blocks", True))
            self.sill = float(obj.get("bf_sill", 0.0))
            self.replace = "bf_prop" in obj
        return context.window_manager.invoke_props_dialog(self)

    def execute(self, context):
        root = project_root()
        if not root:
            self.report({"ERROR"}, "No Battle Frontier project found")
            return {"CANCELLED"}
        objects = [o for o in context.selected_objects if o.type in {"MESH", "EMPTY", "CURVE"}]
        meshes = [o for o in objects if o.type == "MESH"]
        if not meshes:
            self.report({"ERROR"}, "Select the objects that make the asset")
            return {"CANCELLED"}
        aid = slug(self.asset_id or meshes[0].name)
        model_rel = f"models/{aid}.glb"
        glb = root / "content" / "assets" / model_rel
        record_path = props_dir(root) / f"{aid}.json"
        if (glb.exists() or record_path.exists()) and not self.replace:
            self.report({"ERROR"}, f"{aid} is already in the project: tick \"Replace if it exists\"")
            return {"CANCELLED"}
        # its size, in the world
        corners = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
        lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
        hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
        offset = Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z)) if self.recentre else Vector((0, 0, 0))
        tops = [o for o in objects if o.parent not in objects]
        for o in tops:
            o.location -= offset
        context.view_layer.update()
        try:
            glb.parent.mkdir(parents=True, exist_ok=True)
            bpy.ops.export_scene.gltf(filepath=str(glb), export_format="GLB", use_selection=True, export_apply=True, export_yup=True)
        finally:
            for o in tops:
                o.location += offset
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
            "name": self.asset_name or record.get("name") or meshes[0].name,
            "category": self.category,
            # cells it covers along the engine's x and z (Blender's x and y)
            "footprint": [max(1, math.ceil(size.x - 0.05)), max(1, math.ceil(size.y - 0.05))],
            "height": round(float(size.z), 2),
            "sill": round(float(self.sill), 2),
            "blocks": bool(self.blocks),
            "model": model_rel,
        })
        record.setdefault("parts", [])
        record_path.parent.mkdir(parents=True, exist_ok=True)
        record_path.write_text(to_text(ordered_prop(record)), encoding="utf-8", newline="\n")
        tag(meshes, record)
        self.report({"INFO"}, f"{aid}: {glb.relative_to(root)} and {record_path.relative_to(root)}; check with `content check`")
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
        col = layout.column(align=True)
        col.label(text="Props", icon="MESH_MONKEY")
        col.prop(wm, "bf_prop", text="")
        op = col.operator(BF_OT_import_prop.bl_idname, icon="IMPORT")
        op.prop_id = wm.bf_prop
        layout.separator()
        col = layout.column(align=True)
        col.label(text="Your model", icon="EXPORT")
        col.operator(BF_OT_add_to_project.bl_idname, icon="ADD")
        col.label(text="Stand it at the origin, front to -Y, 1 unit = 1 cell")
        layout.separator()
        col = layout.column(align=True)
        col.label(text="Maps and buildings", icon="WORLD")
        col.prop(wm, "bf_map", text="")
        col.operator(BF_OT_export_map.bl_idname, icon="FILE_REFRESH").map_id = wm.bf_map
        col.prop(wm, "bf_export", text="")
        col.operator(BF_OT_import_export.bl_idname, icon="IMPORT").file = wm.bf_export


CLASSES = (BF_Preferences, BF_OT_find_project, BF_OT_import_prop, BF_OT_import_export, BF_OT_export_map, BF_OT_add_to_project, BF_PT_panel)


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
