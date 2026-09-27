# soe-nif: Skyrim SE mesh reader

Crate `crates/nif`, library name `soe_nif`. Reads Gamebryo 20.2.0.7 files as
Bethesda writes them and hands a renderer a flat list of meshes. No
dependencies.

## What it reads

| Stream | Game | Coverage |
|---|---|---|
| 83 | Skyrim LE | NiTriShape, NiTriStrips, skin blocks, shader and alpha properties |
| 100 | Skyrim SE | Everything above plus BSTriShape and the SSE skin partition vertex buffer |
| 130 | Fallout 4 | BSTriShape geometry with half positions, texture set, lighting shader up to the specular fields, effect shader |

Anything else in a file (particles, controllers, extra data, Havok blocks)
is kept as raw bytes under `Block::Unknown` or `Block::Collision`, found by
the per-block size table in the header. That table is also the reader's
self-check: a block that parses to a different length than the header
records is reported as `Error::BlockSize` with the block index and type,
which is how a layout mistake shows up instead of silently misreading
everything after it.

Block families with trailing fields the mesh path does not need are read as
their base type and skipped to their recorded end: the NiNode family
(`BSFadeNode`, `BSMultiBoundNode`, `NiSwitchNode`, ...), the NiTriShape
family (`BSLODTriShape`) and the BSTriShape family (`BSDynamicTriShape`,
`BSSubIndexTriShape`). Every subtype in nif.xml appends its fields after the
base's, so the prefix read is exact.

## Layers

- `stream`: bounds-checked little-endian reads. Every count that sizes an
  allocation is checked against the bytes left first, so a hostile length
  field fails with `Error::CountTooLarge` instead of allocating.
- `header`: version numbers, the Bethesda export block, block types, block
  sizes, the string table.
- `blocks`: one struct per parsed block type, in the order nif.xml gives.
  Enum `Block` holds them in file order.
- `vertex`: the packed `BSVertexData` layout and its 64-bit descriptor.
- `controller`: `NiTransformController` and its interpolators, see below.
- `mesh`: the neutral output, see below, and the scene walk that produces
  it. Cycle protection and a depth limit make the walk safe on any input.

## The neutral mesh

`mesh::Mesh` is what the Godot host mirrors:

- `positions`, `normals`, `tangents` (`[x, y, z, handedness]`), `uvs`,
  `colors`, `indices` as a triangle list.
- `bone_weights` and `bone_indices`, four per vertex, indexing `skin.bones`.
- `skin`: skeleton root, per-bone name, node reference and inverse bind
  transform.
- `material`: shader kind, texture slots by Skyrim meaning (diffuse, normal,
  glow, height, environment, environment mask, inner layer, backlight), the
  flag-derived booleans a renderer acts on (double sided, vertex colors,
  vertex alpha, model-space normals, shadows, depth write and test, decal),
  blend factors and alpha test from NiAlphaProperty, and the per-shader-type
  tail such as the environment map scale.
- `transform`: world transform accumulated from the root through every
  parent node.

Conventions are the file's: right-handed, Z up, game units, DirectX UV
origin. Converting to a host's axes is the host's job and is documented at
the top of `mesh.rs`.

## Skinning

Two layouts exist and both are normalised to the per-vertex form above.

- Skyrim LE NiTriShape: weights and bone indices live in `NiSkinPartition`
  partitions with a vertex map, and bone indices there are partition-local,
  mapped through the partition's bone list to the skin instance's bones.
  When the partition carries no weights, `NiSkinData`'s per-bone lists are
  used.
- SSE BSTriShape: the shape's own data size is zero and the vertex buffer,
  weights included, lives in the `NiSkinPartition` global buffer. Bone
  indices must be mapped through each owning partition's bone list into
  the skin instance. The earlier claim that these indices were global was
  corrected by the actor-mesh survey recorded in `docs/status.md`: 564 of
  846 partitions across 400 meshes reorder the bone list. Use the
  `NiSkinPartition` bone tables declared in `nifxml/nif.xml`.

## FaceGen head morphs

`tri::parse_head_tri` reads FaceGen `FRTRI003` files. It retains the base
positions and named morph deltas. Each morph stores three signed 16-bit values
per vertex, multiplied by its file scale. The reader consumes topology, UVs
and modifier lists to reach the morph data, while retaining only positions and
morphs. The layout follows `TriHeadFile::Read` in the local
`BodySlide-and-Outfit-Studio/src/files/TriFile.cpp` reference. A PyNifly source
was not present in the local corpus. nif.xml's `NiMorphData` describes a
separate NIF animation format, not this FaceGen TRI file. Race `PHTN` and
`PHWT` are FaceFX phoneme name and weight records, not geometry morphs, as
shown by Mutagen's Skyrim `Race.xml` and xEdit's `wbPHWT` definition.

`HeadTri::apply_morph` adds a selected morph to caller supplied base positions.
It checks that the mesh and TRI vertex counts match, returns an exact copy at
zero weight, and does not mutate the caller's positions. Callers can evaluate
each frame from the same base to avoid accumulating deltas.

## Transform controllers (2026-09-24, in progress)

`controller` parses `NiTimeController` and the transform family
(`NiTransformController`, the deprecated `NiKeyframeController` name folds to
the same reader since nif.xml gives it no extra fields at our version),
`NiTransformInterpolator`, `NiTransformData` (`NiKeyframeData`), `NiPosData`
and `NiFloatData`. `NiPathInterpolator` is parsed too but its curve-follow
evaluation is not implemented, so a path-driven controller reads cleanly and
falls back to its interpolator default rather than moving along the path.

