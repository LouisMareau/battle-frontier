"""A rigged sample prop, made from scratch in Blender and added to the project through the
add-on: a flag pole whose flag waves (an armature of two bones and an "idle" action).

    blender --background --factory-startup --python tools/blender/make_flag.py -- PROJECT [ID]

Writes content/assets/models/<ID>.glb and content/data/props/<ID>.json (ID: flag_pole).
The test imports `build_flag` to make one in its scratch project.
"""
import math
import sys
from pathlib import Path

import bmesh
import bpy


def material(name, rgba):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = rgba
    return m


def build_flag(collection):
    """The pole, the flag skinned to two bones, the armature with its waving action. Returns
    the objects to select for "Add to the project"."""
    # the pole: a thin cylinder, 3 units tall, standing on the origin
    mesh = bpy.data.meshes.new("pole")
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=10, radius1=0.05, radius2=0.04, depth=3.0)
    bmesh.ops.translate(bm, verts=bm.verts, vec=(0.0, 0.0, 1.5))
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material("pole", (0.35, 0.33, 0.3, 1.0)))
    pole = bpy.data.objects.new("pole", mesh)
    collection.objects.link(pole)
    # the flag: a cloth of 6 x 3 quads from the top of the pole along +X
    mesh = bpy.data.meshes.new("flag")
    bm = bmesh.new()
    cols, rows = 6, 3
    grid = [[bm.verts.new((0.05 + 0.9 * c / cols, 0.0, 2.95 - 0.5 * r / rows)) for c in range(cols + 1)] for r in range(rows + 1)]
    for r in range(rows):
        for c in range(cols):
            bm.faces.new((grid[r][c], grid[r + 1][c], grid[r + 1][c + 1], grid[r][c + 1]))
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material("flag", (0.75, 0.12, 0.1, 1.0)))
    flag = bpy.data.objects.new("flag", mesh)
    collection.objects.link(flag)
    # the armature: "root" at the pole's top along the flag, "tip" after it
    arm_data = bpy.data.armatures.new("flag_rig")
    arm = bpy.data.objects.new("flag_rig", arm_data)
    collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    root = arm_data.edit_bones.new("root")
    root.head, root.tail = (0.05, 0.0, 2.7), (0.35, 0.0, 2.7)
    tip = arm_data.edit_bones.new("tip")
    tip.head, tip.tail = (0.35, 0.0, 2.7), (0.95, 0.0, 2.7)
    tip.parent = root
    tip.use_connect = True
    bpy.ops.object.mode_set(mode="OBJECT")
    # the flag's weights: the root holds the hoist, the tip takes over along the fly
    g_root = flag.vertex_groups.new(name="root")
    g_tip = flag.vertex_groups.new(name="tip")
    for v in flag.data.vertices:
        w = min(1.0, max(0.0, (v.co.x - 0.15) / 0.6))
        g_tip.add([v.index], w, "REPLACE")
        g_root.add([v.index], 1.0 - w, "REPLACE")
    flag.parent = arm
    mod = flag.modifiers.new("rig", "ARMATURE")
    mod.object = arm
    # the wave: the tip swings about the pole's axis, the root a little, over two seconds
    scene = bpy.context.scene
    scene.render.fps = 24
    scene.frame_start, scene.frame_end = 1, 48
    arm.animation_data_create()
    action = bpy.data.actions.new("idle")
    arm.animation_data.action = action
    bpy.ops.object.mode_set(mode="POSE")
    for name, swing, phase in (("tip", 0.5, 0.0), ("root", 0.18, 0.8)):
        bone = arm.pose.bones[name]
        bone.rotation_mode = "XYZ"
        for k in range(0, 49, 6):
            t = k / 48.0
            bone.rotation_euler = (0.0, 0.08 * math.sin(2 * math.pi * t * 2 + phase), swing * math.sin(2 * math.pi * t + phase))
            bone.keyframe_insert("rotation_euler", frame=k + 1)
    bpy.ops.object.mode_set(mode="OBJECT")
    for fc in action.fcurves:
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    return [pole, flag, arm]


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1:]
    project = Path(argv[0]).resolve()
    asset_id = argv[1] if len(argv) > 1 else "flag_pole"
    sys.path.insert(0, str(project / "tools" / "blender"))
    import os
    os.environ["BATTLE_FRONTIER_ROOT"] = str(project)
    import battle_frontier as bf
    bf.register()
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o, do_unlink=True)
    objects = build_flag(bpy.context.collection)
    for o in bpy.context.view_layer.objects:
        o.select_set(o in objects)
    bpy.context.view_layer.objects.active = objects[0]
    r = bpy.ops.battle_frontier.add_to_project(asset_id=asset_id, asset_name="Flag pole", category="decoration", blocks=True, unstackable=False, replace=True)
    print("add_to_project:", r)
