"""Building the model of a request in Blender (3.1 and newer): the helpers a request's model.py
uses. The recipe is tools/blender/requests.md; a request is private/requests/<id>/, made by
the map editor's Requests window (see model_requests.py).

    blender --background --factory-startup --python private/requests/<id>/model.py
    blender --background --factory-startup --python private/requests/<id>/model.py -- --finish

The first builds the model and renders it (preview_front.png, preview_side.png,
preview_three_quarter.png, and preview_idle.png for a creature) with compare.png: the
pictures above the renders, to compare. The second also adds it to the project (the add-on's
write_prop: temporary/, or content/ when the design is the user's) and marks the request done.

A model is built from shapes (spheres, ellipsoids, capsules, boxes, cylinders, rings, shapes
turned on a lathe, flat slabs, metaballs), each with a colour region; `fuse` makes them one
clean mesh (voxel remesh, cuts carved, decimated) whose faces take the material of the region
nearest them. Then `eyes`, `fit` (its height in cells, standing on the origin), for a creature
`rig` (automatic weights) and `idle`, then `previews`, `compare` and `finish`. One Blender
unit is one cell; the model faces the front view (-Y).

Any shape, or the fused body, can be reshaped too: `editing` (a bmesh by hand: extrude, inset,
bevel, move faces, edges and vertices), `modifier` (any of Blender's modifiers, applied),
`sculpt` (grab, inflate, smooth, flatten, pinch, crease brushes), `tube` (curved parts), `fur`.
And any of Blender's Python besides: the kit is a start, not a limit.
"""
import math
import subprocess
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Euler, Matrix, Vector
from mathutils.bvhtree import BVHTree

sys.path.insert(0, str(Path(__file__).resolve().parent))
import battle_frontier as bf  # noqa: E402
import model_requests as mr  # noqa: E402


# ---------------------------------------------------------------- the request

class Job:
    """The request a model.py builds: its record, its folder, the project, and whether this run
    adds the model to the project (`-- --finish`)."""

    def __init__(self, script):
        self.dir = Path(script).resolve().parent
        self.id = self.dir.name
        self.root = mr.find_root(self.dir)
        argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
        self.finishing = "--finish" in argv
        self.record = mr.load(self.root, self.id)

    @property
    def creature(self):
        return self.record["kind"] == "creature"

    @property
    def smooth(self):
        return self.record["style"] != "low_poly"

    @property
    def accurate(self):
        """As close to the pictures as it can be (requests.md, "Accurate")."""
        return self.record["style"] == "accurate"

    @property
    def pose(self):
        """A creature's: "t_pose" or "as_is"."""
        return self.record["pose"]

    @property
    def shapes(self):
        """{"min", "max", "unlimited"}: how many shapes to build it from."""
        return self.record["shapes"]

    @property
    def height(self):
        return float(self.record["height"])

    def pictures(self):
        return [self.dir / f for f in self.record["images"]]

    def step(self, text, done):
        """What the editor's bar says now (only while it's being made)."""
        if self.record["status"] == "in_progress":
            self.record = mr.step(self.root, self.id, text, done)


def start(script, minutes=None):
    """The request of the model.py at `script`: the scene emptied, the request in progress (a
    done one stays done unless this run finishes it again)."""
    job = Job(script)
    clear()
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = 1.0
    if job.record["status"] in ("waiting", "failed") or (job.record["status"] == "done" and job.finishing):
        job.record = mr.start(job.root, job.id, minutes)
    job.step("building the model", 0.15)
    r = job.record
    print(f"{job.id}: {r['name']}, a {r['kind']} {r['height']} cells tall, {r['style']}; pictures: {', '.join(r['images'])}", flush=True)
    return job


def clear():
    """Nothing left in the file: objects, meshes, materials, rigs, actions, lights."""
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o, do_unlink=True)
    for blocks in (bpy.data.meshes, bpy.data.materials, bpy.data.metaballs, bpy.data.armatures, bpy.data.actions, bpy.data.lights, bpy.data.cameras):
        for b in list(blocks):
            blocks.remove(b)


# ---------------------------------------------------------------- shapes

def _matrix(at=(0, 0, 0), rotation=(0, 0, 0), scale=(1, 1, 1)):
    """Moved to `at`, turned by `rotation` (degrees about X, Y, Z), scaled first."""
    return Matrix.Translation(Vector(at)) @ Euler([math.radians(a) for a in rotation]).to_matrix().to_4x4() @ Matrix.Diagonal(Vector((*scale, 1.0)))


def _link(name, bm, region):
    bm.normal_update()
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(o)
    o["bf_region"] = region
    return o


