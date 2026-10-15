# The add-on, tested in a background Blender against a scratch project (nothing in the real
# content/ is written; build/exports/ gets the exported map):
#   blender --background --factory-startup --python-exit-code 1 --python tools/blender/test_addon.py -- PROJECT SCRATCH
# It finds the project, imports a stand-in prop, makes a textured asset and adds it to the
# scratch project, imports it back from its .glb, refuses to overwrite it, and has the real
# project export a map and imports that. Exits 1 when something fails.

import json
import math
import os
import shutil
import sys
from pathlib import Path

import bmesh
import bpy

argv = sys.argv[sys.argv.index("--") + 1:]
project, scratch = Path(argv[0]).resolve(), Path(argv[1]).resolve()
sys.path.insert(0, str(project / "tools" / "blender"))
import battle_frontier as bf  # noqa: E402

bf.register()
failed = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""), flush=True)
    if not ok:
        failed.append(what)


def bounds(objects):
    from mathutils import Vector
    cs = [o.matrix_world @ Vector(c) for o in objects if o.type == "MESH" for c in o.bound_box]
    lo = Vector((min(c.x for c in cs), min(c.y for c in cs), min(c.z for c in cs)))
    hi = Vector((max(c.x for c in cs), max(c.y for c in cs), max(c.z for c in cs)))
    return lo, hi


def clear():
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o, do_unlink=True)


# a scratch project: the markers and the props, nothing else
if scratch.exists():
    shutil.rmtree(scratch)
(scratch / "crates" / "content").mkdir(parents=True)
(scratch / "Trunk.toml").write_text("# a scratch copy for the add-on's test\n")
shutil.copytree(project / "content" / "data" / "props", scratch / "content" / "data" / "props")
clear()

# 1. finding it
found = bf.find_project(roots=[scratch.parent], depth=2, quick=False)
check("finds the project by looking around", found == str(scratch), str(found))
os.environ["BATTLE_FRONTIER_ROOT"] = str(scratch)
check("knows it from BATTLE_FRONTIER_ROOT", str(bf.project_root()) == str(scratch))

# 2. a stand-in prop
r = bpy.ops.battle_frontier.import_prop(prop_id="pine_tree")
pine = bpy.data.objects.get("pine_tree")
lo, hi = bounds([pine]) if pine else (None, None)
check("imports a stand-in from its shapes", r == {"FINISHED"} and pine is not None)
if pine:
    check("as tall as the prop", abs(hi.z - 3.3) < 0.05 and abs(lo.z) < 0.01, f"{lo.z:.2f} to {hi.z:.2f}")
    check("a material per colour", len(pine.data.materials) == 4, str([m.name for m in pine.data.materials]))
clear()

# 3. an asset of one's own: a stone well with a roof and a painted sign, made off the origin
def solid(name, build, colour=None, image=None, location=(0, 0, 0)):
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    build(bm)
    if image is not None:
        uv = bm.loops.layers.uv.new()
        for f in bm.faces:
            for loop in f.loops:
                co = loop.vert.co
                loop[uv].uv = (co.x + 0.5, co.z + 0.5)
    bm.to_mesh(mesh)
    bm.free()
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    if image is not None:
        tex = mat.node_tree.nodes.new("ShaderNodeTexImage")
        tex.image = image
        mat.node_tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    else:
        bsdf.inputs["Base Color"].default_value = colour
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    obj.location = location
    bpy.context.collection.objects.link(obj)
    return obj


