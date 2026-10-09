# soe-nif: Skyrim SE mesh reader

Crate `crates/nif`, library name `soe_nif`. Reads Gamebryo 20.2.0.7 files as
Bethesda writes them and hands a renderer a flat list of meshes. The `gltf`
dependency handles authoring glTF buffers and accessors for static mesh import.

## What it reads

| Stream | Game | Coverage |
|---|---|---|
| 83 | Skyrim LE | NiTriShape, NiTriStrips, skin blocks, shader and alpha properties |
| 100 | Skyrim SE | Everything above plus BSTriShape and the SSE skin partition vertex buffer |
| 130 | Fallout 4 | BSTriShape geometry with half positions, texture set, lighting shader up to the specular fields, effect shader |

Anything else in a file (unsupported particle modifiers, extra data, Havok blocks)
is kept as raw bytes under `Block::Unknown` or `Block::Collision`, found by
the per-block size table in the header. That table is also the reader's
self-check: a block that parses to a different length than the header
records is reported as `Error::BlockSize` with the block index and type,
which is how a layout mistake shows up instead of silently misreading
everything after it.

Block families with trailing fields the mesh path does not need are read as
their base type and skipped to their recorded end: the NiNode family
(`BSFadeNode`, `BSMultiBoundNode`, `NiSwitchNode`, ...) and the BSTriShape
family (`BSSubIndexTriShape`). `BSDynamicTriShape` is read as a `BsTriShape`
and exposes its full precision dynamic positions as `dynamic_positions`, one
`[f32; 4]` per vertex. Its dynamic data byte size must equal 16 times the
shape's vertex count. This follows `BSDynamicTriShape` in
`../engine-refs/nifxml/nif.xml` and nifly's `BSDynamicTriShape::Sync` in
`../engine-refs/nifly/src/Geometry.cpp`. `BSLODTriShape` is read as a
`TriShape` and also exposes its three trailing LOD size values as
`lod_sizes: Some([lod0, lod1, lod2])`. Each value remains a `u32` in the order
declared by nif.xml. Ordinary `NiTriShape` and `NiTriStrips` blocks use
`lod_sizes: None`. The field layout follows
`../engine-refs/nifxml/nif.xml` and nifly's `BSLODTriShape::Sync` in
`../engine-refs/nifly/src/Geometry.cpp`.

`BSSegmentedTriShape` is read through its `NiTriShape` geometry prefix. Its
segment table is skipped with the trailing fields, while the mesh output keeps
the complete triangle list and shape name. Object LOD rendering currently
draws that full list for each selected BTO block.

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
- `particle`: SSE volume emitter rate and lifetime decoding, plus bounded
  particle birth and death timing.
- `mesh`: the neutral output, see below, and the scene walk that produces
  it. Cycle protection and a depth limit make the walk safe on any input.
- `serialize`: writes Skyrim SE files containing `NiNode` blocks and opaque
  collision blocks, including the header tables, block sizes and root footer.
  It rejects other typed block kinds and strings outside Windows-1252.

The byte writer is scoped to the collision graph produced by `soe-physics`.
It is not a general writer for parsed meshes. Its node fields follow the
`NiNode` and `NiAVObject` layouts in
`../engine-refs/nifxml/nif.xml`, and its NIF header follows
`tests/common/mod.rs`, which was written independently from that schema.
The EDT-17 shape fields were checked against nifly's
`src/bhk.cpp::bhkConvexVerticesShape::Sync` and TES5Edit's
`Core/wbDataFormatNif.pas` definitions. PyNifly was named as another source
for this row but is not present in the local reference corpus.

## The neutral mesh

`mesh::Mesh` is what the Godot host mirrors:

- `positions`, `normals`, `tangents` (`[x, y, z, handedness]`), `uv_sets`,
  `colors`, `indices` as a triangle list. `uv_sets` contains one
  vertex-aligned coordinate array per source set. `uvs` remains an alias for
  the first set for callers that only use one. `BSTriShape` stores one set in
  its packed vertex stream. `NiGeometryData` stores the UV set count in its
  data flags where its stream version supports multiple sets, followed by one
  coordinate array per set. This follows the layouts in
  `../engine-refs/nifxml/nif.xml` and the corresponding `uvSets` handling in
  `../engine-refs/nifly/src/Geometry.cpp`. For the supported Skyrim 20.2
  streams, BS Data Flags bit 0 marks UV presence, so serialized geometry has
  at most one UV set. The FMT-06 mesh-walker test adds a second set to parsed
  geometry data to verify that the neutral mesh output preserves every set.
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