def sphere(at, radius, region, scale=(1, 1, 1), rotation=(0, 0, 0), segments=24):
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=max(6, segments // 2), radius=radius)
    bmesh.ops.transform(bm, matrix=_matrix(at, rotation, scale), verts=bm.verts)
    return _link("sphere", bm, region)


def ellipsoid(at, radii, region, rotation=(0, 0, 0), segments=24):
    """A sphere stretched to `radii` along X, Y and Z."""
    return sphere(at, 1.0, region, scale=radii, rotation=rotation, segments=segments)


def capsule(a, b, radius, region, radius_b=None, segments=16):
    """A rounded limb from `a` to `b`: `radius` at a, `radius_b` at b (the same by default)."""
    rb = radius if radius_b is None else radius_b
    a, b = Vector(a), Vector(b)
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=max(6, segments // 2), radius=radius, matrix=Matrix.Translation(a))
    bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=max(6, segments // 2), radius=rb, matrix=Matrix.Translation(b))
    axis = b - a
    if axis.length > 1e-6:
        place = Matrix.Translation((a + b) / 2) @ axis.to_track_quat("Z", "Y").to_matrix().to_4x4()
        bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=segments, radius1=radius, radius2=rb, depth=axis.length, matrix=place)
    return _link("capsule", bm, region)


def box(at, size, region, rotation=(0, 0, 0)):
    """A box of `size` (X, Y, Z) centred on `at`."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.transform(bm, matrix=_matrix(at, rotation, size), verts=bm.verts)
    return _link("box", bm, region)


def cylinder(at, radius, height, region, top=None, rotation=(0, 0, 0), segments=32):
    """Standing on `at` (the middle of its bottom), `height` up before `rotation`; `top` is
    the radius at the top (0: a cone)."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=segments, radius1=radius, radius2=radius if top is None else top, depth=height,
                          matrix=Matrix.Translation((0, 0, height / 2)))
    bmesh.ops.transform(bm, matrix=_matrix(at, rotation), verts=bm.verts)
    return _link("cylinder", bm, region)


def torus(at, radius, thickness, region, rotation=(0, 0, 0), segments=40, sides=12):
    """A ring lying flat (around Z) before `rotation`: `radius` to the middle of its tube,
    `thickness` the tube's radius."""
    bm = bmesh.new()
    rings = []
    for i in range(segments):
        u = i / segments * math.tau
        rings.append([bm.verts.new(((radius + thickness * math.cos(v)) * math.cos(u), (radius + thickness * math.cos(v)) * math.sin(u), thickness * math.sin(v)))
                      for v in (j / sides * math.tau for j in range(sides))])
    for i in range(segments):
        for j in range(sides):
            a, b = rings[i][j], rings[(i + 1) % segments][j]
            bm.faces.new((a, b, rings[(i + 1) % segments][(j + 1) % sides], rings[i][(j + 1) % sides]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.transform(bm, matrix=_matrix(at, rotation), verts=bm.verts)
    return _link("torus", bm, region)


def lathe(profile, region, at=(0, 0, 0), rotation=(0, 0, 0), segments=48):
    """A shape turned about the vertical (a vase, a post, a lantern): `profile` is (radius,
    height) points from the bottom up; the ends are closed (a radius of 0 ends in a point)."""
    bm = bmesh.new()
    angles = [k / segments * math.tau for k in range(segments)]
    rings = [[bm.verts.new((0.0, 0.0, z))] if r <= 1e-6 else [bm.verts.new((r * math.cos(t), r * math.sin(t), z)) for t in angles] for r, z in profile]
    for lo, hi in zip(rings, rings[1:]):
        if len(lo) == 1 and len(hi) == 1:
            continue
        for k in range(segments):
            k1 = (k + 1) % segments
            if len(lo) == 1:
                bm.faces.new((lo[0], hi[k], hi[k1]))
            elif len(hi) == 1:
                bm.faces.new((lo[k], lo[k1], hi[0]))
            else:
                bm.faces.new((lo[k], lo[k1], hi[k1], hi[k]))
    for ring in (rings[0], rings[-1]):
        if len(ring) > 2:
            bm.faces.new(ring)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.transform(bm, matrix=_matrix(at, rotation), verts=bm.verts)
    return _link("lathe", bm, region)


def slab(outline, thickness, region, at=(0, 0, 0), rotation=(0, 0, 0)):
    """A flat shape (an ear, a fin, a leaf, a horn's blade): `outline` is (x, z) points as the
    front view shows them, `thickness` deep along Y, on `at` before `rotation`."""
    bm = bmesh.new()
    front = [bm.verts.new((x, -thickness / 2, z)) for x, z in outline]
    back = [bm.verts.new((x, thickness / 2, z)) for x, z in outline]
    caps = [bm.faces.new(front), bm.faces.new(list(reversed(back)))]
    n = len(outline)
    for k in range(n):
        bm.faces.new((front[k], back[k], back[(k + 1) % n], front[(k + 1) % n]))
    bmesh.ops.triangulate(bm, faces=caps)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.transform(bm, matrix=_matrix(at, rotation), verts=bm.verts)
    return _link("slab", bm, region)


def blob(elements, region, resolution=None, threshold=0.6):
    """Metaballs melted into one soft shape, made a mesh. Elements: ("ball", at, radius),
    ("ellipsoid", at, radius, (sx, sy, sz)), ("capsule", at, radius, length), each with an
    optional rotation (degrees) last; a negative radius carves. A metaball's surface sits
    inside its radius (about 0.6 to 0.8 of it): size by the renders."""
    mb = bpy.data.metaballs.new("blob")
    mb.threshold = threshold
    reach = 0.0
    for el in elements:
        kind, at, radius = el[0], Vector(el[1]), float(el[2])
        rest = list(el[3:])
        e = mb.elements.new()
        e.type = {"ball": "BALL", "ellipsoid": "ELLIPSOID", "capsule": "CAPSULE"}[kind]
        e.co = at
        e.radius = abs(radius)
        e.use_negative = radius < 0
        if kind == "ellipsoid":
            e.size_x, e.size_y, e.size_z = rest.pop(0)
        elif kind == "capsule":
            e.size_x = rest.pop(0) / 2
        if rest:
            e.rotation = Euler([math.radians(a) for a in rest.pop(0)]).to_quaternion()
        reach = max(reach, abs(radius))
    mb.resolution = mb.render_resolution = resolution or max(reach / 12, 0.004)
    o = bpy.data.objects.new("blob", mb)
    bpy.context.collection.objects.link(o)
    me = bpy.data.meshes.new_from_object(o.evaluated_get(bpy.context.evaluated_depsgraph_get()))
    bpy.data.objects.remove(o, do_unlink=True)
    bpy.data.metaballs.remove(mb)
    out = bpy.data.objects.new("blob", me)
    bpy.context.collection.objects.link(out)
    out["bf_region"] = region
    return out


def mirrored(part, axis="x"):
    """A copy of a part on the other side (the creature's other ear, leg, eye socket...)."""
    o = part.copy()
    o.data = part.data.copy()
    bpy.context.collection.objects.link(o)
    scale = {"x": (-1, 1, 1), "y": (1, -1, 1), "z": (1, 1, -1)}[axis]
    o.data.transform(Matrix.Diagonal(Vector((*scale, 1.0))))
    o.data.flip_normals()
    o["bf_region"] = part["bf_region"]
    return o


def tube(points, radius, region, radius_end=None, segments=12, smoothness=8):
    """A curved part through `points` (a horn, a tail, a tentacle, an antenna, a handle):
    round, `radius` at the first point narrowing (or widening) to `radius_end` at the last,
    the path smoothed through them (Catmull-Rom), its ends closed."""
    pts = [Vector(p) for p in points]
    if len(pts) < 2:
        raise ValueError("a tube needs two points at least")
    rb = radius if radius_end is None else radius_end
    ext = [pts[0] * 2 - pts[1], *pts, pts[-1] * 2 - pts[-2]]
    path = []
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for k in range(smoothness):
            t = k / smoothness
            path.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t))
    path.append(pts[-1])
    bm = bmesh.new()
    rings = []
    normal = None
    for i, at in enumerate(path):
        ahead = (path[min(i + 1, len(path) - 1)] - path[max(i - 1, 0)]).normalized()
        # a frame carried along the path, so the tube doesn't twist
        normal = ahead.orthogonal().normalized() if normal is None else (normal - ahead * normal.dot(ahead)).normalized()
        side = ahead.cross(normal)
        r = radius + (rb - radius) * i / (len(path) - 1)
        rings.append([bm.verts.new(at + (normal * math.cos(a) + side * math.sin(a)) * r) for a in (k / segments * math.tau for k in range(segments))])
    for lo, hi in zip(rings, rings[1:]):
        for k in range(segments):
            bm.faces.new((lo[k], lo[(k + 1) % segments], hi[(k + 1) % segments], hi[k]))
    for ring in (rings[0], rings[-1]):
        bm.faces.new(ring)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _link("tube", bm, region)


# ---------------------------------------------------------------- reshaping: by hand, modifiers, sculpting

def modifier(obj, kind, **settings):
    """Any of Blender's modifiers on a mesh, applied for good: on a shape before `fuse`, or on
    the fused body after it. "SUBSURF" (levels=2), "BEVEL" (width=0.02, segments=3),
    "SOLIDIFY" (thickness=0.01), "SIMPLE_DEFORM" (deform_method="BEND", angle=0.5),
    "DISPLACE" (texture=..., strength=0.02), "CAST", "SMOOTH", "MIRROR", "ARRAY", "LATTICE",
    "SHRINKWRAP" (target=...), "WELD", "TRIANGULATE", "REMESH"... Returns the object."""
    _apply(obj, kind, **settings)
    return obj


class editing:
    """A mesh to edit by hand, as a bmesh in the object's own space (the world's, for the
    kit's shapes): extrude, inset, bevel, subdivide, bridge, delete, move faces, edges and
    vertices, with any of bmesh.ops; written back when the block ends (not on an error).

        with kit.editing(body) as bm:
            top = [f for f in bm.faces if f.calc_center_median().z > 0.9]
            kit.extrude(bm, top, (0, 0, 0.1))
    """

    def __init__(self, obj):
        self.obj = obj

    def __enter__(self):
        self.bm = bmesh.new()
        self.bm.from_mesh(self.obj.data)
        return self.bm

    def __exit__(self, kind, value, tb):
        try:
            if kind is None:
                self.bm.normal_update()
                self.bm.to_mesh(self.obj.data)
                self.obj.data.update()
        finally:
            self.bm.free()
        return False


def extrude(bm, faces, move):
    """In an `editing` block: faces pulled out by `move` with their sides, as Blender's extrude
    does it (bmesh's own extrude keeps the old faces inside the mesh: they go here). Returns
    the faces at the new end."""
    made = bmesh.ops.extrude_face_region(bm, geom=list(faces))["geom"]
    bmesh.ops.delete(bm, geom=list(faces), context="FACES_ONLY")
    bmesh.ops.translate(bm, verts=[e for e in made if isinstance(e, bmesh.types.BMVert)], vec=Vector(move))
    return [e for e in made if isinstance(e, bmesh.types.BMFace)]


def _region_index(me, region):
    index = next((i for i, m in enumerate(me.materials) if m and m.name.endswith(f" {region}")), None)
    if index is None:
        raise ValueError(f"no region {region!r}: {[m.name for m in me.materials]}")
    return index


BRUSHES = ("grab", "inflate", "smooth", "flatten", "pinch", "crease")


def sculpt(obj, brush, at, radius, strength=1.0, direction=(0, 0, 1), region=None, times=1):
    """Sculpting, scripted: one stroke of a brush on the mesh around `at`, its effect fading
    smoothly to nothing at `radius` (cells), as Blender's sculpt brushes work:
    "grab" moves the vertices by `direction` times `strength`; "inflate" pushes them out along
    their normals by `strength` cells (negative deflates); "smooth" relaxes them towards their
    neighbours (`strength` 0 to 1); "flatten" brings them to the plane through `at` that
    faces `direction`; "pinch" draws them towards `at`; "crease" cuts a groove, in by
    `strength` cells with a sharp falloff. `region`: only that colour region's vertices;
    `times`: the stroke again. Best on the fused body, where the mesh is dense (before `rig`).
    Returns how many vertices it moved."""
    if brush not in BRUSHES:
        raise ValueError(f"brush {brush!r}: one of {', '.join(BRUSHES)}")
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.normal_update()
    centre, way = Vector(at), Vector(direction)
    allowed = None
    if region is not None:
        index = _region_index(me, region)
        allowed = {v for f in bm.faces if f.material_index == index for v in f.verts}
    moved = 0
    for _ in range(max(1, times)):
        weights = {}
        for v in bm.verts:
            d = (v.co - centre).length
            if d < radius and (allowed is None or v in allowed):
                t = d / radius
                weights[v] = (1 - t) ** 3 if brush == "crease" else 1 - t * t * (3 - 2 * t)
        if brush == "smooth":
            targets = {v: sum((e.other_vert(v).co for e in v.link_edges), Vector()) / max(1, len(v.link_edges)) for v in weights}
        for v, w in weights.items():
            if brush == "grab":
                v.co += way * strength * w
            elif brush == "inflate":
                v.co += v.normal * strength * w
            elif brush == "smooth":
                v.co = v.co.lerp(targets[v], min(1.0, strength * w))
            elif brush == "flatten":
                n = way.normalized()
                v.co -= n * (v.co - centre).dot(n) * min(1.0, strength * w)
            elif brush == "pinch":
                v.co = v.co.lerp(centre, min(1.0, 0.5 * strength * w))
            elif brush == "crease":
                v.co -= v.normal * strength * w
        moved = len(weights)
        bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    me.update()
    return moved


# ---------------------------------------------------------------- one clean mesh

def _world_bmesh(o):
    """An object's mesh, modifiers and all, in world space."""
    bm = bmesh.new()
    bm.from_object(o, bpy.context.evaluated_depsgraph_get())
    bm.transform(o.matrix_world)
    bm.normal_update()
    return bm


def _join(bms):
    """World-space meshes in one."""
    out = bmesh.new()
    for b in bms:
        me = bpy.data.meshes.new("join")
        b.to_mesh(me)
        out.from_mesh(me)
        bpy.data.meshes.remove(me)
    return out


def _apply(o, kind, **settings):
    """A modifier applied for good."""
    mod = o.modifiers.new(kind.lower(), kind)
    for k, v in settings.items():
        setattr(mod, k, v)
    me = bpy.data.meshes.new_from_object(o.evaluated_get(bpy.context.evaluated_depsgraph_get()))
    o.modifiers.remove(mod)
    old, o.data = o.data, me
    bpy.data.meshes.remove(old)


def bounds(objects):
    """The meshes' box in the world, from their vertices as they are now (an object's
    bound_box lags behind until the scene updates, and follows a rig's pose)."""
    points = []
    for o in objects:
        if o.type != "MESH" or not o.data.vertices:
            continue
        co = np.empty(len(o.data.vertices) * 3, dtype=np.float64)
        o.data.vertices.foreach_get("co", co)
        m = np.array(o.matrix_world)
        points.append(co.reshape(-1, 3) @ m[:3, :3].T + m[:3, 3])
    p = np.concatenate(points)
    return Vector(p.min(axis=0)), Vector(p.max(axis=0))


def material(name, spec):
    """A material: "#rrggbb", or {"color": "#rrggbb", "rough": 0.65, "metal": 0.0, "glow": 0.0,
    "specular": 0.5}. Blender's Principled BSDF for the glTF file (the engine draws the base
    colour), the same values for Workbench's renders."""
    spec = {"color": spec} if isinstance(spec, str) else dict(spec)
    rgb = bf.hex_to_linear(spec.get("color", "#9a9a9a"))
    rough, metal, glow = float(spec.get("rough", 0.65)), float(spec.get("metal", 0.0)), float(spec.get("glow", 0.0))
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    bsdf.inputs["Specular"].default_value = float(spec.get("specular", 0.5))
    if glow > 0:
        bsdf.inputs["Emission"].default_value = (*rgb, 1.0)
        bsdf.inputs["Emission Strength"].default_value = glow
    mat.diffuse_color = (*rgb, 1.0)
    mat.roughness, mat.metallic = rough, metal
    return mat


GLOSSY = {"rough": 0.06, "specular": 0.9}


def _inside(tree, p):
    """Inside a closed volume, by the normal of its surface nearest the point."""
    hit = tree.find_nearest(p)
    return hit[0] is not None and (p - hit[0]).dot(hit[1]) < 0


def fuse(parts, name, colours=None, smooth=True, voxel=None, faces=None, cuts=(), paint=(), keep=False):
    """`parts` made one clean mesh named `name`: joined and voxel-remeshed (one watertight
    surface, even faces), `cuts` carved out (recessed windows, a mouth), decimated to about
    `faces` triangles (smooth: 6000, low-poly: 800). Each face takes the material of the colour
    region of the part or cut nearest it, then of a `paint` volume it's inside (markings,
    patches: shapes that only colour); stray faces take their neighbours' region. `colours`:
    region -> material (see `material`). Smooth or flat shaded. The parts, cuts and paint go
    (unless `keep`)."""
    colours = colours or {}
    sources = [(p, p["bf_region"]) for p in parts] + [(c, c["bf_region"]) for c in cuts]
    bms = [(_world_bmesh(o), region) for o, region in sources]
    lo, hi = bounds(parts)
    body = _link(name, _join([b for b, _ in bms[:len(parts)]]), "")
    size = max(hi - lo)
    _apply(body, "REMESH", mode="VOXEL", voxel_size=voxel or size / (110 if smooth else 30), adaptivity=0.0, use_smooth_shade=smooth)
    for c in cuts:
        c.hide_render = True
        _apply(body, "BOOLEAN", operation="DIFFERENCE", object=c, solver="EXACT")
    # the regions, on the dense mesh: the part (or cut) nearest each face, then the paint volumes
    regions = []
    for _, region in bms:
        if region not in regions:
            regions.append(region)
    trees = {}
    for region in regions:
        joined = _join([b for b, r in bms if r == region])
        trees[region] = BVHTree.FromBMesh(joined)
        joined.free()
    volumes = []
    for v in paint:
        vb = _world_bmesh(v)
        volumes.append((BVHTree.FromBMesh(vb), v["bf_region"]))
        vb.free()
        if v["bf_region"] not in regions:
            regions.append(v["bf_region"])
    for b, _ in bms:
        b.free()
    out = bmesh.new()
    out.from_mesh(body.data)

    def distance(region, c):
        hit = trees[region].find_nearest(c)
        return hit[3] if hit[0] is not None else 1e9

    for f in out.faces:
        c = f.calc_center_median()
        best = min(trees, key=lambda r: distance(r, c))
        for tree, region in volumes:
            if _inside(tree, c):
                best = region
        f.material_index = regions.index(best)
    # a face whose neighbours all have another region takes theirs
    for _ in range(2):
        changes = []
        for f in out.faces:
            around = [g.material_index for e in f.edges for g in e.link_faces if g is not f]
            if around and f.material_index not in around:
                changes.append((f, max(set(around), key=around.count)))
        for f, m in changes:
            f.material_index = m
    for f in out.faces:
        f.smooth = smooth
    # the materials first: emptying a mesh's materials resets its faces' indices
    body.data.materials.clear()
    for region in regions:
        body.data.materials.append(material(f"{name} {region}", colours.get(region, "#9a9a9a")))
    # where regions meet stays as fine as it is: decimating spares those vertices
    border = [v.index for v in out.verts if len({f.material_index for f in v.link_faces}) > 1]
    out.to_mesh(body.data)
    out.free()
    # the budget in triangles (what decimating counts, and what the engine draws)
    n, target = sum(len(p.vertices) - 2 for p in body.data.polygons), faces or (6000 if smooth else 800)
    if n > target:
        group = body.vertex_groups.new(name="decimate")
        group.add(range(len(body.data.vertices)), 1.0, "REPLACE")
        group.add(border, 0.0, "REPLACE")
        _apply(body, "DECIMATE", decimate_type="COLLAPSE", ratio=target / n, use_collapse_triangulate=True, vertex_group="decimate", vertex_group_factor=1.0)
        body.vertex_groups.remove(body.vertex_groups["decimate"])
        # long thin triangles where it's nearly flat shade in streaks: their edges turned
        # to make them even (only within a region, only where turning changes nothing)
        bm = bmesh.new()
        bm.from_mesh(body.data)
        flat = [e for e in bm.edges if len(e.link_faces) == 2 and e.link_faces[0].material_index == e.link_faces[1].material_index and e.calc_face_angle(1.0) < math.radians(6)]
        bmesh.ops.beautify_fill(bm, faces=bm.faces[:], edges=flat, method="ANGLE")
        bm.to_mesh(body.data)
        bm.free()
    if smooth:
        body.data.use_auto_smooth = True
        body.data.auto_smooth_angle = math.radians(60)
        # normals weighted by the faces' areas: flat parts shade flat, curves stay smooth
        _apply(body, "WEIGHTED_NORMAL", mode="FACE_AREA", weight=50, keep_sharp=True)
    if not keep:
        for o in list(parts) + list(cuts) + list(paint):
            me = o.data
            bpy.data.objects.remove(o, do_unlink=True)
            if me.users == 0:
                bpy.data.meshes.remove(me)
    apart = pieces(body)
    print(f"{name}: {len(body.data.polygons)} faces, regions {', '.join(regions)}"
          + (f"; IN {apart} PIECES: a part doesn't touch the rest (move it in, or give it more overlap)" if apart > 1 else ""), flush=True)
    return body


def pieces(body):
    """How many separate pieces a mesh is in (one, for a model: more means a part doesn't
    touch the rest)."""
    me = body.data
    parent = list(range(len(me.vertices)))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for e in me.edges:
        a, b = find(e.vertices[0]), find(e.vertices[1])
        if a != b:
            parent[a] = b
    return len({find(i) for i in range(len(me.vertices))})


def eyes(body, centres, radius, colour="#17181c", glint="#ffffff", look=(0, -1, 0.2), sink=0.4, segments=16):
    """Glossy eyes set into `body`: a dark ball at each centre and a small white glint on it,
    with materials of their own; their vertices are the group "eyes", which `rig` gives the
    head. Each eye is seated on the body's surface nearest its centre, `sink` of its radius
    into it (a centre can be rough: where on the face it goes)."""
    me = body.data
    surface = bmesh.new()
    surface.from_mesh(me)
    surface.transform(body.matrix_world)
    tree = BVHTree.FromBMesh(surface)
    surface.free()
    seated = []
    for c in map(Vector, centres):
        at, normal = tree.find_nearest(c)[:2]
        seated.append(c if at is None else at - normal * radius * sink)
    centres = [body.matrix_world.inverted() @ c for c in seated]
    me.materials.append(material(f"{body.name} eye", dict(GLOSSY, color=colour)))
    me.materials.append(material(f"{body.name} eye glint", dict(GLOSSY, color=glint, glow=0.4)))
    eye_i, glint_i = len(me.materials) - 2, len(me.materials) - 1
    look = Vector(look).normalized()
    side = look.cross(Vector((0, 0, 1))).normalized() if abs(look.z) < 0.99 else Vector((1, 0, 0))
    up = side.cross(look).normalized()
    bm = bmesh.new()
    bm.from_mesh(me)
    group = body.vertex_groups.get("eyes") or body.vertex_groups.new(name="eyes")
    deform = bm.verts.layers.deform.verify()
    for c in map(Vector, centres):
        for at, r, index, n in ((c, radius, eye_i, segments), (c + (look * 0.8 + up * 0.35 + side * 0.25).normalized() * radius * 0.86, radius * 0.27, glint_i, 8)):
            made = bmesh.ops.create_uvsphere(bm, u_segments=n, v_segments=max(4, n // 2), radius=r, matrix=Matrix.Translation(at))["verts"]
            for f in {f for v in made for f in v.link_faces}:
                f.material_index = index
                f.smooth = True
            for v in made:
                v[deform][group.index] = 1.0
    bm.to_mesh(me)
    bm.free()
    return body


def fur(body, region, count=1500, length=0.04, thickness=0.004, droop=0.3, seed=1, colour=None):
    """Fur (or hair) grown on a colour region of `body` as Blender's hair would grow it, but
    made mesh, since glTF (and the game) has no particle hair: `count` strands from the
    region's faces (spread by their area), about `length` cells long (each a little longer or
    shorter), `thickness` at the root tapering to a point, bending down by `droop`, in the
    region's material (or a material of `colour`). They're part of the body: grow fur once it's
    fitted (lengths in cells), then `fit` again (so it stands its height with its fur on), then
    `rig`, which weights the strands too. Returns how many strands grew."""
    import bisect
    import random
    me = body.data
    index = next((i for i, m in enumerate(me.materials) if m and m.name.endswith(f" {region}")), None)
    if index is None:
        raise ValueError(f"{body.name} has no region {region!r}: {[m.name for m in me.materials]}")
    bm = bmesh.new()
    bm.from_mesh(me)
    faces = [f for f in bm.faces if f.material_index == index]
    if not faces:
        bm.free()
        return 0
    if colour:
        me.materials.append(material(f"{body.name} {region} fur", colour))
        index = len(me.materials) - 1
    areas, total = [], 0.0
    for f in faces:
        total += f.calc_area()
        areas.append(total)
    rng = random.Random(seed)
    turns = [k / 3 * math.tau for k in range(3)]
    grown = []
    for _ in range(count):
        f = faces[min(bisect.bisect(areas, rng.random() * total), len(faces) - 1)]
        corners = [v.co for v in f.verts]
        i = rng.randrange(1, len(corners) - 1)
        a, b, c = corners[0], corners[i], corners[i + 1]
        u, w = rng.random(), rng.random()
        if u + w > 1:
            u, w = 1 - u, 1 - w
        root = a + (b - a) * u + (c - a) * w
        n = f.normal.normalized()
        long = length * (0.7 + 0.6 * rng.random())
        down = Vector((0.0, 0.0, -droop * long))
        mid, tip = root + n * long * 0.5 + down * 0.25, root + n * long + down
        side = n.orthogonal().normalized()
        other = n.cross(side)

        def ring(at, r):
            return [bm.verts.new(at + (side * math.cos(t) + other * math.sin(t)) * r) for t in turns]

        low, high, point = ring(root, thickness), ring(mid, thickness * 0.55), bm.verts.new(tip)
        for k in range(3):
            k1 = (k + 1) % 3
            for made in (bm.faces.new((low[k], low[k1], high[k1], high[k])), bm.faces.new((high[k], high[k1], point))):
                made.material_index = index
                made.smooth = True
                grown.append(made)
    # facing out, each strand
    bmesh.ops.recalc_face_normals(bm, faces=grown)
    bm.to_mesh(me)
    bm.free()
    print(f"{body.name}: {count} strands of fur on {region} ({len(me.polygons)} faces now)", flush=True)
    return count


def fit(objects, height):
    """Scaled as a whole to stand `height` cells tall, on the origin with the middle of its
    footprint there (the project's convention). Meshes made here stand at the origin, so their
    vertices move; lights and the rest move with them."""
    lo, hi = bounds(objects)
    s = height / max(hi.z - lo.z, 1e-6)
    m = Matrix.Scale(s, 4) @ Matrix.Translation(-Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z)))
    for o in objects:
        if o.type == "MESH":
            o.data.transform(o.matrix_world)
            o.matrix_world = Matrix.Identity(4)
            o.data.transform(m)
            o.data.update()
        else:
            o.location = m @ o.location
    return s


def light(at, color="#ffc46b", intensity=1.0, reach=6.0, kind="point", cone=100.0, aim=(0, 0, -1)):
    """A lamp's light on the model, which becomes the prop's `light` (lit at night): a point or
    a spot, 1000 W for an intensity of 1, `reach` in cells."""
    data = bpy.data.lights.new("light", "POINT" if kind == "point" else "SPOT")
    data.color = bf.hex_to_linear(color)
    data.energy = intensity * bf.WATTS_PER_INTENSITY
    data.use_custom_distance = True
    data.cutoff_distance = reach
    if kind != "point":
        data.spot_size = math.radians(cone)
    o = bpy.data.objects.new("light", data)
    o.location = at
    if kind != "point":
        o.rotation_euler = Vector(aim).to_track_quat("-Z", "Y").to_euler()
    bpy.context.collection.objects.link(o)
    return o


# ---------------------------------------------------------------- creatures: the rig and the idle

def _side(name):
    for s in (".L", ".R", ".l", ".r"):
        if name.endswith(s):
            return name[: -len(s)], s
    return name, ""


def _seg_dist(p, a, b):
    ab = b - a
    t = 0.0 if ab.length_squared < 1e-12 else max(0.0, min(1.0, (p - a).dot(ab) / ab.length_squared))
    return (p - (a + ab * t)).length


def rig(body, chains, parents=None, name="rig"):
    """An armature from `chains`: {"spine": [hips, chest], "head": [neck, top], "leg.L":
    [hip, knee, foot], "tail": [...]...}, each a run of connected bones through its points
    (in the model as it stands, after `fit`). The first chain is the root; each other one hangs
    from the bone nearest its first point, or from the chain (or bone) `parents` names for it.
    The body is skinned with automatic weights (bone heat); a vertex it misses goes to its
    nearest bone, the eyes to the head. Returns the armature."""
    arm = bpy.data.objects.new(name, bpy.data.armatures.new(name))
    bpy.context.collection.objects.link(arm)
    layer = bpy.context.view_layer
    for o in layer.objects:
        o.select_set(False)
    arm.select_set(True)
    layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    bones, ends = {}, {}
    for chain, points in chains.items():
        points = [Vector(p) for p in points]
        base, side = _side(chain)
        wanted = (parents or {}).get(chain)
        prev = ends.get(wanted, wanted) if wanted else (min(bones, key=lambda b: _seg_dist(points[0], *bones[b])) if bones else None)
        for k in range(len(points) - 1):
            bname = f"{base}{side}" if len(points) == 2 else f"{base}.{k + 1}{side}"
            b = arm.data.edit_bones.new(bname)
            b.head, b.tail = points[k], points[k + 1]
            if prev:
                b.parent = arm.data.edit_bones[prev]
                b.use_connect = k > 0
            bones[bname] = (points[k], points[k + 1])
            prev = bname
        ends[chain] = prev
    bpy.ops.object.mode_set(mode="OBJECT")
    for o in layer.objects:
        o.select_set(False)
    body.select_set(True)
    arm.select_set(True)
    layer.objects.active = arm
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    head = ends.get("head") or ends.get("neck")
    eye_group = body.vertex_groups.get("eyes")
    if head is None and eye_group is not None:
        pts = [v.co for v in body.data.vertices if any(g.group == eye_group.index for g in v.groups)]
        if pts:
            mid = sum(pts, Vector()) / len(pts)
            head = min(bones, key=lambda b: _seg_dist(mid, *bones[b]))
    groups = {g.index: g.name for g in body.vertex_groups}
    missed, eyes_moved = 0, 0
    for v in body.data.vertices:
        weighted = [groups[g.group] for g in v.groups if groups.get(g.group) in bones and g.weight > 1e-4]
        if eye_group is not None and head and any(g.group == eye_group.index for g in v.groups):
            for g in weighted:
                body.vertex_groups[g].remove([v.index])
            body.vertex_groups[head].add([v.index], 1.0, "REPLACE")
            eyes_moved += 1
        elif not weighted:
            body.vertex_groups[min(bones, key=lambda b: _seg_dist(v.co, *bones[b]))].add([v.index], 1.0, "REPLACE")
            missed += 1
    print(f"{name}: {len(bones)} bones ({', '.join(bones)}); {missed} vertices bone heat missed, given their nearest bone; {eyes_moved} eye vertices on {head}", flush=True)
    return arm


# what moves in the idle, by the bone's name: degrees about the bone's X, Y and Z, and when
# (a fraction of the loop) for each; `i` is the bone's place in its chain (1, 2...)
IDLE = (
    (("spine", "body", "chest", "torso", "hips", "belly"), lambda i: ((2.0, 0, 0), (0.0, 0, 0))),
    (("neck",), lambda i: ((2.5, 0, 0), (0.08, 0, 0))),
    (("head",), lambda i: ((4.0, 0, 3.0), (0.12, 0, 0.37))),
    (("tail",), lambda i: ((2.0, 0, 9.0 + 5.0 * i), (0.1 * i, 0, 0.12 * i))),
    (("ear", "antenna", "feeler", "leaf", "crest"), lambda i: ((6.0, 0, 3.0), (0.2 + 0.1 * i, 0, 0.45 + 0.1 * i))),
    (("arm", "hand", "fin", "flipper"), lambda i: ((4.0, 0, 0), (0.3 + 0.05 * i, 0, 0))),
    (("wing",), lambda i: ((0, 6.0, 4.0), (0, 0.25, 0.25))),
)


def idle(arm, seconds=2.0, fps=24, amount=1.0):
    """A looping "idle" action from the bones' names: the body breathes, the neck and head nod
    and look about, tails, ears and antennae sway with a lag down their chain, arms and fins
    swing a little, wings stir; legs and feet stay planted. `amount` scales it all."""
    scene = bpy.context.scene
    scene.render.fps = fps
    frames = max(2, round(seconds * fps))
    scene.frame_start, scene.frame_end = 1, frames + 1
    arm.animation_data_create()
    action = bpy.data.actions.new("idle")
    arm.animation_data.action = action
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    moving = []
    for pb in arm.pose.bones:
        base = _side(pb.name)[0].lower()
        words = base.split(".")
        i = int(words[-1]) if words[-1].isdigit() else 1
        rule = next((f for names, f in IDLE if words[0] in names), None)
        if rule is None:
            continue
        amp, phase = rule(i)
        pb.rotation_mode = "XYZ"
        for k in range(0, frames + 1, 2):
            t = k / frames
            pb.rotation_euler = tuple(math.radians(a * amount) * math.sin(math.tau * (t + p)) for a, p in zip(amp, phase))
            pb.keyframe_insert("rotation_euler", frame=k + 1)
        moving.append(pb.name)
    bpy.ops.object.mode_set(mode="OBJECT")
    print(f"idle: {seconds} s, moving {', '.join(moving) or 'nothing (no bone has a name it knows)'}", flush=True)
    return action


# ---------------------------------------------------------------- renders, to compare

def _view(scene, cam, lo, hi, direction, ortho):
    centre, dims = (lo + hi) / 2, hi - lo
    d = Vector(direction).normalized()
    if ortho:
        cam.data.type = "ORTHO"
        across = abs(d.x) * dims.y + abs(d.y) * dims.x
        cam.data.ortho_scale = max(across, dims.z) * 1.18
        cam.location = centre + d * (dims.length * 2 + 2)
    else:
        cam.data.type = "PERSP"
        cam.data.lens = 50
        fov = 2 * math.atan(18 / 50)
        cam.location = centre + d * (dims.length * 0.47 / math.tan(fov / 2))
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    cam.data.clip_start, cam.data.clip_end = 0.01, 1000


def _render(scene, path):
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


VIEWS = (("front", (0, -1, 0), True), ("side", (1, 0, 0), True), ("three_quarter", (0.62, -0.78, 0.38), False))


def previews(job, objects, size=640):
    """Workbench renders into the request's folder: front and side (straight on, like a
    turnaround sheet) and three-quarter; with a rig, preview_idle.png too: four moments of the
    idle side by side. Returns the files (the three views), for `compare` and `finish`."""
    job.step("rendering the previews", 0.5)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    shading = scene.display.shading
    shading.light = "STUDIO"
    shading.color_type = "MATERIAL"
    shading.show_specular_highlight = True
    shading.show_cavity = True
    shading.show_object_outline = True
    shading.show_shadows = False
    scene.display.render_aa = "8"
    scene.view_settings.view_transform = "Standard"
    if scene.world is None:
        scene.world = bpy.data.worlds.new("World")
    scene.world.color = (0.62, 0.62, 0.6)
    scene.render.film_transparent = False
    scene.render.resolution_x = scene.render.resolution_y = size
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGB"
    meshes = [o for o in objects if o.type == "MESH"]
    # the three views at rest; the rig is posed again after (the export samples the pose)
    arms = [o for o in objects if o.type == "ARMATURE"]
    for a in arms:
        a.data.pose_position = "REST"
    lo, hi = bounds(meshes)
    cam = bpy.data.objects.new("preview camera", bpy.data.cameras.new("preview camera"))
    bpy.context.collection.objects.link(cam)
    scene.camera = cam
    files = []
    for view, direction, ortho in VIEWS:
        _view(scene, cam, lo, hi, direction, ortho)
        _render(scene, job.dir / f"preview_{view}.png")
        files.append(f"preview_{view}.png")
    if arms and arms[0].animation_data and arms[0].animation_data.action:
        arms[0].data.pose_position = "POSE"
        _view(scene, cam, lo - Vector((0.1, 0.1, 0.0)) * job.height, hi + Vector((0.1, 0.1, 0.1)) * job.height, (0.62, -0.78, 0.38), False)
        scene.render.resolution_x = scene.render.resolution_y = size // 2
        moments = []
        for k in range(4):
            scene.frame_set(scene.frame_start + round(k / 4 * (scene.frame_end - scene.frame_start)))
            path = job.dir / f"idle_{k}.png"
            _render(scene, path)
            moments.append(path)
        scene.frame_set(scene.frame_start)
        _sheet(job.dir / "preview_idle.png", [moments], size // 2)
        for p in moments:
            p.unlink()
        scene.render.resolution_x = scene.render.resolution_y = size
    for a in arms:
        a.data.pose_position = "POSE"
    bpy.data.objects.remove(cam, do_unlink=True)
    print(f"rendered {', '.join(files)}" + (" and preview_idle.png" if arms else "") + f" in {job.dir}", flush=True)
    return files


def _pixels(path, height):
    """A picture's pixels (rows bottom up, as Blender keeps them), `height` high (narrower
    when it's very wide); None when Blender can't read it."""
    img = bpy.data.images.load(str(path), check_existing=False)
    w, h = img.size
    if w == 0 or h == 0:
        bpy.data.images.remove(img)
        return None
    s = min(height / h, 2.5 * height / w)
    nw, nh = max(1, round(w * s)), max(1, round(h * s))
    img.scale(nw, nh)
    px = np.empty(nw * nh * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    bpy.data.images.remove(img)
    return px.reshape(nh, nw, 4)


def _sheet(path, rows, height, gap=12):
    """Pictures in rows, top row first, on a light ground, saved as a PNG."""
    rows = [[p if isinstance(p, np.ndarray) else _pixels(p, height) for p in row] for row in rows]
    rows = [[p for p in row if p is not None] for row in rows]
    width = max(sum(p.shape[1] for p in row) + gap * (len(row) + 1) for row in rows)
    tall = len(rows) * (height + gap) + gap
    sheet = np.empty((tall, width, 4), dtype=np.float32)
    sheet[:] = (0.93, 0.92, 0.9, 1.0)
    for r, row in enumerate(rows):
        y = tall - (r + 1) * (height + gap)
        x = gap
        for p in row:
            h, w = p.shape[:2]
            sheet[y + (height - h):y + height, x:x + w] = p
            x += w + gap
    img = bpy.data.images.new(Path(path).stem, width, tall, alpha=True)
    img.pixels.foreach_set(sheet.ravel())
    img.filepath_raw = str(path)
    img.file_format = "PNG"
    img.save()
    bpy.data.images.remove(img)


def compare(job, files, height=300):
    """compare.png in the request's folder: the request's pictures on top, the renders under
    them, the same height. Look at it beside the pictures themselves."""
    job.step("comparing with the pictures", 0.6)
    path = job.dir / "compare.png"
    _sheet(path, [job.pictures(), [job.dir / f for f in files]], height)
    print(f"compare: {path}", flush=True)
    return path


# ---------------------------------------------------------------- into the project

def free_prop_id(root, base):
    pid, n = base, 2
    while any((Path(root) / space / "data" / "props" / f"{pid}.json").is_file() for space in ("content", "temporary")):
        pid, n = f"{base}_{n}", n + 1
    return pid


def engine_check(root, glb):
    """What the engine reads in the model (export-glb check), or why it couldn't be asked."""
    try:
        run = subprocess.run(["cargo", "run", "-q", "-p", "engine-world", "--bin", "export-glb", "--", "check", str(glb)], cwd=str(root), capture_output=True, text=True, timeout=900)
    except (OSError, subprocess.TimeoutExpired) as e:
        return f"(the engine's check didn't run: {e})"
    return (run.stdout or run.stderr).strip()


def finish(job, objects, files, reply=None, category=None, blocks=True, unstackable=False):
    """With `-- --finish`: the model added to the project through the add-on's write_prop,
    into temporary/ unless the request says the design is the user's (content/), as the prop
    the request already made, or one of its id (numbered when that's another prop's); the
    request done, with its renders, the prop and `reply`. Without it, only says how."""
    if not job.finishing:
        print("Rendered only: look at compare.png; run again with `-- --finish` to add it to the project.", flush=True)
        return None
    job.step("adding it to the project", 0.95)
    record = mr.load(job.root, job.id)
    space = "content" if record["mine"] else "temporary"
    pid = record["prop"] or free_prop_id(job.root, record["id"])
    facts = {"name": record["name"], "category": category or ("character" if job.creature else "decoration"), "blocks": blocks, "unstackable": unstackable}
    glb, path, _, notes = bf.write_prop(bpy.context, str(job.root), space, pid, [o for o in objects if not bf.is_bounds(o)], facts)
    other = "temporary" if space == "content" else "content"
    if (job.root / other / "data" / "props" / f"{pid}.json").is_file():
        notes.append(f"{other}/ has a {pid} too, from when the design was marked otherwise: delete it by hand if it shouldn't be there")
    check = engine_check(job.root, glb)
    mr.finish(job.root, job.id, pid, files, reply)
    print(f"Done: {pid} in {space}/ ({Path(glb).relative_to(job.root)}, {Path(path).relative_to(job.root)})" + (f"; {'; '.join(notes)}" if notes else ""), flush=True)
    print(f"The engine reads: {check}", flush=True)
    return glb
