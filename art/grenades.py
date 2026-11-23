"""Builds the three grenade models (frag, flash, smoke) in metres, Z up, the
body centred on the origin. Saves art/<kind>.blend and exports
assets/models/grenades/<kind>.glb. Run with:
blender --background --factory-startup --python grenades.py -- <repo root> <preview png>
"""
import math
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

ROOT, PREVIEW = sys.argv[sys.argv.index("--") + 1:][:2]


def material(name, color, metallic=0.0, roughness=0.6, emission=None, strength=1.0):
    # Colours are given in sRGB, like the game's; Blender wants them linear.
    lin = lambda c: tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)
    color = lin(color)
    emission = emission and lin(emission)
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    if emission:
        bsdf.inputs["Emission Color"].default_value = (*emission, 1.0)
        bsdf.inputs["Emission Strength"].default_value = strength
    mat.diffuse_color = (*color, 1.0)
    return mat


def lathe(bm, profile, segments, mat, matrix=Matrix(), shade=None):
    """Spins (radius, z) points around Z. Radius 0 at either end closes it with
    a pole. Returns the side faces as rows[ring][segment]."""
    rings = []
    for r, z in profile:
        if r == 0.0:
            rings.append([bm.verts.new(matrix @ Vector((0, 0, z)))])
        else:
            rings.append([
                bm.verts.new(matrix @ Vector((r * math.cos(a), r * math.sin(a), z)))
                for a in (i * math.tau / segments for i in range(segments))
            ])
    rows = []
    for low, high in zip(rings, rings[1:]):
        row = []
        for i in range(segments):
            j = (i + 1) % segments
            if len(low) == 1:
                verts = (low[0], high[i], high[j])
            elif len(high) == 1:
                verts = (low[i], high[0], low[j])
            else:
                verts = (low[i], high[i], high[j], low[j])
            # Wound so normals point out (from the inside the order reverses).
            face = bm.faces.new(verts[::-1])
            face.material_index = mat
            row.append(face)
        rows.append(row)
    for ring, flip in ((rings[0], False), (rings[-1], True)):
        if len(ring) > 1:
            face = bm.faces.new(ring[::-1] if not flip else ring)
            face.material_index = mat
            rows.append([face])
    return rows


def cylinder(bm, radius, z0, z1, mat, segments=16, matrix=Matrix(), bevel=0.0):
    b = min(bevel, radius * 0.4, (z1 - z0) * 0.4)
    profile = [(radius - b, z0), (radius, z0 + b), (radius, z1 - b), (radius - b, z1)] if b else [(radius, z0), (radius, z1)]
    return lathe(bm, profile, segments, mat, matrix)


def torus(bm, major, minor, mat, matrix, segments=20, sides=8):
    grid = [
        [
            bm.verts.new(matrix @ Vector((
                (major + minor * math.cos(v)) * math.cos(u),
                (major + minor * math.cos(v)) * math.sin(u),
                minor * math.sin(v),
            )))
            for v in (k * math.tau / sides for k in range(sides))
        ]
        for u in (i * math.tau / segments for i in range(segments))
    ]
    for i in range(segments):
        for k in range(sides):
            a, b = grid[i], grid[(i + 1) % segments]
            face = bm.faces.new((a[k], b[k], b[(k + 1) % sides], a[(k + 1) % sides]))
            face.material_index = mat


def strip(bm, points, width, thickness, mat):
    """A flat bar along points in the XZ plane, `width` across in Y."""
    rows = []
    for i, p in enumerate(points):
        a = points[max(i - 1, 0)]
        b = points[min(i + 1, len(points) - 1)]
        tangent = (Vector(b) - Vector(a)).normalized()
        normal = Vector((tangent.y, -tangent.x)) * (thickness / 2)
        x, z = p
        rows.append([
            bm.verts.new((x + sx * normal.x, sy * width / 2, z + sx * normal.y))
            for sx, sy in ((1, -1), (1, 1), (-1, 1), (-1, -1))
        ])
    for low, high in zip(rows, rows[1:]):
        for k in range(4):
            face = bm.faces.new((low[k], low[(k + 1) % 4], high[(k + 1) % 4], high[k]))
            face.material_index = mat
    for ring, flip in ((rows[0], False), (rows[-1], True)):
        face = bm.faces.new(ring if not flip else ring[::-1])
        face.material_index = mat