## Static glTF import and Skyrim SE export

`static_mesh::import_gltf` reads the default scene, or the first scene when no
default is declared. It imports unskinned, non-morph triangle primitives with
only `POSITION`, `NORMAL` and `TEXCOORD_0` attributes, bakes node transforms
into positions and normals, and makes area weighted normals when the source
has none. External buffer and image URIs become file paths. Diffuse, normal,
emissive and metallic-roughness textures map to Skyrim slots 0, 1, 2 and 7.
Embedded buffer URIs are rejected. Embedded image data is rejected when a
material references it because it has no external path to preserve.

`static_mesh::export_nif` writes Skyrim SE stream 100 files with a root
`NiNode`, one `BSTriShape` per mesh, `BSLightingShaderProperty` and
`BSShaderTextureSet` links. It preserves vertex positions, normals, UV0,
triangle indices and material texture paths. It rejects skinning, malformed
attributes, non-finite values, empty geometry, and geometry that exceeds the
16-bit vertex or index limits. It does not export collision or animation.

The writer follows the static-shape and shader block layouts in
`nifly/include/Geometry.hpp`, `nifly/include/Shaders.hpp` and the local
independent test writer in `tests/common/mod.rs`. PyNifly was not cloned into
the local reference corpus. Its upstream import/export options and project
overview describe a wider Blender workflow with game-specific settings,
skinning and collision, so this writer implements only the editor row's static
geometry and texture links.

## Skinning

Two layouts exist and both are normalised to the per-vertex form above.

`NiSkinInstance` and `BSDismemberSkinInstance` store their bone-reference count
as a `u32`, followed by that many references. Skyrim skeleton files also use
`0xFFFF_FFFF` as an empty-array sentinel, which the reader maps to no bones.
The field order follows `NiSkinInstance` in
`../engine-refs/nifxml/nif.xml`. Nifly's `NiSkinInstance::Sync` in
`../engine-refs/nifly/src/Skin.cpp` confirms the bone references are one
counted array. Neither source specifies the sentinel, so the explicit empty
array behavior follows the vanilla skeleton files covered by
`fmt_11_all_vanilla_skeleton_meshes_parse`.

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
and modifier lists to reach the morph data, while retaining positions and
morphs. The layout follows `TriHeadFile::Read` in the local
`BodySlide-and-Outfit-Studio/src/files/TriFile.cpp` reference. A PyNifly source
was not present in the local corpus. nif.xml's `NiMorphData` describes a
separate NIF animation format, not this FaceGen TRI file. Race `PHTN` and
`PHWT` are FaceFX phoneme name and weight records, not geometry morphs, as
shown by Mutagen's Skyrim `Race.xml` and xEdit's `wbPHWT` definition.

`HeadTri::apply_morph` adds one selected morph to caller supplied base
positions. `apply_morphs` adds several named morphs in file vertex order.
Both check that the mesh and TRI vertex counts match, preserve exact positions
at zero weight, and leave the input unchanged. `apply_to_mesh` uses those
operations on a resolved head-part mesh, checks its base positions against the
TRI within 0.0001 game units, and clears its stale normals and tangents when
any morph changes geometry. The NIF topology, UVs, material and transform
remain attached to the output mesh. Resolving NPC `PNAM` links and mapping its
`NAM9` values to named TRI morphs remains caller work.

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

Controller details, read against `../engine-refs/openmw`'s `components/nif`:

- TBC float and vector tracks generate Kochanek-Bartels tangents using
  tension, bias and continuity and their neighbouring keys. Quadratic and
  TBC tracks use cubic Hermite interpolation. Quaternion tracks use slerp,
  matching OpenMW's quaternion evaluator.
- `XyzRotation` tracks compose the three per-axis key groups in a fixed X
  then Y then Z order. nif.xml carries no field for axis composition order at
  our version, so this follows OpenMW's own fallback case rather than a
  documented source. Unverified against a real Skyrim SE static that actually
  uses Euler rather than quaternion keys.
