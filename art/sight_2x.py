"""Builds the 2x sight: a short tube with a lens at each end and a red dot in a
ring on the front lens, on a clamp like the red dot's. Metres, Z up, the front
towards -Y, its base on the origin, like the other sights. Small enough to
suit the pistol too. Saves art/2x.blend and exports
assets/models/sights/2x.glb. Run with:
blender --background --factory-startup --python sight_2x.py -- <repo root>
"""
import math
import sys

import bmesh
import bpy
from mathutils import Matrix

ROOT = sys.argv[sys.argv.index("--") + 1]
# Nothing else in the file: not the default cube, camera and light.
bpy.ops.wm.read_factory_settings(use_empty=True)

# Through the middle of the tube, where the dot is.
AXIS_Z = 0.029
FRONT_LENS_Y = -0.024
REAR_LENS_Y = 0.024


def material(name, linear, alpha=1.0, roughness=0.5, emission=None, strength=1.0):
    """The same materials as the other sights (colours are linear here)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*linear, 1.0)
    bsdf.inputs["Metallic"].default_value = 0.0
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Alpha"].default_value = alpha
    if emission:
        bsdf.inputs["Emission Color"].default_value = (*emission, 1.0)
        bsdf.inputs["Emission Strength"].default_value = strength
    if alpha < 1.0:
        mat.surface_render_method = "BLENDED"
    mat.diffuse_color = (*linear, alpha)
    return mat


BODY = material("Sight_Body", (0.035, 0.036, 0.04))
DOT = material("Sight_Dot", (1.0, 0.04, 0.03), roughness=0.45, emission=(1.0, 0.04, 0.03), strength=2.0)
LENS = material("Sight_Lens", (0.55, 0.9, 0.8), alpha=0.16, roughness=0.05)

collection = bpy.data.collections.new("TwoX")
bpy.context.scene.collection.children.link(collection)


def finish(name, bm, mat):
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    collection.objects.link(obj)
    return obj


def box(name, x0, x1, y0, y1, z0, z1, mat=BODY):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2))
                          @ Matrix.Diagonal((x1 - x0, y1 - y0, z1 - z0, 1.0)))
    return finish(name, bm, mat)


def along_y(y0, y1):
    """Turns shapes made along Z to lie along Y, from y0 to y1, on the axis."""
    return Matrix.Translation((0, (y0 + y1) / 2, AXIS_Z)) @ Matrix.Rotation(math.pi / 2, 4, "X")


def on_circle(radius, angle, y):
    """A point `radius` from the axis at `y`, `angle` round from the right."""
    return (radius * math.cos(angle), y, AXIS_Z + radius * math.sin(angle))


def tube(name, outer, inner, y0, y1, segments=8):
    """An open tube along Y, so you can see through it. Eight flat sides, one on top."""
    bm = bmesh.new()
    angles = [(i + 0.5) * math.tau / segments for i in range(segments)]
    (outer_front, outer_back), (inner_front, inner_back) = [
        [[bm.verts.new(on_circle(radius, a, y)) for a in angles] for y in (y0, y1)] for radius in (outer, inner)
    ]
    for i in range(segments):
        j = (i + 1) % segments
        bm.faces.new((outer_front[i], outer_front[j], outer_back[j], outer_back[i]))
        bm.faces.new((inner_front[j], inner_front[i], inner_back[i], inner_back[j]))
        bm.faces.new((outer_front[j], outer_front[i], inner_front[i], inner_front[j]))
        bm.faces.new((outer_back[i], outer_back[j], inner_back[j], inner_back[i]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return finish(name, bm, BODY)


def disc(name, radius, y, mat, segments=24):
    bm = bmesh.new()
    bmesh.ops.create_circle(bm, cap_ends=True, segments=segments, radius=radius, matrix=along_y(y, y))
    return finish(name, bm, mat)


def ring(name, outer, inner, y, mat, segments=24):
    bm = bmesh.new()
    angles = [i * math.tau / segments for i in range(segments)]
    rims = [[bm.verts.new(on_circle(radius, a, y)) for a in angles] for radius in (outer, inner)]
    for i in range(segments):
        j = (i + 1) % segments
        bm.faces.new((rims[0][i], rims[0][j], rims[1][j], rims[1][i]))
    return finish(name, bm, mat)


def cylinder_x(name, radius, x0, x1, y, z):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=12, radius1=radius, radius2=radius, depth=x1 - x0,
                          matrix=Matrix.Translation(((x0 + x1) / 2, y, z)) @ Matrix.Rotation(math.pi / 2, 4, "Y"))
    return finish(name, bm, BODY)


# The clamp and the riser it stands on, like the red dot's.
box("Mount", -0.012, 0.012, -0.02, 0.02, 0.0, 0.008)
cylinder_x("ClampKnob", 0.0055, 0.012, 0.019, 0.004, 0.005)
box("Riser", -0.009, 0.009, -0.015, 0.015, 0.008, AXIS_Z - 0.012)
# The tube, a wider hood over the front lens, and an eyepiece at the back.
tube("Tube", 0.0145, 0.0122, -0.022, 0.022)
tube("Hood", 0.0158, 0.0125, -0.030, -0.022)
tube("Eyepiece", 0.0154, 0.0122, 0.022, 0.029)
disc("FrontLens", 0.0123, FRONT_LENS_Y, LENS)
disc("RearLens", 0.0123, REAR_LENS_Y, LENS)
# A dot in a thin ring, just behind the front lens.
dot_y = FRONT_LENS_Y + 0.0004
disc("Dot", 0.0007, dot_y, DOT, segments=12)
ring("Ring", 0.0042, 0.0036, dot_y, DOT)
# Adjustment turrets on top and on the right.
box("TurretTop", -0.006, 0.006, -0.007, 0.005, AXIS_Z + 0.013, AXIS_Z + 0.019)
box("TurretSide", 0.013, 0.0195, -0.007, 0.005, AXIS_Z - 0.006, AXIS_Z + 0.006)

aim = bpy.data.objects.new("AimPoint", None)
aim.empty_display_size = 0.005
aim.location = (0.0, dot_y, AXIS_Z)
collection.objects.link(aim)

bpy.ops.wm.save_as_mainfile(filepath=f"{ROOT}/art/2x.blend")
bpy.ops.export_scene.gltf(
    filepath=f"{ROOT}/assets/models/sights/2x.glb", export_format="GLB", export_yup=True, export_apply=True
)