sign = bpy.data.images.new("well_sign", 16, 16)
px = []
for y in range(16):
    for x in range(16):
        px += [0.55, 0.32, 0.15, 1.0] if (x // 4 + y // 4) % 2 else [0.95, 0.85, 0.55, 1.0]
sign.pixels = px
base = (3.0, 2.0, 0.0)
ring = solid("well_ring", lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=16, radius1=0.8, radius2=0.8, depth=0.7), (0.45, 0.43, 0.4, 1), location=(base[0], base[1], 0.35))
roof = solid("well_roof", lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=4, radius1=1.0, radius2=0.0, depth=0.6), (0.6, 0.15, 0.1, 1), location=(base[0], base[1], 1.9))
post_a = solid("well_post_a", lambda bm: bmesh.ops.create_cube(bm, size=1.0), (0.4, 0.25, 0.12, 1), location=(base[0] - 0.7, base[1], 1.0))
post_a.scale = (0.12, 0.12, 1.4)
post_b = solid("well_post_b", lambda bm: bmesh.ops.create_cube(bm, size=1.0), (0.4, 0.25, 0.12, 1), location=(base[0] + 0.7, base[1], 1.0))
post_b.scale = (0.12, 0.12, 1.4)
plate = solid("well_sign", lambda bm: bmesh.ops.create_cube(bm, size=1.0), image=sign, location=(base[0], base[1] - 0.85, 0.5))
plate.scale = (0.5, 0.04, 0.3)
parts = [ring, roof, post_a, post_b, plate]
for o in bpy.context.view_layer.objects:
    o.select_set(o in parts)
bpy.context.view_layer.objects.active = ring
before = [tuple(o.location) for o in parts]
r = bpy.ops.battle_frontier.add_to_project(asset_id="stone_well", asset_name="Stone well", category="decoration", blocks=True)
check("adds it to the project", r == {"FINISHED"})
check("leaves it where it was in Blender", [tuple(o.location) for o in parts] == before)
glb = scratch / "content" / "assets" / "models" / "stone_well.glb"
record_path = scratch / "content" / "data" / "props" / "stone_well.json"
check("writes the model", glb.is_file() and glb.read_bytes()[:4] == b"glTF", str(glb))
record = json.loads(record_path.read_text(encoding="utf-8")) if record_path.is_file() else {}
check("writes its record", record.get("model") == "models/stone_well.glb" and record.get("category") == "decoration" and record.get("blocks") is True, json.dumps(record))
check("measures its footprint and height", record.get("footprint") == [2, 2] and abs(record.get("height", 0) - 2.2) < 0.05, f"{record.get('footprint')} {record.get('height')}")
check("lays the record out as the project does", record_path.read_text(encoding="utf-8") == bf.to_text(bf.ordered_prop(record)))

# 4. back from its .glb, standing on the origin
clear()
r = bpy.ops.battle_frontier.import_prop(prop_id="stone_well")
objs = list(bpy.context.selected_objects)
lo, hi = bounds(objs) if objs else (None, None)
check("imports it back from its model", r == {"FINISHED"} and len(objs) >= 1, f"{len(objs)} objects")
if objs:
    check("standing on the origin", abs((lo.x + hi.x) / 2) < 0.02 and abs((lo.y + hi.y) / 2) < 0.02 and abs(lo.z) < 0.02, f"{lo[:]} {hi[:]}")
    textured = any(n.type == "TEX_IMAGE" and n.image for o in objs if o.type == "MESH" for m in o.data.materials if m and m.use_nodes for n in m.node_tree.nodes)
    check("with its painted sign", textured)

# 5. no overwriting by accident
for o in bpy.context.view_layer.objects:
    o.select_set(True)
try:
    r = bpy.ops.battle_frontier.add_to_project(asset_id="stone_well", category="decoration")
except RuntimeError:  # Blender raises when an operator reports an error
    r = {"CANCELLED"}
check("won't replace it unasked", r == {"CANCELLED"})

# 6. a whole map, exported by the project itself
os.environ["BATTLE_FRONTIER_ROOT"] = str(project)
clear()
r = bpy.ops.battle_frontier.export_map(map_id="test_meadow")
count = len(bpy.data.objects)
check("exports and imports a map", r == {"FINISHED"} and count > 100, f"{count} objects")
names = {o.name for o in bpy.data.objects}
check("ground, props, trees and buildings apart", {"Ground", "Props", "Trees", "Buildings"} <= names)
clear()
r = bpy.ops.battle_frontier.import_export(file="test_meadow.glb")
check("imports an export", r == {"FINISHED"} and len(bpy.data.objects) > 100)

print("all good" if not failed else f"{len(failed)} failed", flush=True)
sys.exit(1 if failed else 0)
