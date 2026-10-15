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
import struct
import subprocess
import sys
import wave
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
box = bpy.data.objects.get("pine_tree bounds")
check("with its bounds: its cells and its height", box is not None and box.get("bf_bounds") and abs(box.dimensions.z - 3.4) < 0.01 and abs(box.dimensions.x - 1.0) < 0.01,
      f"{box.dimensions[:] if box else None}")
clear()

# 2b. exactly the engine's shapes: the stand-ins against the engine's own export
engine = scratch / "engine"
run = subprocess.run(["cargo", "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "props", "--out", str(engine)], cwd=str(project), capture_output=True, text=True)
check("the engine exports every prop", run.returncode == 0, run.stderr[-300:])


def points(objects):
    out = []
    for o in objects:
        if o.type != "MESH" or bf.is_bounds(o):
            continue
        for v in o.data.vertices:
            out.append(o.matrix_world @ v.co)
    return out


for pid in ("pine_tree", "round_tree", "boulder", "bench", "lamp_post"):
    clear()
    theirs = points(bf.import_glb(engine / f"{pid}.glb"))
    clear()
    bpy.ops.battle_frontier.import_prop(prop_id=pid)
    ours = points(list(bpy.data.objects))
    far = max((min((a - b).length for b in theirs) for a in ours), default=1.0)
    back = max((min((a - b).length for b in ours) for a in theirs), default=1.0)
    check(f"{pid}: the same shape as in the game", far < 1e-4 and back < 1e-4, f"{len(ours)} vs {len(theirs)} points, off by {max(far, back):.6f}")
# a lamp's light comes along, both ways, in Blender's watts (1000 W = an intensity of 1)
lamp_record = json.loads((scratch / "content" / "data" / "props" / "lamp_post.json").read_text(encoding="utf-8"))
wanted_watts = lamp_record["light"]["intensity"] * 1000.0
lamp_lights = [o for o in bpy.data.objects if o.type == "LIGHT"]
check("a lamp's light comes with its stand-in", len(lamp_lights) == 1 and abs(lamp_lights[0].data.energy - wanted_watts) < 1 and lamp_lights[0].data.type == "SPOT",
      f"{[(o.data.type, o.data.energy) for o in lamp_lights]}")
if lamp_lights:
    at = lamp_lights[0].matrix_world.translation
    check("  where the record puts it", max(abs(a - b) for a, b in zip(bf.to_engine(at), lamp_record["light"]["at"])) < 0.01, f"{tuple(round(v, 2) for v in bf.to_engine(at))} vs {lamp_record['light']['at']}")
clear()
engine_lights = [o for o in bf.import_glb(engine / "lamp_post.glb") if o.type == "LIGHT"]
check("and with the game's export of it", len(engine_lights) == 1 and abs(engine_lights[0].data.energy - wanted_watts) < 1, f"{[o.data.energy for o in engine_lights]}")
clear()

# 2c. every prop in one file, each knowing which it is
catalogue = bf.import_glb(engine / "props.glb")
ids = {o.get("bf_prop") for o in catalogue if o.get("bf_prop")}
known = {p.stem for p in (scratch / "content" / "data" / "props").glob("*.json")}
check("every prop in one file", known <= ids, f"{len(ids)} props, missing {sorted(known - ids)[:5]}")
check("  laid out apart", len({tuple(round(v, 1) for v in o.matrix_world.translation) for o in catalogue if o.get("bf_prop")}) == len(ids))
check("  with their lights and their cells", sum(1 for o in catalogue if o.type == "LIGHT") >= 1 and sum(1 for o in catalogue if bf.is_bounds(o)) == len(ids))
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
# a lamp over the well, aimed at its front and down (a light points down its -Z), a speaker
# with a hum, and a camera that must stay behind
lamp_data = bpy.data.lights.new("well_lamp", "SPOT")
lamp_data.energy = 1500.0
lamp_data.color = (1.0, 0.5, 0.25)
lamp_data.spot_size = math.radians(90.0)
lamp_data.use_custom_distance = True
lamp_data.cutoff_distance = 5.0
lamp = bpy.data.objects.new("well_lamp", lamp_data)
lamp.location = (base[0], base[1] - 0.5, 1.8)
lamp.rotation_euler = (math.radians(-45.0), 0.0, 0.0)
bpy.context.collection.objects.link(lamp)
hum = scratch / "hum.wav"
with wave.open(str(hum), "wb") as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(8000)
    w.writeframes(bytes(800))
speaker = bpy.data.objects.new("well_hum", bpy.data.speakers.new("well_hum"))
speaker.data.sound = bpy.data.sounds.load(str(hum))
speaker.data.volume = 0.5
speaker.data.distance_max = 7.0
speaker.location = (base[0], base[1], 0.6)
bpy.context.collection.objects.link(speaker)
camera = bpy.data.objects.new("well_cam", bpy.data.cameras.new("well_cam"))
camera.location = (base[0], base[1] - 6.0, 3.0)
bpy.context.collection.objects.link(camera)
parts = [ring, roof, post_a, post_b, plate]
with_it = parts + [lamp, speaker, camera]
for o in bpy.context.view_layer.objects:
    o.select_set(o in with_it)
bpy.context.view_layer.objects.active = ring
before = [tuple(o.location) for o in with_it]
r = bpy.ops.battle_frontier.add_to_project(asset_id="test_well", asset_name="Test well", category="decoration", blocks=True, unstackable=True)
check("adds it to the project", r == {"FINISHED"})
check("leaves it where it was in Blender", [tuple(o.location) for o in with_it] == before)
glb = scratch / "content" / "assets" / "models" / "test_well.glb"
record_path = scratch / "content" / "data" / "props" / "test_well.json"
check("writes the model", glb.is_file() and glb.read_bytes()[:4] == b"glTF", str(glb))
record = json.loads(record_path.read_text(encoding="utf-8")) if record_path.is_file() else {}
check("writes its record", record.get("model") == "models/test_well.glb" and record.get("category") == "decoration" and record.get("blocks") is True and record.get("unstackable") is True, json.dumps(record))


def glb_doc(path):
    data = path.read_bytes()
    length = struct.unpack_from("<I", data, 12)[0]
    return json.loads(data[20:20 + length])


doc = glb_doc(glb) if glb.is_file() else {}
names = [n.get("name", "") for n in doc.get("nodes", [])]
check("the model leaves its bounds behind", names and not any("bounds" in n for n in names), str(names))
check("and the camera: never one in a model", "cameras" not in doc and "well_cam" not in names and not any("camera" in n.keys() for n in doc.get("nodes", [])), str(names))
check("its light goes into the model", len(doc.get("extensions", {}).get("KHR_lights_punctual", {}).get("lights", [])) == 1, str(doc.get("extensions")))
well_lo, well_hi = bounds(parts)
origin = ((well_lo.x + well_hi.x) / 2, (well_lo.y + well_hi.y) / 2, well_lo.z)
light = record.get("light") or {}
wanted_at = [round(v, 2) for v in bf.to_engine((lamp.location[0] - origin[0], lamp.location[1] - origin[1], lamp.location[2] - origin[2]))]
check("and into the record: watts over 1000, colour, cone, range", light.get("kind") == "spot" and light.get("intensity") == 1.5 and light.get("color") == "#ffbc89" and light.get("cone") == 90.0 and light.get("range") == 5.0 and light.get("night") is True, json.dumps(light))
check("  where it sits, where it points", max(abs(a - b) for a, b in zip(light.get("at", [9, 9, 9]), wanted_at)) < 0.011 and light.get("aim") == [0.0, -0.71, 0.71], f"{light.get('at')} vs {wanted_at}, {light.get('aim')}")
sound = record.get("sound") or {}
check("its sound into the record, the file into the project", sound.get("file") == "sounds/test_well_hum.wav" and sound.get("volume") == 0.5 and sound.get("range") == 7.0 and (scratch / "content" / "assets" / "sounds" / "test_well_hum.wav").is_file(), json.dumps(sound))
check("shows the bounds in Blender", bpy.data.objects.get("test_well bounds") is not None)
check("measures its footprint and height", record.get("footprint") == [2, 2] and abs(record.get("height", 0) - 2.2) < 0.05, f"{record.get('footprint')} {record.get('height')}")
check("lays the record out as the project does", record_path.read_text(encoding="utf-8") == bf.to_text(bf.ordered_prop(record)))

# 4. back from its .glb, standing on the origin
clear()
r = bpy.ops.battle_frontier.import_prop(prop_id="test_well")
objs = list(bpy.context.selected_objects)
lo, hi = bounds(objs) if objs else (None, None)
check("imports it back from its model", r == {"FINISHED"} and len(objs) >= 1, f"{len(objs)} objects")
if objs:
    check("standing on the origin", abs((lo.x + hi.x) / 2) < 0.02 and abs((lo.y + hi.y) / 2) < 0.02 and abs(lo.z) < 0.02, f"{lo[:]} {hi[:]}")
    textured = any(n.type == "TEX_IMAGE" and n.image for o in objs if o.type == "MESH" for m in o.data.materials if m and m.use_nodes for n in m.node_tree.nodes)
    check("with its painted sign", textured)
    back_lights = [o for o in bpy.data.objects if o.type == "LIGHT"]
    check("and its lamp, as it was", len(back_lights) == 1 and abs(back_lights[0].data.energy - 1500.0) < 1 and abs(math.degrees(back_lights[0].data.spot_size) - 90.0) < 0.5,
          f"{[(o.data.energy, math.degrees(o.data.spot_size)) for o in back_lights]}")
    check("no camera came along", not any(o.type == "CAMERA" for o in bpy.data.objects))

# 5. no overwriting by accident
for o in bpy.context.view_layer.objects:
    o.select_set(True)
try:
    r = bpy.ops.battle_frontier.add_to_project(asset_id="test_well", category="decoration")
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
