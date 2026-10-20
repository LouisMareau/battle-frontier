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
import make_flag  # noqa: E402

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
    # (real size: its top cone's tip 13.3 up; a trunk and five cones, each its own colour)
    check("as tall as the prop", abs(hi.z - 13.3) < 0.05 and abs(lo.z) < 0.01, f"{lo.z:.2f} to {hi.z:.2f}")
    check("a material per colour", len(pine.data.materials) == 6, str([m.name for m in pine.data.materials]))
box = bpy.data.objects.get("pine_tree bounds")
check("with its bounds: its cells and its height", box is not None and box.get("bf_bounds") and abs(box.dimensions.z - 13.3) < 0.01 and abs(box.dimensions.x - 1.0) < 0.01,
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


for pid in ("pine_tree", "round_tree", "boulder", "bench", "mud_slide", "stone_stairs", "lamp_post"):
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


def glb_values(path, accessor):
    """An accessor's values in a .glb, as numbers (0 to 1 for normalized ones)."""
    data = path.read_bytes()
    length = struct.unpack_from("<I", data, 12)[0]
    doc, binary = json.loads(data[20:20 + length]), data[20 + length + 8:]
    a = doc["accessors"][accessor]
    view = doc["bufferViews"][a["bufferView"]]
    kind, size, top = {5126: ("f", 4, 1.0), 5123: ("H", 2, 65535.0), 5121: ("B", 1, 255.0)}[a["componentType"]]
    n = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
    stride, start = view.get("byteStride", size * n), view.get("byteOffset", 0) + a.get("byteOffset", 0)
    top = top if a.get("normalized") else 1.0
    return [tuple(c / top for c in struct.unpack_from("<" + kind * n, binary, start + i * stride)) for i in range(a["count"])]


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

# 3b. a rigged asset: a flag that waves, its rig and its animation through the pipeline
clear()
rigged = make_flag.build_flag(bpy.context.collection)
for o in bpy.context.view_layer.objects:
    o.select_set(o in rigged)
bpy.context.view_layer.objects.active = rigged[0]
r = bpy.ops.battle_frontier.add_to_project(asset_id="test_flag", asset_name="Test flag", category="decoration")
check("adds a rigged asset to the project", r == {"FINISHED"})
flag_glb = scratch / "content" / "assets" / "models" / "test_flag.glb"
flag_doc = glb_doc(flag_glb) if flag_glb.is_file() else {}
check("its rig and its animation go into the model", len(flag_doc.get("skins", [])) == 1 and [a.get("name") for a in flag_doc.get("animations", [])] == ["idle"], f"{len(flag_doc.get('skins', []))} skins, {[a.get('name') for a in flag_doc.get('animations', [])]}")
run = subprocess.run(["cargo", "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "check", str(flag_glb)], cwd=str(project), capture_output=True, text=True)
check("the engine reads the rig and the animation", "a rig of 2 joints" in run.stdout and "idle (2.0 s)" in run.stdout, (run.stdout.strip() or run.stderr.strip())[-200:])
clear()
r = bpy.ops.battle_frontier.import_prop(prop_id="test_flag")
check("comes back rigged, with its action", r == {"FINISHED"} and any(o.type == "ARMATURE" for o in bpy.data.objects) and len(bpy.data.actions) >= 1, f"{[o.type for o in bpy.data.objects]}, {len(bpy.data.actions)} actions")

# 3c. what lies along ramps: where it was made (its foot along Y -0.5, its top along Y 0.5, on
# the origin), one cell deep whatever pokes out past its cell (a pool at its foot), and how
def slide_shape(bm):
    v = [bm.verts.new(p) for p in [(-0.5, -0.5, 0), (0.5, -0.5, 0), (0.5, 0.5, 0), (-0.5, 0.5, 0), (-0.5, 0.5, 1), (0.5, 0.5, 1)]]
    for f in [(0, 1, 5, 4), (3, 4, 5, 2), (1, 2, 5), (0, 4, 3), (0, 3, 2, 1)]:
        bm.faces.new([v[i] for i in f])
    pool = [bm.verts.new(p) for p in [(-0.5, -0.9, 0.03), (0.5, -0.9, 0.03), (0.5, -0.5, 0.03), (-0.5, -0.5, 0.03)]]
    bm.faces.new(pool)


clear()
slide = solid("test_slide", slide_shape, (0.42, 0.29, 0.18, 1.0))
for o in bpy.context.view_layer.objects:
    o.select_set(o is slide)
bpy.context.view_layer.objects.active = slide
r = bpy.ops.battle_frontier.add_to_project(asset_id="test_slide", asset_name="Test slide", category="ramp", blocks=False)
slide_path = scratch / "content" / "data" / "props" / "test_slide.json"
slide_record = json.loads(slide_path.read_text(encoding="utf-8")) if slide_path.is_file() else {}
check("along ramps: one cell deep, how it lies", r == {"FINISHED"} and slide_record.get("footprint") == [1, 1] and slide_record.get("ramp") == {"along": "stretch", "rise": 1.0}, json.dumps(slide_record))
slide_doc = glb_doc(scratch / "content" / "assets" / "models" / "test_slide.glb") if r == {"FINISHED"} else {}
moved = [n.get("translation", [0, 0, 0]) for n in slide_doc.get("nodes", []) if "mesh" in n]
positions = {p["attributes"]["POSITION"] for m in slide_doc.get("meshes", []) for p in m.get("primitives", [])}
spans = [a for i, a in enumerate(slide_doc.get("accessors", [])) if i in positions and "min" in a]
zs = (min(a["min"][2] for a in spans), max(a["max"][2] for a in spans)) if spans else (0, 0)
check("  where it was made: its top along z -0.5, its pool out to 0.9 past its foot", moved and all(abs(c) < 1e-4 for t in moved for c in t) and abs(zs[0] + 0.5) < 1e-3 and abs(zs[1] - 0.9) < 1e-3, f"{moved} {zs}")
clear()

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

# 5b. opened from the map editor ("Edit in Blender"): written back to the project by itself,
# where it came from, its facts and folders kept
clear()
well_record = scratch / "content" / "data" / "props" / "test_well.json"
rec = json.loads(well_record.read_text(encoding="utf-8"))
rec["folders"] = ["Town"]
well_record.write_text(bf.to_text(bf.ordered_prop(rec)), encoding="utf-8", newline="\n")
r = bpy.ops.battle_frontier.import_prop(prop_id="test_well")
bf.start_editing(bpy.context, "test_well")
check("an edit session: its .blend under build/blender/", (scratch / "build" / "blender" / "test_well.blend").is_file() and bpy.context.scene.get("bf_editing") == "test_well")
model_before = glb.read_bytes()
for o in bpy.data.objects:
    if o.type == "MESH" and not bf.is_bounds(o):
        o.scale.z *= 1.5
        o.location.z *= 1.5
bpy.context.view_layer.update()
message = bf.save_edited(bpy.context)
rec2 = json.loads(well_record.read_text(encoding="utf-8"))
check("saved back: taller, its folders, category and flags kept", abs(rec2.get("height", 0) - 3.3) < 0.08 and rec2.get("folders") == ["Town"] and rec2.get("category") == "decoration" and rec2.get("unstackable") is True,
      f"{message} · {json.dumps(rec2)}")
check("  its model rewritten", glb.read_bytes() != model_before)
# a placeholder stays a placeholder: written back to temporary/, never content/
tmp_props = scratch / "temporary" / "data" / "props"
tmp_props.mkdir(parents=True, exist_ok=True)
crate = json.loads((scratch / "content" / "data" / "props" / "crate.json").read_text(encoding="utf-8"))
crate.update({"id": "temp_crate", "name": "Temp crate"})
(tmp_props / "temp_crate.json").write_text(bf.to_text(bf.ordered_prop(crate)), encoding="utf-8", newline="\n")
clear()
r = bpy.ops.battle_frontier.import_prop(prop_id="temp_crate")
check("imports a placeholder prop too", r == {"FINISHED"} and any(o.type == "MESH" for o in bpy.data.objects))
bf.start_editing(bpy.context, "temp_crate")
check("  untouched, it isn't written back (a stand-in stays one)", bf.save_edited(bpy.context) == "" and not (scratch / "temporary" / "assets" / "models" / "temp_crate.glb").exists())
for o in bpy.data.objects:
    if o.type == "MESH" and not bf.is_bounds(o):
        o.scale.x *= 0.8
bpy.context.view_layer.update()
bf.save_edited(bpy.context)
saved_tmp = json.loads((tmp_props / "temp_crate.json").read_text(encoding="utf-8"))
check("  written back to temporary/, never content/", (scratch / "temporary" / "assets" / "models" / "temp_crate.glb").is_file()
      and not (scratch / "content" / "assets" / "models" / "temp_crate.glb").exists() and not (scratch / "content" / "data" / "props" / "temp_crate.json").exists()
      and saved_tmp.get("model") == "models/temp_crate.glb")
del bpy.context.scene["bf_editing"]
clear()

# 5c. a model request made into a model (request_kit.py, the recipe in requests.md): a small
# creature of shapes, fused, with eyes, fitted, rigged with an idle, rendered and added; a
# design that isn't the user's goes to temporary/, one that is to content/
import model_requests as mr  # noqa: E402
import request_kit as kit  # noqa: E402
from mathutils.bvhtree import BVHTree  # noqa: E402

# the reshaping tools: a tube, edits by hand, a modifier, sculpting
clear()
horn = kit.tube([(0, 0, 0), (0.1, 0, 0.3), (0.3, 0, 0.5)], 0.08, "horn", radius_end=0.01)
lo, hi = kit.bounds([horn])
check("a tube follows its points and tapers, its ends closed", len(horn.data.polygons) == 16 * 12 + 2 and 0.49 < hi.z < 0.53 and -0.09 < lo.z < 0.0 and hi.x > 0.29,
      f"{len(horn.data.polygons)} faces, {lo[:]} to {hi[:]}")
block = kit.box((0, 0, 0.5), (1, 1, 1), "stone")
with kit.editing(block) as bm:
    top = [f for f in bm.faces if f.calc_center_median().z > 0.99]
    kit.extrude(bm, top, (0, 0, 0.25))
lo, hi = kit.bounds([block])
check("edits by hand: a box's top extruded a quarter up", abs(hi.z - 1.25) < 1e-4 and len(block.data.polygons) == 10, f"{hi.z:.3f}, {len(block.data.polygons)} faces")
kit.modifier(block, "SUBSURF", levels=1)
check("a modifier, applied: subdivided", len(block.data.polygons) == 40 and not block.modifiers, f"{len(block.data.polygons)} faces")
ball = kit.sphere((0, 0, 0), 1.0, "skin", segments=32)
before = [v.co.copy() for v in ball.data.vertices]
moved = kit.sculpt(ball, "grab", (0, 0, 1), 0.5, strength=0.3, direction=(0, 0, 1))
after = [v.co for v in ball.data.vertices]
pole = max(range(len(before)), key=lambda i: before[i].z)
south = min(range(len(before)), key=lambda i: before[i].z)
check("sculpting: a grab lifts what's under it, fading, and leaves the rest", moved > 0 and abs(after[pole].z - before[pole].z - 0.3) < 0.01 and (after[south] - before[south]).length < 1e-6,
      f"{moved} moved, the pole by {after[pole].z - before[pole].z:.3f}")
smoothed = kit.sculpt(ball, "smooth", (0, 0, 1), 0.5, strength=1.0, times=3)
try:
    kit.sculpt(ball, "twist", (0, 0, 0), 1.0)
    refused = False
except ValueError:
    refused = True
check("  smooth too; a brush it doesn't know is refused", smoothed > 0 and refused)
clear()

picture = bpy.data.images.new("critter_ref", 32, 24)
picture.pixels = [0.45, 0.62, 0.3, 1.0] * (32 * 24)
picture.filepath_raw = str(scratch / "critter.png")
picture.file_format = "PNG"
picture.save()
made = mr.new(scratch, "Test critter", [scratch / "critter.png"], kind="creature", height=0.8, notes="a round green critter")
check("a request made from a picture, laid out as the editor writes it", made["id"] == "test_critter" and (scratch / "private" / "requests" / "test_critter" / "critter.png").is_file()
      and (scratch / "private" / "requests" / "test_critter" / "request.json").read_text(encoding="utf-8") == bf.to_text(mr.normal(made)))
job = kit.start(scratch / "private" / "requests" / "test_critter" / "model.py")
job.finishing = True
progress = job.record["progress"]
check("started: in progress, its bar running on an estimate", job.record["status"] == "in_progress" and progress["started"] > 0 and progress["minutes"] == mr.estimate(made) and progress["step"] == "building the model", json.dumps(progress))
parts = [kit.blob([("ellipsoid", (0, 0, 0.42), 0.42, (1.0, 1.15, 0.9)), ("ball", (0, -0.34, 0.66), 0.3)], "fur")]
for side in (-1, 1):
    parts.append(kit.capsule((0.17 * side, -0.14, 0.24), (0.19 * side, -0.16, 0.03), 0.07, "fur"))
    parts.append(kit.capsule((0.17 * side, 0.2, 0.24), (0.19 * side, 0.22, 0.03), 0.07, "fur"))
    parts.append(kit.slab([(-0.05, 0.0), (0.05, 0.0), (0.0, 0.2)], 0.03, "ear", at=(0.11 * side, -0.32, 0.72), rotation=(0, 20 * side, 0)))
parts.append(kit.capsule((0, 0.25, 0.4), (0, 0.6, 0.55), 0.05, "fur", radius_b=0.03))
belly = kit.ellipsoid((0, -0.36, 0.36), (0.2, 0.12, 0.18), "belly")
critter = kit.fuse(parts, "test_critter", {"fur": "#6f9a4a", "belly": "#e8dcb0", "ear": "#4d7a35"}, faces=3000, paint=[belly])
check("one clean mesh, in one piece (ears, legs and tail all on the body)", len([o for o in bpy.data.objects if o.type == "MESH"]) == 1 and kit.pieces(critter) == 1, f"{kit.pieces(critter)} pieces")
belly_index = [m.name for m in critter.data.materials].index("test_critter belly")
belly_verts = {i for p in critter.data.polygons if p.material_index == belly_index for i in p.vertices}
was = [v.co.copy() for v in critter.data.vertices]
moved = kit.sculpt(critter, "inflate", (0, -0.4, 0.36), 0.3, strength=0.01, region="belly")
changed = {i for i, v in enumerate(critter.data.vertices) if (v.co - was[i]).length > 1e-7}
check("  a stroke kept to one colour region moves only that region", moved > 0 and changed and changed <= belly_verts, f"{len(changed)} moved, {len(changed - belly_verts)} outside the belly")
face = bmesh.new()
face.from_mesh(critter.data)
face_tree = BVHTree.FromBMesh(face)
face.free()
# the eyes asked for in front of the face (where metaballs leave it is hard to tell): seated on it
kit.eyes(critter, [(-0.1, -0.7, 0.72), (0.1, -0.7, 0.72)], 0.055)
eye_index = critter.vertex_groups["eyes"].index
gaps = [min(face_tree.find_nearest(v.co)[3] for v in critter.data.vertices if any(g.group == eye_index for g in v.groups) and (v.co.x > 0) == right) for right in (False, True)]
check("  the eyes seated on its face, not floating", all(g < 0.02 for g in gaps), f"{gaps}")
kit.fit([critter], job.height)
lo, hi = kit.bounds([critter])
check("  as tall as asked, on the origin", abs(hi.z - 0.8) < 1e-3 and abs(lo.z) < 1e-3 and abs(lo.x + hi.x) < 0.02, f"{len(critter.data.polygons)} faces, {lo[:]} to {hi[:]}")
# fur on its fur, made mesh: strands the game draws (glTF has no particle hair)
before_fur = len(critter.data.polygons)
grown = kit.fur(critter, "fur", count=200, length=0.03)
fur_faces = sum(1 for p in critter.data.polygons[before_fur:] if critter.data.materials[p.material_index].name == "test_critter fur")
check("fur grown on its region, made mesh", grown == 200 and len(critter.data.polygons) == before_fur + 200 * 6 and fur_faces == 200 * 6, f"{len(critter.data.polygons) - before_fur} new faces")
wind_layer = critter.data.vertex_colors.get("wind")


def wind_of(o, polygons):
    layer = o.data.vertex_colors["wind"]
    return [tuple(layer.data[i].color) for p in polygons for i in p.loop_indices]


if wind_layer:
    strands = wind_of(critter, critter.data.polygons[before_fur:])
    body = wind_of(critter, critter.data.polygons[:before_fur])
check("  painted for the wind: the strands sway to their tips and flutter, each in its own time; the body stays",
      wind_layer is not None and max(c[0] for c in strands) > 0.99 and min(c[0] for c in strands) == 0.0 and max(c[1] for c in strands) > 0.6
      and len({round(c[2], 3) for c in strands}) > 50 and max(max(c[:3]) for c in body) == 0.0,
      f"{critter.data.vertex_colors.keys()}")
kit.fit([critter], job.height)  # its height with its fur on
names = [m.name for m in critter.data.materials]
used = {p.material_index for p in critter.data.polygons}
check("a material per colour region, glossy eyes, every one used", names == ["test_critter fur", "test_critter ear", "test_critter belly", "test_critter eye", "test_critter eye glint"] and used == set(range(5))
      and critter.data.materials["test_critter eye"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value < 0.1, f"{names}, used {sorted(used)}")
arm = kit.rig(critter, {"spine": [(0, 0.25, 0.4), (0, -0.05, 0.42), (0, -0.25, 0.48)], "head": [(0, -0.25, 0.48), (0, -0.32, 0.78)],
                        "leg.L": [(0.17, -0.14, 0.24), (0.19, -0.16, 0.02)], "leg.R": [(-0.17, -0.14, 0.24), (-0.19, -0.16, 0.02)],
                        "tail": [(0, 0.3, 0.38), (0, 0.42, 0.45), (0, 0.5, 0.52)]})
groups = {g.index: g.name for g in critter.vertex_groups}
bones = {b.name for b in arm.data.bones}
unweighted = [v.index for v in critter.data.vertices if not any(groups[g.group] in bones and g.weight > 1e-4 for g in v.groups)]
eye_index = critter.vertex_groups["eyes"].index
on_head = all(any(groups[g.group] == "head" and g.weight > 0.99 for g in v.groups) for v in critter.data.vertices if any(g.group == eye_index for g in v.groups))
check("rigged: spine, head, legs, tail, every vertex weighted, the eyes on the head", bones == {"spine.1", "spine.2", "head", "leg.L", "leg.R", "tail.1", "tail.2"} and not unweighted and on_head,
      f"{sorted(bones)}, {len(unweighted)} unweighted")
kit.idle(arm)
# its lines: the picture's, seen from a three-quarter; the front and the side drawn from them
folder_ = scratch / "private" / "requests" / "test_critter"
(folder_ / "lines.json").write_text(json.dumps({
    "pictures": {"critter.png": {"view": "three-quarter", "camera": {"turn": 35, "tilt": 20},
                                 "lines": [{"name": "outline", "closed": True, "points": [[3, 3], [29, 3], [29, 21], [3, 21]]}]}},
    "front": {"lines": [{"name": "outline", "closed": True, "points": [[10, 40], [90, 40], [90, 120], [10, 120]]}, {"name": "eye", "mark": [40, 60]},
                        {"name": "bone leg.L", "points": [[70, 90], [72, 120]]}]},
    "side": {"lines": [{"name": "outline", "closed": True, "points": [[0, 50], [150, 50], [150, 120], [0, 120]]},
                       {"name": "bone leg.L", "points": [[40, 95], [38, 120]]}]}}), encoding="utf-8")
drawn = job.profiles()
chains = job.chains()
want = [(0.2, -0.4, 0.292857), (0.22, -0.422857, 0.0)]
check("the user's bones carried into the front and the side: the rig's chains in cells (x from the front, y from the side)", list(chains) == ["leg.L"]
      and all(abs(a - b) < 1e-4 for got, w in zip(chains["leg.L"], want) for a, b in zip(got, w)), str(chains))
front = drawn.get("front", {}).get("outline", [])
check("its front and side from lines.json, in cells as it stands: the height asked, on the ground, centred", sorted(drawn) == ["front", "side"]
      and min(p[1] for p in front) == 0 and abs(max(p[1] for p in front) - 0.8) < 1e-9 and abs(min(p[0] for p in front) + 0.4) < 1e-9
      and drawn["front"]["eye"] == [(-0.1, 0.6)] and abs(max(p[0] for p in drawn["side"]["outline"]) - 0.857143) < 1e-5, json.dumps(drawn)[:300])
looks = [tuple(round(c, 6) for c in kit.camera_direction(t, 0)) for t in (0, 90, -90)]
check("  a picture's camera: 0 the front, 90 from the model's left, -90 from its right", looks == [(0.0, -1.0, 0.0), (1.0, 0.0, 0.0), (-1.0, 0.0, 0.0)], f"{looks}")
files = kit.previews(job, [critter, arm], size=200)
sheet = kit.compare(job, files, height=120)
views = json.loads((folder_ / "views.json").read_text(encoding="utf-8")) if (folder_ / "views.json").is_file() else {}
seen = views.get("seen_critter.png", {}).get("box", [0, 0, 0, 0])
front_box = views.get("preview_front.png", {}).get("box", [0, 0, 0, 0])
check("  the model seen from the picture's camera too, and how each render sees the world", sorted(views) == ["preview_front.png", "preview_side.png", "seen_critter.png"]
      and (folder_ / "seen_critter.png").is_file() and abs((front_box[0] + front_box[2]) / 2 - 100) < 2 and abs((front_box[1] + front_box[3]) / 2 - 100) < 2
      and abs(max(seen[2] - seen[0], seen[3] - seen[1]) - 200 / 1.15) < 2 and views["preview_side.png"]["right"] == [0.0, 1.0, 0.0], json.dumps(views)[:400])
check("rendered from the front, the side, three-quarters, the idle in four moments, and the sheet", files == ["preview_front.png", "preview_side.png", "preview_three_quarter.png"]
      and all((folder_ / f).is_file() for f in files + ["preview_idle.png", "compare.png"]) and sheet == folder_ / "compare.png")
kit.finish(job, [critter, arm], files, reply="test")
done = mr.load(scratch, "test_critter")
critter_glb = scratch / "temporary" / "assets" / "models" / "test_critter.glb"
critter_record = scratch / "temporary" / "data" / "props" / "test_critter.json"
check("added to temporary/ (not the user's design), never content/", critter_glb.is_file() and critter_record.is_file() and not (scratch / "content" / "data" / "props" / "test_critter.json").exists())
record = json.loads(critter_record.read_text(encoding="utf-8")) if critter_record.is_file() else {}
check("  a character, measured", record.get("category") == "character" and record.get("model") == "models/test_critter.glb" and abs(record.get("height", 0) - 0.8) < 0.01, json.dumps(record))
check("the request done: its renders, its prop, its bar full", done["status"] == "done" and done["prop"] == "test_critter" and done["previews"] == files and done["progress"]["done"] == 1.0 and done["reply"] == "test", json.dumps(done))
critter_doc = glb_doc(critter_glb) if critter_glb.is_file() else {}
check("its rig and its idle in the model", len(critter_doc.get("skins", [])) == 1 and [a.get("name") for a in critter_doc.get("animations", [])] == ["idle"], f"{len(critter_doc.get('skins', []))} skins")
run = subprocess.run(["cargo", "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "check", str(critter_glb)], cwd=str(project), capture_output=True, text=True)
check("the engine reads its rig and its idle", "a rig of 7 joints" in run.stdout and "idle (2.0 s)" in run.stdout, (run.stdout.strip() or run.stderr.strip())[-300:])
check("  and its fur's wind paint", "painted for the wind" in run.stdout, (run.stdout.strip() or run.stderr.strip())[-300:])
# a design that is the user's: into content/
mr.new(scratch, "Test pot", [scratch / "critter.png"], kind="prop", height=0.5, mine=True)
job = kit.start(scratch / "private" / "requests" / "test_pot" / "model.py")
job.finishing = True
pot = kit.fuse([kit.lathe([(0.0, 0.0), (0.2, 0.0), (0.28, 0.2), (0.18, 0.45), (0.2, 0.5), (0.0, 0.5)], "clay")], "test_pot", {"clay": "#b8693e"}, smooth=False)
kit.fit([pot], job.height)
kit.finish(job, [pot], [])
check("a design of the user's goes to content/", (scratch / "content" / "data" / "props" / "test_pot.json").is_file() and (scratch / "content" / "assets" / "models" / "test_pot.glb").is_file()
      and not (scratch / "temporary" / "data" / "props" / "test_pot.json").exists() and mr.load(scratch, "test_pot")["status"] == "done")
check("  low-poly: flat shaded, few faces", not any(p.use_smooth for p in pot.data.polygons) and len(pot.data.polygons) <= 800, f"{len(pot.data.polygons)} faces")
clear()

# grass (requests.md, "Grass"): fitted to fill its cells exactly, so copies side by side join; a
# variant of it (requests.md, "Variants") rendered and added as a prop of its own, listed in the
# request; a grass prop, out of the way
mr.new(scratch, "Test tuft", [scratch / "critter.png"], kind="grass", height=0.3, cells=[2, 1], variants=1)
job = kit.start(scratch / "private" / "requests" / "test_tuft" / "model.py")
job.finishing = True


def tuft(seed):
    blades = [kit.capsule((x, y, 0.0), (x * 1.1, y * 1.1, 0.5 + 0.1 * (seed % 3)), 0.03, "blade") for x, y in ((-0.4, -0.2), (0.0, 0.1), (0.35, -0.1))]
    return [kit.part("blades", blades, {"blade": "#4a8a3a"}, remesh=False)]


made = tuft(2)
kit.fit_cells(made, job.cells, job.height)
lo, hi = kit.bounds(made)
check("grass fitted to its cells exactly and its height, on the origin", job.grass and job.cells == (2, 1) and abs(hi.x - lo.x - 2) < 1e-4 and abs(hi.y - lo.y - 1) < 1e-4
      and abs(hi.z - 0.3) < 1e-4 and abs(lo.z) < 1e-4 and abs(lo.x + hi.x) < 1e-4 and abs(lo.y + hi.y) < 1e-4, f"{lo[:]} {hi[:]}")
files = [kit.variant_preview(job, made, 2)]
kit.finish_variant(job, 2, made)
kit.drop(made)
check("  its variant rendered and added as a prop of its own, out of the scene after", files == ["variant_2.png"] and (job.dir / "variant_2.png").is_file()
      and (scratch / "temporary" / "data" / "props" / "test_tuft_variant_2.json").is_file() and not kit.parts(), f"{files} {kit.parts()}")
made = tuft(1)
kit.fit_cells(made, job.cells, job.height)
kit.finish(job, made, files)
done = mr.load(scratch, "test_tuft")
props_dir = scratch / "temporary" / "data" / "props"
tuft_record = json.loads((props_dir / "test_tuft.json").read_text(encoding="utf-8")) if (props_dir / "test_tuft.json").is_file() else {}
variant_record = json.loads((props_dir / "test_tuft_variant_2.json").read_text(encoding="utf-8")) if (props_dir / "test_tuft_variant_2.json").is_file() else {}
check("  the model a grass prop out of the way, filling its 2 x 1 cells; its variant named so, the same", tuft_record.get("category") == "grass" and not tuft_record.get("blocks", False)
      and tuft_record.get("footprint") == [2, 1] and variant_record.get("name") == "Test tuft (variant 2)" and variant_record.get("category") == "grass"
      and not variant_record.get("blocks", False) and variant_record.get("footprint") == [2, 1], json.dumps([tuft_record, variant_record]))
check("  the request done, its variant listed, its render with the model's", done["status"] == "done" and done["prop"] == "test_tuft" and done["variant_props"] == ["test_tuft_variant_2"]
      and done["previews"] == files, json.dumps({k: done[k] for k in ("status", "prop", "variant_props", "previews")}))
check("  light enough to grow: its .glb's vertices within its cells' most", 0 < mr.glb_vertices(scratch / "temporary" / "assets" / "models" / "test_tuft.glb") <= 2 * mr.GRASS_MOST)
clear()

# grass too heavy to grow over a field (more than mr.GRASS_MOST vertices a cell, as the engine
# counts them): refused, nothing written to the project, the request not done
mr.new(scratch, "Heavy sod", [scratch / "critter.png"], kind="grass", height=0.3)
job = kit.start(scratch / "private" / "requests" / "heavy_sod" / "model.py")
job.finishing = True
Matrix = __import__("mathutils").Matrix
dense = solid("dense sod", lambda bm: [bmesh.ops.create_cone(bm, cap_ends=True, segments=32, radius1=0.04, radius2=0.0, depth=0.3,
                                                             matrix=Matrix.Translation((-0.45 + 0.1 * (i % 10), -0.45 + 0.3 * (i // 10), 0.15))) for i in range(40)],
              colour=(0.3, 0.6, 0.3, 1))
kit.fit_cells([dense], job.cells, job.height)
try:
    kit.finish(job, [dense], [])
    refused = ""
except ValueError as e:
    refused = str(e)
check("grass too heavy to grow refused, with its count; nothing written, not done", "too heavy to grow" in refused and "at most 1000" in refused
      and not (scratch / "temporary" / "data" / "props" / "heavy_sod.json").exists() and not (scratch / "temporary" / "assets" / "models" / "heavy_sod.glb").exists()
      and not (job.dir / "weighed").exists() and mr.load(scratch, "heavy_sod")["status"] != "done", refused)
clear()

# 5e. a model painted for the wind: painted from its height (a stem and its leaves, each its
# own phase), added to the project (its paint the second colour layer, marked so; the meshes
# left as they were), read by the engine, and back from its .glb with its "wind" layer
clear()
stem = solid("shrub stem", lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.08, radius2=0.05, depth=1.0, matrix=__import__("mathutils").Matrix.Translation((0, 0, 0.5))), colour=(0.4, 0.25, 0.1, 1))
leaves = solid("shrub leaves", lambda bm: bmesh.ops.create_uvsphere(bm, u_segments=10, v_segments=6, radius=0.3, matrix=__import__("mathutils").Matrix.Translation((0, 0, 1.2))), colour=(0.2, 0.6, 0.2, 1))
for o in bpy.context.view_layer.objects:
    o.select_set(o in (stem, leaves))
bpy.context.view_layer.objects.active = stem
r = bpy.ops.battle_frontier.wind_from_height(flutter=0.5)
foot = min(c[0] for c in wind_of(stem, stem.data.polygons)) if stem.data.vertex_colors.get("wind") else 1
top = max(c[0] for c in wind_of(leaves, leaves.data.polygons)) if leaves.data.vertex_colors.get("wind") else 0
phase = {round(c[2], 4) for o in (stem, leaves) for c in wind_of(o, o.data.polygons)} if leaves.data.vertex_colors.get("wind") else set()
check("wind paint from height: a \"wind\" layer each, rooted at the foot, free at the top, each part in its own time", r == {"FINISHED"} and foot == 0.0 and top > 0.99 and len(phase) == 2, f"{foot} {top} {phase}")
painted_before = [tuple(c) for c in wind_of(leaves, leaves.data.polygons)]
r = bpy.ops.battle_frontier.add_to_project(asset_id="test_shrub", asset_name="Test shrub", category="flora")
shrub_glb = scratch / "content" / "assets" / "models" / "test_shrub.glb"
shrub = glb_doc(shrub_glb) if shrub_glb.is_file() else {}
prims = [p for m in shrub.get("meshes", []) for p in m.get("primitives", [])]
marked = [n for n in shrub.get("nodes", []) if "bf_wind" in json.dumps(n.get("extras", {}))] + [m for m in shrub.get("meshes", []) if "bf_wind" in json.dumps(m.get("extras", {}))]
check("added: its colours first, its wind paint second, marked so", r == {"FINISHED"} and prims and all("COLOR_0" in p["attributes"] and "COLOR_1" in p["attributes"] for p in prims) and marked,
      f"{[sorted(p['attributes']) for p in prims]}, {len(marked)} marked")
check("  the meshes as they were: their one \"wind\" layer", [o.data.vertex_colors.keys() for o in (stem, leaves)] == [["wind"], ["wind"]], f"{[o.data.vertex_colors.keys() for o in (stem, leaves)]}")
# (a new colour layer starts as a copy of the one active, here the wind's paint: never left so)
whites = [c for p in prims if "COLOR_0" in p["attributes"] for c in glb_values(shrub_glb, p["attributes"]["COLOR_0"])]
check("  its colours white, as it has none of its own (not a copy of its wind paint)", whites and min(min(c) for c in whites) > 0.999,
      f"{len(whites)} corners, the darkest {min((min(c) for c in whites), default=None)}")
run = subprocess.run(["cargo", "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "check", str(shrub_glb)], cwd=str(project), capture_output=True, text=True)
check("  the engine reads its wind paint", "painted for the wind" in run.stdout, (run.stdout.strip() or run.stderr.strip())[-300:])
clear()
r = bpy.ops.battle_frontier.import_prop(prop_id="test_shrub")
back = [o for o in bpy.data.objects if o.type == "MESH" and not bf.is_bounds(o)]
layers = [o.data.vertex_colors.keys() for o in back]
check("  back from its .glb, its paint the \"wind\" layer again", r == {"FINISHED"} and back and all(k[1:] == ["wind"] for k in layers), f"{layers}")
if back and all("wind" in o.data.vertex_colors for o in back):
    tops = [max(c[0] for c in wind_of(o, o.data.polygons)) for o in back]
    check("    as it was painted", max(tops) > 0.98, f"{tops}")
clear()

# 5e2. a plant painted as one (kit.plant_wind; requests.md, "The wind"): a stalk with a leaf on
# its tip and a leaf on the ground beside it, both made thick. One time for all of it (its
# leaves lean together); the sway rising over the whole plant, so the leaf's foot sways as its
# stalk's tip does; each leaf fluttering from its own foot to its far end, the stalk never
translation = __import__("mathutils").Matrix.Translation


def thick_leaves(bm):
    for at, size in (((0.1, 0.0, 0.4), 0.1), ((-0.3, 0.0, 0.02), 0.08)):
        grid = bmesh.ops.create_grid(bm, x_segments=4, y_segments=2, size=size, matrix=translation(at))
        bmesh.ops.solidify(bm, geom=[f for f in bm.faces if all(v in grid["verts"] for v in f.verts)], thickness=0.02)


stalk = solid("plant stalk", lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, segments=6, radius1=0.02, radius2=0.015, depth=0.4, matrix=translation((0, 0, 0.2))), colour=(0.3, 0.5, 0.2, 1))
plant = solid("plant leaves", thick_leaves, colour=(0.4, 0.7, 0.3, 1))
kit.plant_wind([plant, stalk], root=(0, 0, 0), sway=0.4, flutter=0.5, stems=[stalk])


def paint_of(o):
    layer = o.data.vertex_colors["wind"]
    out = {}
    for p in o.data.polygons:
        for i, v in zip(p.loop_indices, p.vertices):
            out.setdefault(v, set()).add(tuple(round(c, 4) for c in layer.data[i].color[:3]))
    return out


if all(o.data.vertex_colors.get("wind") for o in (plant, stalk)):
    leaf_paint, stalk_paint = paint_of(plant), paint_of(stalk)
    one = {c[2] for paints in (leaf_paint, stalk_paint) for cs in paints.values() for c in cs}
    corners_agree = all(len(cs) == 1 for paints in (leaf_paint, stalk_paint) for cs in paints.values())
    check("a plant painted as one: one time for all of it, every corner of a vertex the same", len(one) == 1 and corners_agree, f"{len(one)} phases")
    # (the paint is kept in steps of 1/255)
    near = lambda a, b: abs(a - b) < 0.005  # noqa: E731
    paint = lambda paints, v: next(iter(paints[v]))  # noqa: E731
    places = [(o.data.vertices[v].co, paint(paints, v)) for o, paints in ((plant, leaf_paint), (stalk, stalk_paint)) for v in paints]
    reach = max(co.length for co, _ in places)
    off = [(co[:], c) for co, c in places if not near(c[0], 0.4 * (co.length / reach) ** 1.5)]
    check("  swaying by one measure over the whole plant (a leaf as its stalk, as far out), from its root to its farthest", not off, f"{len(off)} off, e.g. {off[:2]}")
    # each leaf's foot: where the stalk's tip (its end furthest from the root) holds it, else its
    # point nearest the root
    pieces = bf._islands(plant.data)
    tip = max((v.co for v in stalk.data.vertices), key=lambda co: co.length)
    foot, far = {}, {}
    for v in leaf_paint:
        co, k = plant.data.vertices[v].co, pieces[v]
        held = lambda c: (c - tip).length < kit.HELD  # noqa: E731
        if k not in foot or (held(co) and (not held(foot[k]) or (co - tip).length < (foot[k] - tip).length)) or (not held(foot[k]) and not held(co) and co.length < foot[k].length):
            foot[k] = co
    feet = sorted(foot.values(), key=lambda f: f.z)
    check("  the leaf on the stalk held at the stalk's tip, the other at its point nearest the root", len(feet) == 2 and feet[0].z < 0.05 and (feet[1] - tip).length < kit.HELD, f"{[f[:] for f in feet]}")
    for v in leaf_paint:
        far[pieces[v]] = max(far.get(pieces[v], 0.0), (plant.data.vertices[v].co - foot[pieces[v]]).length)
    wrong = [v for v in leaf_paint if not near(paint(leaf_paint, v)[1], 0.5 * (plant.data.vertices[v].co - foot[pieces[v]]).length / far[pieces[v]])]
    check("  each leaf fluttering from its own foot to its far end, the stalk never", len(foot) == 2 and not wrong and all(paint(stalk_paint, v)[1] == 0.0 for v in stalk_paint), f"{len(foot)} leaves, {len(wrong)} off")
else:
    check("a plant painted as one", False, "no wind layer")
clear()

# 5f. a tree: its cells are its foot's, up to a character's height, the crown spreading over the cells
# round it (walked under) left out; it stands on its trunk's middle, the crown where it was beside it
Matrix = __import__("mathutils").Matrix
trunk = solid("tree trunk", lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, segments=10, radius1=0.4, radius2=0.25, depth=6.0, matrix=Matrix.Translation((0, 0, 3.0))),
              colour=(0.35, 0.2, 0.1, 1), location=(3.0, 1.0, 0.0))
crown = solid("tree crown", lambda bm: bmesh.ops.create_uvsphere(bm, u_segments=12, v_segments=8, radius=2.5), colour=(0.2, 0.5, 0.2, 1), location=(4.0, 1.0, 6.0))
for o in bpy.context.view_layer.objects:
    o.select_set(o in (trunk, crown))
bpy.context.view_layer.objects.active = trunk
r = bpy.ops.battle_frontier.add_to_project(asset_id="test_tree", asset_name="Test tree", category="tree")
tree_path = scratch / "content" / "data" / "props" / "test_tree.json"
tree = json.loads(tree_path.read_text(encoding="utf-8")) if tree_path.is_file() else {}
check("a tree: its cells its trunk's foot, not its crown's; its whole height", r == {"FINISHED"} and tree.get("footprint") == [1, 1] and abs(tree.get("height", 0) - 8.5) < 0.05,
      f"{tree.get('footprint')} {tree.get('height')}")
tree_doc = glb_doc(scratch / "content" / "assets" / "models" / "test_tree.glb") if r == {"FINISHED"} else {}
at = {n.get("name"): [round(v, 2) + 0.0 for v in n.get("translation", [0, 0, 0])] for n in tree_doc.get("nodes", [])}
check("  standing on its trunk's middle, the crown where it was beside it", at.get("tree trunk") == [0.0, 0.0, 0.0] and at.get("tree crown") == [1.0, 6.0, 0.0], f"{at}")
clear()

# 5g. a set of floor tiles (requests.md, "A set of floor tiles"): two tiles, each modelled a cell
# wide on the origin (a flag, and a long slab's half running on into the next cell), laid out by
# the game's rules and rendered from above and slanting, then added as floor props of the set
# where they were modelled; made again without the half, the half the first run made goes
import contextlib  # noqa: E402
import io  # noqa: E402
mr.new(scratch, "Test paving", [scratch / "critter.png"], kind="floor", notes="grey flags, long slabs in halves")
job = kit.start(scratch / "private" / "requests" / "test_paving" / "model.py")
check("a set of floor tiles: the job knows it", job.floor and not job.creature)


def paving(job, half=True):
    flag = kit.part("flag", [kit.box((0, 0, 0.025), (0.96, 0.96, 0.05), "stone")], {"stone": "#8f8a80"}, remesh=False)
    kit.floor_tile(job, "Flag", [flag], ["joint"] * 4, weight=2.0)
    if half:
        # its slab runs on past its east edge, where the other half (this one turned round) goes on
        piece = kit.part("half", [kit.box((0.15, 0, 0.025), (0.7, 0.96, 0.05), "stone")], {"stone": "#a39d90"}, remesh=False)
        kit.floor_tile(job, "Half", [piece], ["joint", "long", "joint", "joint"], turns=True)


paving(job)
said = io.StringIO()
with contextlib.redirect_stdout(said):
    files = kit.layout(job, cells=(4, 3), size=160)
meshes = [o for o in bpy.data.objects if o.type == "MESH"]
check("laid out: from above and slanting, every edge fitting, its copies gone after", files == ["layout.png", "layout_side.png"]
      and all((job.dir / f).is_file() for f in files) and "4 x 3 cells, 0 edges that don't fit" in said.getvalue()
      and sorted(o.name for o in meshes) == ["flag", "half"] and not any(o.hide_render for o in meshes), said.getvalue()[-300:])
top = bpy.data.images.load(str(job.dir / "layout.png")) if (job.dir / "layout.png").is_file() else None
shades = list(top.pixels)[0::4] if top else [0.0]
check("  the floor in the picture from above, not the background alone", top is not None and tuple(top.size) == (160, 160) and max(shades) - min(shades) > 0.1,
      f"{tuple(top.size) if top else None}, {min(shades):.2f} to {max(shades):.2f}")
if top:
    bpy.data.images.remove(top)
job.finishing = True
made = kit.finish_floor_set(job, files, reply="A flag, and a long slab in halves.")
props, models = scratch / "temporary" / "data" / "props", scratch / "temporary" / "assets" / "models"
recs = {t: json.loads((props / f"{t}.json").read_text(encoding="utf-8")) for t in (made or []) if (props / f"{t}.json").is_file()}
flag_rec, half_rec = recs.get("test_paving_flag", {}), recs.get("test_paving_half", {})
check("added: each tile a floor prop of the set, named after the request and the tile, in temporary/", made == ["test_paving_flag", "test_paving_half"]
      and flag_rec.get("category") == "floor" and not flag_rec.get("blocks") and flag_rec.get("name") == "Test paving: Flag"
      and flag_rec.get("floor") == {"set": "test_paving", "edges": ["joint"] * 4, "weight": 2.0}
      and half_rec.get("floor") == {"set": "test_paving", "edges": ["joint", "long", "joint", "joint"], "weight": 1.0, "turns": True}
      and (models / "test_paving_half.glb").is_file(), json.dumps(recs)[:600])
check("  laid out as the project lays its records", all((props / f"{t}.json").read_text(encoding="utf-8") == bf.to_text(bf.ordered_prop(r)) for t, r in recs.items()))
half_doc = glb_doc(models / "test_paving_half.glb") if (models / "test_paving_half.glb").is_file() else {}
spans = [half_doc["accessors"][q["attributes"]["POSITION"]] for m in half_doc.get("meshes", []) for q in m["primitives"]]
xs = (min(a["min"][0] for a in spans), max(a["max"][0] for a in spans)) if spans else (9.0, 9.0)
moved = [n.get("translation", [0, 0, 0]) for n in half_doc.get("nodes", [])]
check("  each where it was modelled, never moved to stand on the origin: the half on its half", abs(xs[0] + 0.2) < 0.01 and abs(xs[1] - 0.5) < 0.01
      and all(abs(t[0]) < 1e-4 and abs(t[2]) < 1e-4 for t in moved), f"{xs} {moved}")
rec = mr.load(scratch, "test_paving")
check("  the request done, the set's first tile its prop, the layout its renders", rec["status"] == "done" and rec["prop"] == "test_paving_flag"
      and rec["previews"] == files and rec["reply"] == "A flag, and a long slab in halves.", json.dumps({k: rec[k] for k in ("status", "prop", "previews")}))
clear()
job = kit.start(scratch / "private" / "requests" / "test_paving" / "model.py")
job.finishing = True
paving(job, half=False)
kit.finish_floor_set(job, [], reply="Flags only.")
check("made again without the half: the half the first run made goes, its model too; the flag stays", (props / "test_paving_flag.json").is_file()
      and not (props / "test_paving_half.json").exists() and not (models / "test_paving_half.glb").exists())
clear()

# 5d. a creature in parts, as a modeller makes it for a rigger (requests.md, "Parts"): each part
# its own mesh, the jaw on its bone (the mouth closes), the tongue on its own chain, the eyes,
# legs and tail; the idle and a roar in the file, every part on one skin
mr.new(scratch, "Test lizard", [scratch / "critter.png"], kind="creature", height=0.8, notes="a lizard in parts")
job = kit.start(scratch / "private" / "requests" / "test_lizard" / "model.py")
job.finishing = True
C = {"skin": "#5f8f4a", "belly": "#d9cf9a", "mouth": "#9c3d52", "tongue": "#e0707a", "tooth": "#f2efe6"}
kit.part("body", [kit.ellipsoid((0, 0.05, 0.42), (0.17, 0.3, 0.15), "skin"), kit.capsule((0, -0.2, 0.47), (0, -0.4, 0.58), 0.1, "skin")], C,
         bones=["spine", "neck"], paint=[kit.ellipsoid((0, 0.0, 0.3), (0.12, 0.26, 0.07), "belly")])
head = kit.part("head", [kit.ellipsoid((0, -0.5, 0.66), (0.11, 0.13, 0.1), "skin"), kit.capsule((0, -0.56, 0.645), (0, -0.74, 0.63), 0.06, "skin", radius_b=0.04)], C, bone="head")
jaw = kit.part("jaw", [kit.capsule((0, -0.52, 0.585), (0, -0.73, 0.585), 0.042, "skin", radius_b=0.03),
                       kit.ellipsoid((0, -0.62, 0.61), (0.035, 0.09, 0.012), "mouth")], C, bone="jaw", remesh=False)
tongue = kit.part("tongue", [kit.capsule((0, -0.55, 0.605), (0, -0.7, 0.605), 0.017, "tongue", radius_b=0.011)], C, bones=["tongue"], remesh=False)
teeth = kit.part("teeth", [kit.cylinder((x, -0.71, 0.615), 0.008, 0.022, "tooth", top=0, rotation=(180, 0, 0)) for x in (-0.025, 0.025)], C, bone="head", remesh=False)
for sx, side in ((1, "L"), (-1, "R")):
    for y, kind in ((-0.15, "front"), (0.22, "hind")):
        kit.part(f"{kind} leg.{side}", [kit.capsule((sx * 0.13, y, 0.36), (sx * 0.16, y, 0.03), 0.045, "skin", radius_b=0.035),
                                        kit.ellipsoid((sx * 0.16, y - 0.04, 0.02), (0.05, 0.07, 0.02), "skin")], C, bones=[f"{kind} leg.{side}", "spine"])
tail = kit.part("tail", [kit.tube([(0, 0.3, 0.42), (0, 0.5, 0.38), (0, 0.7, 0.3), (0, 0.85, 0.24)], 0.07, "skin", radius_end=0.012)], C, bones=["tail", "spine"])
lizard_eyes = kit.eyes(head, [(0.07, -0.58, 0.7), (-0.07, -0.58, 0.7)], 0.025, colour="#1d1a22", separate=True)
made = kit.parts()
check("a creature in parts, each its own mesh", sorted(o.name for o in made) == sorted(["body", "head", "jaw", "tongue", "teeth", "front leg.L", "hind leg.L", "front leg.R", "hind leg.R", "tail", "eye.L", "eye.R"])
      and len([o for o in bpy.data.objects if o.type == "MESH"]) == len(made), f"{sorted(o.name for o in made)}")
check("  crisp parts keep each shape's region", [m.name for m in jaw.data.materials] == ["jaw skin", "jaw mouth"] and {q.material_index for q in jaw.data.polygons} == {0, 1})
kit.fit(made, job.height)
lizard = kit.rig(made, {
    "spine": [(0, 0.3, 0.42), (0, 0.0, 0.43), (0, -0.25, 0.48)], "neck": [(0, -0.25, 0.48), (0, -0.42, 0.6)], "head": [(0, -0.42, 0.6), (0, -0.55, 0.72)],
    "jaw": [(0, -0.5, 0.6), (0, -0.72, 0.585)], "tongue": [(0, -0.54, 0.605), (0, -0.62, 0.605), (0, -0.7, 0.605)],
    **{f"{kind} leg.{side}": [(sx * 0.13, y, 0.38), (sx * 0.15, y, 0.2), (sx * 0.16, y, 0.03)] for sx, side in ((1, "L"), (-1, "R")) for y, kind in ((-0.15, "front"), (0.22, "hind"))},
    "tail": [(0, 0.3, 0.42), (0, 0.5, 0.38), (0, 0.7, 0.3), (0, 0.85, 0.24)]}, parents={"jaw": "head", "tongue": "jaw", "tail": "spine.1"})
lizard_bones = {b.name for b in lizard.data.bones}


def weighted_by(o):
    names = {g.index: g.name for g in o.vertex_groups}
    return {names[g.group] for v in o.data.vertices for g in v.groups if g.weight > 1e-4 and names[g.group] in lizard_bones}


front_leg = next(o for o in made if o.name == "front leg.L")
check("  the jaw, the teeth, the eyes each on one bone, whole", weighted_by(jaw) == {"jaw"} and weighted_by(teeth) == {"head"} and all(weighted_by(e) == {"head"} for e in lizard_eyes)
      and all(abs(g.weight - 1) < 1e-6 for v in jaw.data.vertices for g in v.groups), f"{weighted_by(jaw)} {weighted_by(teeth)}")
check("  the tongue on its own chain; a leg on its own and the spine; the tail on its own and the spine; every vertex weighted",
      weighted_by(tongue) == {"tongue.1", "tongue.2"} and "front leg.1.L" in weighted_by(front_leg) and weighted_by(front_leg) <= {"front leg.1.L", "front leg.2.L", "spine.1", "spine.2"}
      and weighted_by(tail) <= {"tail.1", "tail.2", "tail.3", "spine.1", "spine.2"} and all(all(any(g.weight > 1e-4 for g in v.groups) for v in o.data.vertices) for o in made),
      f"{weighted_by(tongue)} {weighted_by(front_leg)} {weighted_by(tail)}")


def posed(o):
    e = o.evaluated_get(bpy.context.evaluated_depsgraph_get())
    m = e.to_mesh()
    out = [e.matrix_world @ v.co for v in m.vertices]
    e.to_mesh_clear()
    return out


rest = (posed(jaw), posed(head))
lizard.pose.bones["jaw"].rotation_mode = "XYZ"
lizard.pose.bones["jaw"].rotation_euler = (0.5, 0, 0)
bpy.context.view_layer.update()
opened = (posed(jaw), posed(head))
lizard.pose.bones["jaw"].rotation_euler = (0, 0, 0)
bpy.context.view_layer.update()
check("  the jaw bone opens the mouth: the jaw moves, the head stays", max((a - b).length for a, b in zip(rest[0], opened[0])) > 0.02
      and max((a - b).length for a, b in zip(rest[1], opened[1])) < 1e-6)
kit.idle(lizard)
kit.action(lizard, "roar", {"jaw": [(0, (0, 0, 0)), (0.3, (30, 0, 0)), (0.8, (30, 0, 0)), (1, (0, 0, 0))], "tongue": [(0, (0, 0, 0)), (0.5, (-15, 0, 0)), (1, (0, 0, 0))]}, seconds=1.2)
files = kit.previews(job, made + [lizard], size=200)
lizard_dir = scratch / "private" / "requests" / "test_lizard"
check("  the idle plays, the roar kept on the rig; each previewed", lizard.animation_data.action.name == "idle" and sorted(t.name for t in lizard.animation_data.nla_tracks) == ["idle", "roar"]
      and (lizard_dir / "preview_idle.png").is_file() and (lizard_dir / "preview_roar.png").is_file())
kit.finish(job, made + [lizard], files, reply="test")
lizard_glb = scratch / "temporary" / "assets" / "models" / "test_lizard.glb"
lizard_doc = glb_doc(lizard_glb) if lizard_glb.is_file() else {}
on_skin = [n for n in lizard_doc.get("nodes", []) if "skin" in n and "mesh" in n]
check("  in the file: one skin for every part, the idle and the roar", len(lizard_doc.get("skins", [])) == 1 and len(on_skin) == len(made)
      and sorted(a.get("name") for a in lizard_doc.get("animations", [])) == ["idle", "roar"], f"{len(on_skin)} skinned nodes, {[a.get('name') for a in lizard_doc.get('animations', [])]}")
run = subprocess.run(["cargo", "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "check", str(lizard_glb)], cwd=str(project), capture_output=True, text=True)
check("  the engine reads every part on the rig, and both actions", f"a rig of {len(lizard_bones)} joints" in run.stdout and "idle (2.0 s)" in run.stdout and "roar (1.2 s)" in run.stdout,
      (run.stdout.strip() or run.stderr.strip())[-300:])
clear()

# 6. a whole map, exported by the project itself
os.environ["BATTLE_FRONTIER_ROOT"] = str(project)
clear()
r = bpy.ops.battle_frontier.export_map(map_id="test_meadow")
count = len(bpy.data.objects)
# (dozens: its props, its buildings' parts, its trees, real size and keeping their crowns' room)
check("exports and imports a map", r == {"FINISHED"} and count > 40, f"{count} objects")
names = {o.name for o in bpy.data.objects}
check("ground, props, trees and buildings apart", {"Ground", "Props", "Trees", "Buildings"} <= names)
clear()
r = bpy.ops.battle_frontier.import_export(file="test_meadow.glb")
check("imports an export", r == {"FINISHED"} and len(bpy.data.objects) > 40, f"{len(bpy.data.objects)} objects")

print("all good" if not failed else f"{len(failed)} failed", flush=True)
sys.exit(1 if failed else 0)