def fuse(bm, top, body_r, metal, ring_mat, spoon_end, spoon_r):
    """The fuse on top of the body, its spoon down the +X side, the pin through
    it along Y and the pull ring hanging off the -Y side."""
    cylinder(bm, 0.0105, top - 0.004, top + 0.006, metal, 16, bevel=0.001)
    cylinder(bm, 0.0135, top + 0.006, top + 0.019, metal, 16, bevel=0.0015)
    cylinder(bm, 0.009, top + 0.019, top + 0.023, metal, 16, bevel=0.001)
    # The striker housing that the spoon hinges on.
    cylinder(bm, 0.004, -0.009, 0.009, metal, 10,
             Matrix.Translation((0.012, 0, top + 0.017)) @ Matrix.Rotation(math.pi / 2, 4, "X"))
    # The spoon: over the top of the fuse, then down the side of the body.
    path = [(0.004, top + 0.0215), (0.0135, top + 0.019), (0.0165, top + 0.012), (0.0165, top + 0.004)]
    steps = 10
    for i in range(1, steps + 1):
        z = top + 0.004 + (spoon_end - top - 0.004) * i / steps
        path.append((spoon_r(z) + 0.0022, z))
    strip(bm, path, 0.011, 0.0014, metal)
    # The pin, through the fuse along Y, and its ring.
    pin_z = top + 0.011
    cylinder(bm, 0.0012, -0.017, 0.017, ring_mat, 8,
             Matrix.Translation((0.004, 0, pin_z)) @ Matrix.Rotation(math.pi / 2, 4, "X"))
    torus(bm, 0.0115, 0.0013, ring_mat,
          Matrix.Translation((0.004, -0.017 - 0.0115, pin_z - 0.004))
          @ Matrix.Rotation(math.radians(70), 4, "Y") @ Matrix.Rotation(math.pi / 2, 4, "X"))


def finish(name, bm, mats):
    mesh = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()
    for mat in mats:
        mesh.materials.append(mat)
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.set_sharp_from_angle(angle=math.radians(35))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def frag():
    body = material("FragBody", (0.16, 0.24, 0.12), roughness=0.7, emission=(0.06, 0.01, 0.0))
    groove = material("FragGroove", (0.06, 0.09, 0.045), roughness=0.9)
    metal = material("FuseMetal", (0.22, 0.23, 0.22), metallic=0.85, roughness=0.4)
    ring = material("PinRing", (0.75, 0.72, 0.62), metallic=1.0, roughness=0.3, emission=(0.35, 0.04, 0.02), strength=1.0)
    bm = bmesh.new()
    a, c, rings = 0.031, 0.037, 8
    shape = lambda t: (a * math.sin(t), -c * math.cos(t))
    profile = [shape(math.pi * i / rings) for i in range(rings + 1)]
    profile[0] = (0.0, -c)
    profile[-1] = (0.0, c)
    rows = lathe(bm, profile, 10, 0)
    # Raised blocks: every quad of the body inset and pushed out, the gaps dark.
    quads = [f for row in rows[1:rings - 1] for f in row]
    bm.normal_update()
    out = bmesh.ops.inset_individual(bm, faces=quads, thickness=0.0022, depth=0.0028)
    for face in out["faces"]:
        face.material_index = 1
    top = c - 0.003
    spoon_r = lambda z: a * math.sqrt(max(0.0, 1 - (z / c) ** 2)) + 0.0024
    fuse(bm, top, a, 2, 3, -0.012, spoon_r)
    return finish("Frag", bm, [body, groove, metal, ring])