Two entry points cover evaluation. `evaluate_time` maps a game clock reading
through a controller's frequency, phase and start and stop time window,
honouring `CycleType`: clamp holds the edge pose, loop wraps the window, and
reverse ping-pongs across it. The mapping matches OpenMW's
`ControllerFunction::calculate`, read in `../engine-refs/openmw` before
writing it, since nif.xml has no field describing the wrap behaviour itself.
`evaluate_transform` takes a mapped time and returns the full transform for
that instant, blending rotation, translation and scale key tracks with a
cubic Hermite for quadratic keys and slerp for rotation, falling back per
channel to the interpolator's default value when a track has no keys.

Open items, read against `../engine-refs/openmw`'s `components/nif`:

- TBC (Kochanek-Bartels) tangent generation is not implemented. A
  Tbc-interpolated key falls back to linear interpolation between its
  neighbours instead of a true Kochanek-Bartels spline. TBC keys are rare on
  Skyrim SE statics, a legacy 3ds Max and Blender authoring feature, so this
  is scoped out rather than built and left unverified.
- `XyzRotation` tracks compose the three per-axis key groups in a fixed X
  then Y then Z order. nif.xml carries no field for axis composition order at
  our version, so this follows OpenMW's own fallback case rather than a
  documented source. Unverified against a real Skyrim SE static that actually
  uses Euler rather than quaternion keys.
- The `godot-host` side that calls `evaluate_transform` once per frame and
  applies the result to a scene node is not wired yet. Today the host bakes a
  static's transform into its vertex world space at streaming time, so an
  animated static needs its node kept separate from that bake before this
  evaluator has anywhere to write its output.

## Tests

`tests/common/mod.rs` is a NIF writer built from nif.xml independently of
the reader. `tests/synthetic.rs` writes the reference scene (a
BSMultiBoundNode over a rotated NiNode over a BSTriShape with lighting
shader, texture set, alpha property, a Havok block and an extra data block)
and pins every decoded value, then covers LE skinning through the partition
and through skin data, strip unrolling, the SSE partition vertex buffer,
effect shaders, Fallout 4 half positions, truncation at every third byte,
two thousand random bit flips, a hostile block count, a wrong block size, a
lying vertex descriptor, cycles and dangling references.

`tests/real_files.rs` runs over `$SKYRIM_DATA/meshes` and
`$SOE_NIF_SAMPLES` when set. Skyrim SE ships nearly every mesh inside
`Skyrim - Meshes0.bsa` and `Skyrim - Meshes1.bsa`, so the useful input is an
extracted folder, which the `bsa` crate will provide once it lands.

Run on 2026-09-06 against 320 files pulled from the SE install with LMO's
BSA extractor (150 spread across Meshes0, 150 across Meshes1, 20 from the
Fish Creation Club archive): 320 parsed with every block ending where the
header says, 313 yielded meshes, 541,750 triangles. The seven with no meshes
are particle effects, the camera rig and a collision-only placeholder, which
is correct. `examples/nif_dump.rs` prints this per file:

```text
cargo run -p soe-nif --example nif_dump -- --quiet meshes/**/*.nif
```

## Live run (2026-09-14)

The archived mesh test parses every `.nif` inside `Skyrim - Meshes0.bsa` and
`Skyrim - Meshes1.bsa`, 22047 meshes. It first ran with 52 failures, all of them
skeleton files failing on a `BSBoneLODExtraData` block. That type was wrongly
grouped into `NODE_FAMILY` in `blocks.rs`, so the reader walked its bone LOD list as
a node's child reference array and read garbage lengths. `BSBoneLODExtraData`
inherits `NiExtraData`, not `NiNode` (checked against `nif.xml`), so it belongs with
the opaque extra data blocks the reader already keeps as raw bytes. The fix drops it
from `NODE_FAMILY`, one line, and a regression test in `tests/synthetic.rs`,
`a_bs_bone_lod_extra_data_is_read_as_extra_data_not_a_node`, builds a synthetic bone
LOD payload and checks it comes back as `Block::Unknown` instead of being walked as
a node. After the fix the live run shows 22047 archived meshes in 3.7 seconds, 21588
with geometry, 42128269 triangles, 0 failures. The loose file test
(`real_files.rs`) finds no `.nif` under `Data/meshes` on this install, every mesh
ships packed in the two mesh archives, so that test prints its skip message here.
The archived mesh test is the one that exercises real content on a plain install.

## Residency estimate

`Mesh::approx_bytes()` sums each vertex's natural width, 12 bytes for
positions plus 12 for normals charged unconditionally, plus 16 for tangents,
8 for UVs, 16 for bone weights and 4 for bone indices when each is present,
and adds 4 bytes an index. Colors are never charged. Added for RND-03's
terrain LOD residency budget in `godot-host`, which needs a recorded byte
cost per loaded block to weigh against a budget.

Normals are charged whether or not the shape carries them: `godot-host`'s
`build_surface` generates a full normals buffer from a shape with none
whenever `generate_missing_normals` is on, its default, and a BTR shape with
no baked normals is the ordinary case for space-saving distant LOD geometry.
An estimate that only charged normals when present would read low against
every such block once built. Colors are dropped instead: `godot-host`'s
`MeshSource`, `MeshData` and `SurfaceArrays` carry no color field at all, so
a shape's vertex colors never reach a built surface and charging for them
would count bytes nothing allocates. It is a rough estimate for weighing a
mesh against a residency budget, not a claim about any one renderer's exact
buffer layout, an upload can pad, align or drop an unused attribute
differently.

## Not yet

- DDS loading (round 2).
- Collision shapes are opaque bytes, the physics layer decodes them.
- Fallout 4 lighting shader fields past specular strength, and Fallout 76
  entirely.
- Controllers, particles, `BSLODTriShape` levels, `BSDynamicTriShape`
  dynamic positions, multiple UV sets beyond the first in the mesh output
  (the block layer keeps them all).