- `godot-host::AnimatedStaticNode` resolves a controller, builds child meshes
  outside the cell batch, and applies `evaluate_transform` on each frame. It
  can also create a trimesh collider under each child mesh so the collider
  follows its animated transform. Ordinary game door activation does not yet
  instantiate this node or start its controller.

## Material color controller evaluation (RND-13)

`controller::evaluate_material_color` samples a `NiMaterialColorController`
color key group using the shared time-controller clock and updates exactly one
of the ambient, diffuse, specular or self-illumination channels in
`MaterialColors`. The track evaluator leaves the channels unchanged when it
has no keys, while `NifFile::apply_material_color_controller` uses the
interpolator's default value when its track is absent or empty.
`MaterialColorTarget` uses the four values declared by `MaterialColor` in
`nifxml/nif.xml`. The sampling and cycle mapping use the existing track and
clock evaluators grounded on OpenMW's `components/nifosg/controller.cpp`.
The block reader decodes `NiMaterialColorController` and
`NiPoint3Interpolator`, and `NifFile::apply_material_color_controller`
resolves their `NiPosData` track and applies a sampled value or interpolator
default to the selected channel.

The Godot host does not yet call this evaluator or apply the result to a live
material. Host cell rendering continues to build static materials from mesh
properties.

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
`tests/fmt_08_bs_lod_tri_shape.rs` checks the three BSLODTriShape LOD size
values, including values above `u16`, and verifies ordinary NiTriShape blocks
do not report LOD sizes.
`tests/fmt_11_skeleton_sentinel.rs` checks the empty-bones sentinel for both
skin-instance block types and parses all skeleton NIFs in the installed
vanilla mesh archives when `SKYRIM_DATA` is set.
`tests/fmt_19_dynamic_tri_shape.rs` checks dynamic position values, vertex
count validation and truncated dynamic position arrays.

`fuzz/fuzz_targets/fuzz_nif_grammar.rs` uses a bounded stream-100 grammar
from `fuzz/src/nif_grammar.rs`. It writes a complete header, type and size
tables, string table, root reference, `NiNode`, and `BSTriShape` with three
packed vertices and one triangle. The generated fields vary vertex positions,
UVs, normals, colors, and node flags. The grammar follows nifxml's local
`nif.xml` block definitions and expects parsing and mesh extraction to accept
every generated file. `tst_07` tests assert the generated block decoders and
mesh path are reached.

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

`static_mesh::export_nif` writes shader type 18 for a mesh whose material
kind is `LodLandscapeNoise`, and type 9 for `LodLandscape`. Other supported
static meshes retain type 0. This lets `godot-host` write terrain LOD BTR
geometry that the existing NIF reader can parse.

`Mesh::approx_bytes()` sums each vertex's natural width, 12 bytes for
positions plus 12 for normals charged unconditionally, plus 16 for tangents,
8 for each UV set, or one set when only the legacy `uvs` field is filled, 16
for bone weights and 4 for bone indices when each is present, and adds 4 bytes
an index. Effect meshes charge 16 color bytes per vertex. Other materials omit colors.
Added for RND-03's
terrain LOD residency budget in `godot-host`, which needs a recorded byte
cost per loaded block to weigh against a budget.

Normals are charged whether or not the shape carries them: `godot-host`'s
`build_surface` generates a full normals buffer from a shape with none
whenever `generate_missing_normals` is on, its default, and a BTR shape with
no baked normals is the ordinary case for space-saving distant LOD geometry.
An estimate that only charged normals when present would read low against
every such block once built. Effect colors are charged because the host now
uploads their vertex color and alpha, supplying white colors when absent.
Other material kinds omit color uploads. This remains a rough residency
estimate rather than a renderer's exact buffer layout.

## Particle emitters (RND-14)

`NifFile::particle_emitters` reads SSE `NiParticleSystem` blocks, named
`NiPSysSphereEmitter`, `NiPSysBoxEmitter` and `NiPSysCylinderEmitter`
modifiers, and `NiPSysEmitterCtlr` birth rates through `NiFloatInterpolator`
or `NiFloatData`. It matches a controller to the system target and modifier
name. The system's `NiPSysData` maximum caps live particles at 8192. The
emitter's active flag and controller active flag suppress births. A
`NiBoolInterpolator` visibility value or `NiBoolData` constant key track
also switches births.