def can(name, body, band, hole, radius, half, hole_rows, band_rows, rows_count):
    """A can-shaped grenade: a bevelled cylinder with `rows_count` rows of side
    faces, holes punched in the rows `hole_rows` and a band on `band_rows`."""
    metal = material(name + "Fuse", (0.22, 0.23, 0.22), metallic=0.85, roughness=0.4)
    ring = material(name + "Ring", (0.75, 0.72, 0.62), metallic=1.0, roughness=0.3)
    bm = bmesh.new()
    bev = 0.003
    zs = [-half + bev + (2 * half - 2 * bev) * i / rows_count for i in range(rows_count + 1)]
    profile = [(0.0, -half), (radius - bev, -half), *[(radius, z) for z in zs], (radius - bev, half), (0.0, half)]
    segments = 18
    rows = lathe(bm, profile, segments, 0)
    side = rows[2:2 + rows_count]
    for r in band_rows:
        for face in side[r]:
            face.material_index = 1
    holes = [side[r][i] for r in hole_rows for i in range(segments) if i % 2 == r % 2]
    bm.normal_update()
    bmesh.ops.inset_individual(bm, faces=holes, thickness=radius * 0.13, depth=-0.0025)
    for face in holes:
        face.material_index = 2
    # A ring of vents round the top too.
    top_ring = rows[-2]
    vents = [f for i, f in enumerate(top_ring) if i % 3 == 0]
    bmesh.ops.inset_individual(bm, faces=vents, thickness=0.0015, depth=-0.0015)
    for face in vents:
        face.material_index = 2
    fuse(bm, half, radius, 3, 4, -half + 0.022, lambda z: radius)
    return finish(name, bm, [body, band, hole, metal, ring])


def flash():
    return can(
        "Flash",
        material("FlashBody", (0.72, 0.72, 0.66), metallic=0.3, roughness=0.5),
        material("FlashBand", (0.95, 0.75, 0.1), roughness=0.5, emission=(0.6, 0.45, 0.03), strength=1.5),
        material("FlashHole", (0.03, 0.03, 0.03), roughness=0.9),
        radius=0.026, half=0.05, hole_rows=[1, 2, 6, 7], band_rows=[4], rows_count=9,
    )


def smoke():
    return can(
        "Smoke",
        material("SmokeBody", (0.24, 0.27, 0.26), roughness=0.7),
        material("SmokeBand", (0.9, 0.9, 0.88), roughness=0.6, emission=(0.25, 0.25, 0.25), strength=1.0),
        material("SmokeHole", (0.03, 0.03, 0.03), roughness=0.9),
        radius=0.03, half=0.058, hole_rows=[1], band_rows=[5, 7], rows_count=9,
    )


built = {}
for kind, build in (("frag", frag), ("flash", flash), ("smoke", smoke)):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    obj = build()
    stats = (len(obj.data.vertices), len(obj.data.polygons), [round(d, 4) for d in obj.dimensions])
    bpy.ops.wm.save_as_mainfile(filepath=f"{ROOT}/art/{kind}.blend")
    bpy.ops.export_scene.gltf(
        filepath=f"{ROOT}/assets/models/grenades/{kind}.glb",
        export_format="GLB",
        export_yup=True,
        export_apply=True,
    )
    built[kind] = stats

# A preview of the three side by side.
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
for i, kind in enumerate(("frag", "flash", "smoke")):
    with bpy.data.libraries.load(f"{ROOT}/art/{kind}.blend") as (src, dst):
        dst.objects = list(src.objects)
    for obj in dst.objects:
        scene.collection.objects.link(obj)
        obj.location.x = (i - 1) * 0.1
        obj.rotation_euler.z = math.radians(-35)
cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
scene.collection.objects.link(cam)
cam.location = (0.0, -0.6, 0.16)
cam.rotation_euler = (math.radians(75), 0, 0)
cam.data.lens = 60
scene.camera = cam
for loc, energy in (((0.3, -0.4, 0.5), 30.0), ((-0.4, -0.2, 0.2), 10.0)):
    lamp = bpy.data.objects.new("Lamp", bpy.data.lights.new("Lamp", "POINT"))
    lamp.data.energy = energy
    lamp.location = loc
    scene.collection.objects.link(lamp)
world = bpy.data.worlds.new("World")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.25, 0.27, 0.3, 1)
scene.world = world
scene.render.engine = "BLENDER_EEVEE"
scene.view_settings.view_transform = "Standard"
scene.render.resolution_x, scene.render.resolution_y = 1200, 600
scene.render.filepath = PREVIEW
bpy.ops.render.render(write_still=True)
print("BUILT", built)

cam.location = (-0.1, -0.2, 0.03)
cam.rotation_euler = (math.radians(85), 0, 0)
scene.render.resolution_x = 600
scene.render.filepath = PREVIEW.replace(".png", "-frag.png")
bpy.ops.render.render(write_still=True)