`particle::Timeline` accumulates births at the sampled rate, assigns a
deterministic value within the authored lifespan variation, removes each
particle when that lifespan ends, and exposes live positions in NIF units.
The named synthetic `RND14SmokeEmitter` fixture in
`rnd_14_particle_emitter_check.rs` asserts four births per second, half
second expiry and the system's authored maximum. The parser layout follows
`nifxml/nif.xml` and nifly's `Particles.cpp`. OpenMW's
`components/nifosg/particle.cpp` supplied the lifetime variation and quota
behavior.

The timeline places particles at the system translation. The host exposes
the timeline through `ParticleEmitterNode` for a named model path.

## Not yet

- DDS loading (round 2).
- Collision shapes are opaque bytes, the physics layer decodes them.
- Fallout 4 lighting shader fields past specular strength, and Fallout 76
  entirely.
- Additional controllers and particle modifiers, `BSLODTriShape` levels,
  `BSDynamicTriShape` dynamic positions, and multiple UV sets beyond the first
  in the host renderer. The block layer keeps all UV sets in the neutral mesh.

## Shared logging, TOOL-06

`parse` emits a trace event with the input byte count and system through
`soe-trace`. Release builds remove the event and its arguments. The caller
installs a subscriber. Filter directives, rate limiting and output follow
[trace.md](trace.md).

## Effect material fields, RND-22

Effect mesh materials expose BSEffectShaderProperty base RGB as emissive color,
base color scale as its multiple, and base alpha as material opacity. The
existing effect parameter block retains the same values along with falloff
cosines, opacities, soft depth and palette path. It also carries texture clamp
mode, whose bit 1 wraps S and bit 0 wraps T. NiAlphaProperty supplies the
source and destination blend factors and optional alpha test independently.
These meanings follow nifly's `src/Shaders.cpp` and `include/Shaders.hpp`,
OpenMW's `components/nif/property.hpp`, and `nifxml/nif.xml`.

`rnd_22_effect_material` tests decode an independent effect fixture and survey
installed mist, window glow and light beam meshes through `SKYRIM_DATA`.

## Object animation playback, RND-23

`object_animation::ObjectAnimation::from_file` resolves active
`NiControllerManager` sequence links, their named geometry or property targets,
and autonomous transform and shader controllers. The additional animation
blocks retain their bytes in `Block::Unknown` and are decoded on demand.
Sequence and controller lengths, counts, string indices and executable links
are checked before sampling. Hierarchy traversal rejects cycles and dangling
scene links. The decoder accepts all 22,047 NIFs in the two vanilla mesh
archives in the installed-data test.

Every manager selects Default, Idle or Loop by name, then its first looping
sequence. `sample` uses the selected sequence's frequency, start time and
cycle mode. Named sequences use case-insensitive lookup and replace idle
sampling. Manager-controlled tracks read their interpolators from sequence
links rather than the controller's blend-interpolator link. Targets below
animated parents inherit the sampled parent pose. Invalid interpolator
channels marked with negative FLT_MAX retain the authored transform channel.
Autonomous controllers retain their frequency, phase, cycle and backwards
playback flags.

The property sampler supports alpha, emissive color and strength, UV offset,
scale and rotation, and effect-shader falloff values. It reads effect and
lighting shader float and color controller layouts, NiAlphaController and
NiTextureTransformController. NiMultiTargetTransformController target arrays
and BSLagBoneController's three scalar fields are length checked. Cumulative
looping sequences carry completed root-transform deltas into
subsequent loops. Lag-bone procedural displacement is not applied by this
sampler. Unsupported interpolator types remain opaque.

Layouts and variable codes follow local nifly `include/Animation.hpp`,
`src/Animation.cpp` and nifxml `nif.xml`. Time mapping and interpolation follow
OpenMW `components/nifosg/controller.cpp` and `components/nif/nifkey.hpp`.
These C++ implementations are references for the existing Rust key tracks and
mesh output, rather than linked dependencies. `rnd_23_object_animation`
checks autonomous and manager playback, cycles, activation sequence names,
interpolation, invalid defaults, shader channels and malformed sequence data.
The installed survey samples banners, candles, windmills, water wheels,
chests and levers.
