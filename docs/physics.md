# soe-physics: Havok shapes from meshes, and a static collision world

Crate `crates/physics`, library `soe_physics`. Depends on `soe-nif` only,
`soe-bsa` for the gated tests.

## Two worlds, and why

The engine links no physics library of its own, and it never will. Godot bundles
Jolt as a module from 4.4 on, selectable with the `physics/3d/physics_engine`
project setting, which `godot/project.godot` now sets to `Jolt Physics`. So the
rendered path drives Godot's server and gets Jolt for free, the way the mesh
path drives `RenderingServer` rather than shipping a renderer. Decided by the
user on 2026-09-18: "if godot has jolt its best to use their tooling".

Two implementations remain, behind one trait, and a caller picks by what it is:

| | `SimpleWorld`, this crate | Godot's server, `crates/godot-host/src/godot_physics.rs` |
|---|---|---|
| Who uses it | The simulation, the determinism tests, `soe-bench`, continuous integration | The rendered path, the player, anything the viewer touches |
| Needs | Nothing, it is plain Rust | Godot running, a `World3D` with a space |
| Solver | Statics, one capsule, rays | Jolt: rigid bodies, constraints, ragdolls, the lot |
| Reproducible from a seed | Yes, and the per tick state hash covers it | No, see below |

**Why the simulation cannot simply live in Godot.** The engine treats
determinism as a property to keep: `crates/game/src/determinism.rs` hashes the
world, the VM and the clock every tick, and a gated test runs 600 ticks of the
installed game twice and requires an identical hash stream. A simulation inside
Godot's server breaks all three legs of that. Its solver state is not ours to
hash. Its stepping is tied to Godot's frame loop rather than to our fixed
timestep. And it cannot run at all in continuous integration or in `soe-bench`,
neither of which has Godot. Jolt is deterministic in the sense its own
documentation claims, same binary and same inputs giving the same result, which
is not the property being asked for here. So the authority stays in our process
and Godot's bodies are what the player collides with, which is the same split
the renderer already has: our data, their device.

**What linking our own would have cost.** `joltc-sys` 0.3.1 (Jolt 5.0.0, May
2024) builds JoltC through the `cmake` crate, and this machine has a C and C++
toolchain but no CMake, so it cannot build here today. `jolt-rust` is not
published. Beyond that it would put a C++ build on every build machine, tie the
engine to Jolt's ABI across releases, and keep a second copy of the same solver
Godot already ships. Against a standing rule to bound dependency scope, that is
a clear loss, and nothing in round 6 needs a feature `SimpleWorld` lacks.

**What Godot's server gives a character controller**, checked through
godot-rust 0.5.5 against the api-4-4 floor: `body_create`, `body_set_mode`,
`body_set_space` and `body_add_shape` build the static world by resource id with
no node per body, `concave_polygon_shape_create` takes triangle soup through
`shape_set_data`, and `body_test_motion` is the sweep a kinematic character
moves on. The `controller` item of round 6 is what will use the last of those.

## Layers

- `bhk`: decodes `Block::Collision` payloads by type name. Layouts are
  nif.xml for stream 20.2.0.7 at Bethesda version 83 and 100, cross-read
  against OpenMW's `physics.cpp`. Fallout 4 (130) is refused as
  `Unsupported`. Covered: `bhkCollisionObject` and its P, SP and Blend
  variants, `bhkRigidBody`, `bhkRigidBodyT`, `bhkSimpleShapePhantom`, box, sphere, capsule, cylinder,
  convex vertices, `bhkTransformShape` and `bhkConvexTransformShape`,
  `bhkListShape`, `bhkMoppBvTreeShape` (the MOPP tree itself is Havok's
  closed code and is skipped, the wrapped shape is used),
  `bhkCompressedMeshShape` and its data with chunks, strips, big
  triangles and per chunk transforms, and `bhkNiTriStripsShape` with its
  referenced `NiTriStripsData` geometry. Everything else comes back as
  `Bhk::Other(name)`.
- `shape`: walks the scene graph the way `soe_nif::mesh` does, resolves
  each collision object to a `Collider` with a world `Iso` transform and a
  `Shape` (box, sphere, capsule, convex hull, triangle mesh, compound),
  converted from Havok metres to game units at 69.99125 units per metre.
  The shell radius on `bhkBoxShape` expands each half extent. On
  `bhkConvexVerticesShape`, the converter samples 26 axis and diagonal
  support directions to approximate the rounded shell. Axis bounds retain
  the authored radius, so the common 0.05 metre radius
  adds 3.49956 game units to the surface. Non-finite and negative convex
  radii are rejected during block decoding.
  A `bhkRigidBodyT`'s own translation and rotation are applied, a plain
  `bhkRigidBody`'s are ignored, as the game does.
- `hull`: faces of a convex point set, brute force, for the convex
  vertices shapes (nif.xml stores their half spaces but not their faces).
- `world`: `PhysicsWorld` and `SimpleWorld`.

## What the shipped files taught the crate

Two things nif.xml and OpenMW disagree on or leave open were settled
against the archives.

- The rigid body prefix before the translation is 52 bytes (world object
  28, entity info 4, the 2010 rigid body info's own 20), as nif.xml has
  it. OpenMW reads 48. All 361 `bhkRigidBodyT` bodies in the first 2,000
  meshes decode to unit quaternions at 52 and to garbage at 48.
- Vanilla statics are not `MO_SYS_FIXED`. A farmhouse or a chest carries
  motion system 5 (`MO_SYS_BOX_STABILIZED`) with mass zero, and mass
  zero is what Havok treats as immovable. `Collider::is_static` therefore
  tests mass first and the motion type second.

### `bhkNiTriStripsShape` collision meshes (FMT-09)

The decoder reads the material, radius, grow-by value, versioned XYZ scale,
strip-data references and one Havok filter per reference. It rejects invalid
radii, non-finite scales and mismatched filter counts. The referenced blocks
must be `NiTriStripsData`, represented by `soe-nif` as `GeometryData` with
triangle strips already unrolled. Their vertices are scaled, their triangles
are appended with adjusted indices, and each triangle inherits the shape's
Havok material. A missing or wrong-type geometry reference is reported in
`Collected::skipped`. The resulting `TriangleMesh` is the triangle input used
by the host's concave shape path.

The layout and scale version gate follow `engine-refs/nifxml/nif.xml` for
`bhkNiTriStripsShape`. OpenMW's `components/nif/physics.cpp` cross-checks the
field order and filter list. The NIF reader applies alternating triangle
winding and drops degenerate triangles. Jolt's `MeshShape` accepts triangle
geometry, which matches the neutral triangle-mesh representation; this
decoder does not add a separate Jolt dependency. The synthetic
`fmt_09_strips_fixture_preserves_collision_triangles_and_winding` test checks
the output triangles, material and scale.

### Convex shell radius on boxes and hulls (FMT-10)

`bhkConvexShape` supplies the shell radius used to form the collision surface.
The converter adds this radius to each box half extent and samples 26
normalized axis and diagonal support offsets on convex hull vertices. The
sampled hull keeps the exact authored offset on axis bounds and approximates
the rounded edges and corners. At 69.99125 game units per metre, the common
0.05 metre radius adds 3.49956 units. Synthetic checks verify that boxes and
hulls reach that surface and that non-finite or negative radii fail decoding.

The field layout and shell meaning come from `engine-refs/nifxml/nif.xml`,
which defines the convex radius as a thin shell and the convex vertex normals
as outward half spaces. The local Jolt `BoxShape.h` and `ConvexHullShape.h`
explain that their input size includes convex radius, which they subtract
internally to retain the total outer size. This crate keeps its current
renderer-neutral `Shape` API and converts the NIF core directly to its outer
surface, rather than adding a Jolt dependency.

### `bhkCylinderShape` collision geometry (FMT-20)

The decoder reads the convex material and shell radius, the two Vector4 axis
endpoints, and the cylinder radius from `bhkCylinderShape`. It rejects
non-finite endpoints or radii, a non-positive cylinder radius, a zero-length
axis and an outer radius that overflows. Shape collection adds the convex shell
radius to the cylinder radius and samples 32 points around each endpoint to
produce a convex hull in game units. The sample points retain the endpoint
planes and outer radius while fitting the existing neutral shape model used by
both physics paths.

The field order and the shell field come from
`engine-refs/nifxml/nif.xml`. Jolt's local `CylinderShape.h` and
`CylinderShape.cpp` describe a centered cylinder with two half-height ends and
a separately specified convex radius. The neutral `Shape` API has no
finite-cylinder variant, so this crate samples the end circles into the
existing `ConvexHull` variant rather than adding a Jolt-specific shape or
dependency. `fmt_20_cylinder_fixture_preserves_endpoints_and_radius` checks
both endpoint planes and the outer radial extent.

### `bhkPlaneShape` collision geometry (FMT-21)

The decoder reads the Havok material, plane normal and distance, and the AABB
half extents and center. It rejects non-finite plane values, a zero normal, and
negative half extents. Shape collection normalizes the plane normal, intersects
the plane with the authored AABB, triangulates the bounded polygon, and applies
the material. Its vertices and plane distance therefore convert from Havok
metres to game units at 69.99125 units per metre. Planes that do not cut the
AABB produce no shape.

The field order and distance meaning come from
`engine-refs/nifxml/nif.xml`'s `bhkPlaneShape`. Havok's local
`hkpPlaneShape.h` confirms that the AABB bounds broad-phase work and plane
edges do not collide. Jolt's local `PlaneShape.h` describes an infinite plane,
so collection uses the bounded triangle mesh representation already supported
by both physics paths. `fmt_21_plane_fixture_preserves_normal_and_distance_after_unit_conversion`
checks the decoded fields and converted surface. The decoder test checks
truncation and invalid values.

### Limited hinge constraint data (FMT-23)

The block decoder supports the Skyrim SE `bhkLimitedHingeConstraint` layout.
It retains both rigid-body block references, priority, each body's full
four-component axis frame and pivot, the minimum and maximum angles in radians,
maximum friction, and the motor type with its serialized payload. Entity counts
other than two, unsupported motor types, truncated fields, non-finite values,
negative friction and reversed limits are rejected. Other constraint block
types remain opaque.

The field order and body count follow `engine-refs/nifxml/nif.xml` and the
matching descriptors in `TES5Edit/Core/wbDataFormatNif.pas`. Jolt's local
`HingeConstraint.h` uses two body frames and a min/max angular limit, matching
the decoded data model. The decoder does not create or simulate a Jolt
constraint. `fmt_23_limited_hinge_retains_body_links_frames_and_limits` checks
all retained fields, while the companion rejection test covers malformed
counts, limits and truncated payloads.

## SimpleWorld

Statics are flattened to world-space primitives on insert: triangles
(meshes, boxes and hulls through the face builder), spheres and capsules.
A uniform grid of 256 unit cells is the broad phase, rebuilt lazily after
inserts and removals. The character is a capsule with feet at `position`,
stepped by integrating gravity, moving, then resolving penetration up to
six times against the closest point between the capsule axis and each
nearby primitive (Ericson's segment-triangle, segment-segment and
point-triangle routines). Contact normals above `max_slope_cos` set
`on_ground` and cancel downward velocity. The fall per step is capped at
one radius so a thin floor cannot be tunnelled. Rays test triangles,
spheres and capsules (cylinder plus caps) and return the nearest hit with
its normal and the body id. Layers that do not block characters (water,
triggers, non collidable, char controller, zones) are skipped by the
character but still hit by rays, since the game layer wants to know about
trigger volumes.

Not in this world: rigid body simulation, constraints, ragdolls, friction,
continuous collision, keyframed movers. Those are Jolt's.

## Tests

### Surface materials for footsteps (2026-09-19)

Shape collection preserves the NIF material ID on primitive shapes and each
compressed-mesh triangle. Material wrappers on compound shapes supply a default,
and child or face materials override it. Scaling and placement preserve these
IDs. `SimpleWorld::raycast` returns the hit surface's material, or `None` when
the source shape has none. The Godot shape adapter accepts the metadata but
does not yet expose it through Jolt queries.

Sources are `engine-refs/nifxml/nif.xml`, its Skyrim Havok material definitions,
`bhkMeshMaterial`, `bhkCMSChunk` and `bhkCMSBigTri`. OpenMW's
`components/nif/physics.cpp` confirms the fields but does not resolve the
large-triangle material field's meaning. A check against both installed Skyrim
mesh archives establishes that it indexes the material table: 24,142 chunks and
65,821 large triangles had valid indices, with 2,913 compressed meshes carrying
multiple materials. This is evidence for the shipped SE assets, not a survey of
modded meshes. Invalid material indices remain unknown.

Synthetic tests check primitive, chunk and large-triangle materials, mixed
faces after an invalid face is skipped, and nested material defaults without
leaking into siblings. This supplies surface identity for the footstep path.
Sound selection and playback are separate game and host integration work.

### Large world boundary survey (2026-09-19)

Branch `large-world-physics` checks the former Skyrim physics boundary with
synthetic geometry. Sources are the local Godot documentation at
`engine-refs/godot-docs/tutorials/physics/large_world_coordinates.rst` and
`engine-refs/godot-docs/classes/class_physicsserver3d.rst`. The former explains
precision loss with distance and the double-precision and origin-shifting
options. The latter supplies the server body, motion-test and ray-query APIs.
These tests exercise our existing solvers, with no new solver or dependency.

`crates/physics/tests/large_world.rs` surveys 33 placements: the origin and
the eight axis and diagonal directions at 64, 65, 128 and 256 exterior cells.
Each cell is 4096 game units. Positive and negative directions are included,
and movement crosses the selected cell boundaries. The tolerances were fixed
before the first run: one game unit for floor and wall contact and ray hit
positions, five game units for displacement over one second of jogging.

- 198 falls and walks cover four cardinal and two diagonal movement directions.
- 132 wall approaches must remain outside the wall on every frame and reach it.
- 132 rays must identify the expected wall, including when facing away from the
  world origin. A second ray after removal must miss the removed wall.

Both Rust tests passed. These are controller and collision-query checks, not
tests of the complete object activation or cell-streaming lifecycle.

`godot/large_world_physics_test.gd` runs without game files or the extension.
It creates world-space triangle meshes on static bodies at the origin, matching
the host's mesh placement convention. At the same 33 placements it drops spheres
onto floors, checks that they stop, casts 132 wall rays and performs 132 sphere
sweeps toward walls. Its contact, ray and sweep tolerance is 0.03 metres,
chosen before the first run. Godot 4.4.1 and 4.7.2 with Jolt both passed:

| Maximum error | Metres |
|---|---|
| Resting sphere centre against floor plus radius | 0.0200001 |
| Ray hit position | 0.00000191 |
| Sweep travel against wall minus radius | 0.0010254 |

The host's `distant_mesh_vertices_keep_placement_within_three_millimetres`
unit test separately checks the Rust triangle conversion against double-precision
expected positions at signed offsets through 256 cells. The Godot script does
not invoke that conversion, so these are separate checks of the adapter and
backend, not an end-to-end asset test.

Run the survey with:

```sh
cargo test -p soe-physics --test large_world -- --nocapture
cargo test -p soe-godot-host distant_mesh_vertices
godot --headless --path godot --script large_world_physics_test.gd
```

This establishes sampled physics behavior beyond 64 cells, through 256 cells
on each horizontal axis, about 15 km from the origin per axis. It does not
establish unlimited world size, rendering precision, ragdolls, constraints,
every collision shape, long-duration motion, or a large mod worldspace loading
and streaming correctly. Absolute positions remain `f32`. Streaming does not
rebase them. No origin shifting was added, and the existing plan's single
precision choice remains a maintenance tradeoff whose limits need measurement.

### Shape and controller tests

`tests/synthetic.rs` writes bhk blocks byte by byte from nif.xml (the
rigid body writer produces the same 250 bytes the shipped files have),
places them under nodes the way `soe_nif` parses them, and checks:
compressed mesh chunks unroll to a triangle mesh in game units, a capsule
dropped from 300 units settles on the floor and walks along it, a water
layer floor is fallen through, a box under a moved node with a body
offset lands and is hit by a ray where the transforms say, nested node
transforms accumulate, big triangles are kept, unknown shape types and
dangling references are reported not fatal, and every truncation of every
block either errors or decodes a shorter prefix, never panics. Unit tests
cover the quaternion, transform inverse and composition, hull faces of a
cube and a tetrahedron, and each closest-point and ray routine.

`tests/real_files.rs` is gated on `SKYRIM_DATA` and reads meshes straight
out of `Skyrim - Meshes0.bsa` and `Meshes1.bsa`. Run on 2026-09-06 over
every mesh (`SOE_PHYSICS_LIMIT=0`):

| | |
|---|---|
| Meshes read | 22,047 |
| With collision | 10,939 |
| Colliders | 13,341 |
| Triangles | 2,712,790 |
| Statics loaded into a world | 10,324 (2,809,639 primitives) |
| Time, release build | 5.5 s |

Shapes: 8,188 triangle meshes, 3,477 convex hulls, 1,698 boxes, 965
capsules, 145 spheres, 997 compounds. Skipped: 151 `bhkSimpleShapePhantom`
bodies (trigger volumes on activators, not rigid bodies), 22
`bhkNiTriStripsShape` (Oblivion-era shapes that survived in a few
meshes), one cylinder, one plane. 52 files the nif crate cannot parse
(skeletons with a `count 4294967295` field, its problem to fix). Every
`bhkRigidBodyT` rotation is a unit quaternion and no collider has an
extent above 200,000 units. The same test then drops a capsule onto
`farmhouse01.nif` and requires it to come to rest on the building.

`examples/physics_dump.rs` prints the colliders of one mesh, from a file or an
archive path, with `--raw` for the trailing bytes of each rigid body.

## Live run (2026-09-14)

Both gated tests passed against the installed archives. The farmhouse capsule test
dropped a capsule from height 815.99 onto a roof starting at 615.99 and it came to
rest at z 551.15, on the farmhouse. The collider decode test converted 2000 shipped
meshes in 1.2 seconds, 1333 of them carrying collision, 1705 colliders and 376971
triangles feeding a world of 1219 statics and 380326 primitives. Shapes broke down
as 1073 triangle meshes, 289 convex hulls, 200 capsules, 189 boxes, 59 compounds, 7
spheres and 42 convex transforms. Twenty one `bhkSimpleShapePhantom` bodies and 4
`bhkNiTriStripsShape` shapes were skipped, both already listed as gaps below. No
code change was needed.

## Not yet

- Rigid bodies, constraints and ragdolls in the headless world. Godot's side
  has them through Jolt already, `SimpleWorld` has statics, one capsule and
  rays, which is what the simulation needs so far.
- The convex shell radius (0.05 m on most shapes) is not added to boxes
  and hulls, so their surfaces sit 3.5 units inside where Havok puts them.
- `bhkNiTriStripsShape`, AABB and caching phantoms,
  constraints (hinge and ragdoll data are parsed as `Other`).
- Keyframed and animated statics (doors, drawbridges) are loaded at their
  rest pose only.
- Skyrim LE files are decoded by the same layouts but were not run
  against real data.

## The character controller, measured (2026-09-18)

`tests/character.rs` builds its geometry directly rather than through `bhk`
bytes, so a 30 degree ramp is 30 degrees and a wall is vertical to the bit, and
a failure points at the controller rather than at the test's own file writer.
Every case reports positions over time, so a failure shows its trajectory.

| Case | Result |
|---|---|
| Fall from 400 units onto a floor | Rests at z 0.0, on ground, and stays within 0.01 units for the last 20 frames |
| A 60 degree face, steeper than the 45 degree standable limit | Slides from x 600 to 542 while z drops 1400 to 972, never penetrates the face, never counts as ground |
| A 20 degree ramp, walkable | Climbs x 198 to 589 and rises z 74 to 217, on ground throughout |
| Walk into a wall at 600 units per second | Stops at x 268.0, which is the wall at 300 minus the 32 unit capsule radius, and no frame crosses the plane |
| Ray down from the feet | Hits the floor plane with an upward normal |

## Collision for a real cell (2026-09-18)

`crates/godot-host/src/collision.rs` walks the same references the renderer
draws and places each model's Havok collision with the same reference
transform, so the two cannot disagree about where a wall is. Gated on
`SKYRIM_DATA`, `WhiterunBanneredMare`:

| | |
|---|---|
| Placed statics with a model | 318 |
| Carrying collision | 287 |
| Static bodies | 281 |
| World primitives | 11,182 |
| Failed to read, blocks skipped | 0, 0 |
| Build time | 0.022 s |

A capsule dropped 300 units above the cell settles on ground, and a ray from
its feet finds a floor within 40 units with an upward normal, which is what
separates standing from being wedged inside geometry by the depenetration loop.
Worth knowing for anyone reading that test: the capsule rests about 240 units
above what a ray straight down the cell's origin finds, because the hearth
leaves an open shaft at the middle, so the ray drops to the ground floor while a
capsule 32 units wide catches the upper floor around it. Walking 1,800 units
from the settled spot is blocked in every direction tried, as a closed interior
must be.

## The same cell inside Godot (2026-09-18)

`godot/physics_test.gd` builds that collision into Godot's physics space through
`CellStreamerNode.stream_cell_collision` and drops a `RigidBody3D` on it. With
`physics/3d/physics_engine` set to `Jolt Physics`, identical on both binaries:

| Godot | Bodies | Shapes | Triangles | Dropped from | Rests at | Frames |
|---|---|---|---|---|---|---|
| 4.4.1 | 281 | 285 | 11,182 | y 12.0 | y 8.295 | 87 |
| 4.7.2 | 281 | 285 | 11,182 | y 12.0 | y 8.295 | 87 |

Those measurements describe the older triangle conversion. As of the
reference fix, boxes, spheres, capsules and convex hulls use Godot's native
shape types. `PhysicsServer3D.body_add_shape` accepts a local transform,
so translated spheres and rotated capsules do not need approximations.
Primitive dimensions and origins are converted from game units to metres.
Capsules retain the midpoint, endpoint axis and cylindrical section.
Compounds compose the parent and child transforms before conversion.

Source: `engine-refs/godot-docs/classes/class_physicsserver3d.rst`,
`body_add_shape` and `shape_set_data`. Rust regression tests cover sphere
translation and radius, capsule endpoints in both directions, and rotated
compound boxes. The old body/triangle counts above are historical. New
contact and performance measurements must not be inferred from them.

The updated `godot/physics_test.gd` run on 2026-09-19, Godot 4.7.2 Flatpak
with Jolt, passed: 281 bodies, 285 shapes, 9,782 triangles, zero failed or
skipped colliders. The probe fell from y 12.0 to y 8.295 and settled after
87 frames. This establishes the real-cell contact smoke test, not a
performance comparison or a survey of every shape. Both release-mode
`real_collision` Rust tests also passed against the installed game.

## Walking, not just colliding (2026-09-18, branch `controller`)

The capsule could fall, slide and stop against a wall. What it could not do was
walk anywhere real: a stair riser and a door threshold both stopped it dead,
because a push-out loop has no idea that a 24 unit lip is something you step
over rather than something you lean on. Three additions close that, and they
are what separate a character controller from a collision response.

**A step height.** When a walk makes less horizontal progress than it asked
for, the move is retried from `step_height` (36 units) higher, then settled
back down no further than it was lifted. It only runs from the ground, so it
cannot be used to climb in mid air, and it is refused when the lift itself does
not fit or when nothing standable is underneath the new spot.

**A downward snap of the same size.** Walking off a riser used to launch the
capsule into a short fall. It now follows the floor down by at most one step
height, so a 30 unit drop reads as a step rather than as a fall.

**Crouch.** `set_crouch` swaps the capsule to `crouch_height` (72 units against
128 standing), and standing up is refused while the taller capsule would not
fit. It answers the state that now holds rather than a success flag, so a
caller that is under a table learns it is still crouching.

The interface is a direction and a speed. `PhysicsWorld::walk` normalises the
direction, drops its Z and multiplies by one of the speed constants, so nothing
about keyboards or actors reaches this crate and the same controller carries the
player and an NPC on a package.

`WALK_SPEED` 110, `FAST_WALK_SPEED` 160, `JOG_SPEED` 180 and `RUN_SPEED` 375
units per real second now live here rather than in `soe-game`, because physics
is the lower crate and a controller cannot depend on the runtime that drives
it. `crates/game/src/ai.rs` still declares its own copies, and should import
these instead. A test pins the four values so the two cannot drift apart
silently while that is outstanding.

`Iso::placement` moved here for the same reason. The reference placement, an
X, Y, Z Euler rotation in radians with a uniform scale, was written out
longhand inside the Godot host, and the collision builder needs the identical
answer. Two copies of a rotation convention drift, so there is now one, and
`crates/godot-host/src/data_source.rs` should call it.

### Measured

| Case | Result |
|---|---|
| A 24 unit stair riser, under the step height | Climbs eight steps, z 0 to 168, x -200 to 288, on ground throughout, never above the tread ahead of it |
| A 72 unit riser, over the step height | Stops at x 33.0, the riser at 0 plus the 32 unit radius, and never rises a step |
| A 12 unit door threshold | Crosses it and stays on the ground |
| Walking off a 30 unit slab | Follows the floor down, at most two frames of real descent, lands at z 0 |
| A ceiling at 110 units | Crouched fits, standing up is refused, and it is allowed again once clear of the ceiling |
| A standing jump | Peak 74.6 units against the 76 configured, lands back on the floor, a second jump in mid air is refused |
| One second at jog speed | 180.1 units, and a 45 degree direction moves equally in x and y |

### The Bannered Mare, walked

`tests/real_files.rs::a_capsule_walks_around_the_bannered_mare` builds the inn's
collision from the shipped plugins and meshes, 281 references with collision
and 11,182 primitives in 0.019 s, which matches what the Godot side loads.

Finding the floor took two attempts worth recording. Rays cast down from above
the cell hit the roof, so the first version stood the capsule on it and walked
straight off the edge into a 1,290 unit fall. Rays now start 150 units above
the lowest collision, which is inside the ground floor room, and the most
popular hit height is the floor: 73 of 256 samples land at z 64, against 20 at
z 72 and single digits everywhere else.

From there, four legs of a second each: 102.5 units, 24.9, 101.6 and 109.9. The
short one is a wall, which is what an inn is full of. The capsule stayed on the
ground for 369 of 420 frames, never dropped below z 33 with the floor at 26,
and no leg wedged it.

### Thin-floor snapping (2026-09-19)

The player follow-up exposed discontinuous vertical movement. The step-down
search resolved only the endpoint of a full 36-unit drop. That could put the
capsule axis through a thin triangle before resolving contact, where winding
could select its underside. OpenMW's `apps/openmw/mwphysics/stepper.cpp` instead
traces down to the first supporting surface.

`SimpleWorld` now approximates that downward search with increments no larger
than a quarter of the capsule radius, stopping at the first standable contact.
Step-up settling uses the same search. The 256-iteration work limit also limits
search distance for unusually small capsules. This is not a full swept-capsule
solver, and horizontal collision is unchanged.

The regression places oppositely wound thin floors 24 units apart and checks
standing and crouching capsules at six starting heights. Existing stair,
slope, ceiling and large-world tests still apply. Installed Bannered Mare
routes and render-camera interpolation are covered in `docs/godot-host.md`.

## Authored actor controller geometry, CMB-29

`bhkSimpleShapePhantom` decodes the published shape reference, Havok filter
(layer, flags and group), and matrix. `shape::collect` retains these phantoms
in `Collected::phantoms`, separate from static rigid bodies. Each entry carries
its node, material-bearing shape tree and root-space transform. Lengths are
converted from Havok metres to game units once. Capsule endpoints are cap
centres, so total axial extent includes two radii. Trigger and other non-
controller layers keep their volume and filter data but are not static
colliders. The controller-shape helpers consider only `CHAR_CONTROLLER`
phantoms.

The layout comes from local `nifxml/nif.xml` (`bhkWorldObject` and
`bhkSimpleShapePhantom`) and `nifly/src/bhk.cpp`. The reader reuses the existing
shape-tree decoder and matrix conversion. Truncation, non-finite geometry,
invalid capsule radii, singular transforms and incomplete phantom shape trees
reject. Nif.xml describes phantoms as non-physical objects that report overlap
transitions. This decoder preserves their data only. No phantom is
automatically installed as a static body or treated as a solid contact body.

`fmt_22_named_trigger_phantom_keeps_volume_and_filter_without_solid_body` in
`crates/physics/tests/synthetic.rs` builds a named trigger phantom from the
published layout. It checks the volume, transform and filter fields, and that
the controller resolver does not use it as an actor controller. Jolt's sensor
API likewise separates overlap notifications from collision response, but
this decoder does not create a Jolt sensor.

The source choice was corrected using the published
[Dynamic Collision Adjustment implementation](https://github.com/ersh1/DynamicCollisionAdjustment/blob/main/src/AdjustmentHandler.cpp)
and its [author's description](https://www.nexusmods.com/skyrimspecialedition/mods/76783).
The author distinguishes skeleton-authored collision from runtime-generated
controllers. Its skeleton check identifies the controller collision layer and
separates the character bumper by material. Collection preserves that material
instead of treating every controller-layer shape as the main hit shape.

`controller_geometry_survey` in `godot-host` and the installed physics test
read the two mesh archives in place. The 2026-09-20 run parsed 76 skeletons,
with 36 controller-layer phantoms in 26 files. This includes bumpers. The
human skeleton carries a bumper, while the giant and dragon also carry main
controller geometry. The full physics regression parsed 22,047 meshes.

`character_metadata_survey` checks the independent HKX source assumption.
All 353 character data objects in the installed animation archive have
height 1.7, radius 0.4, filter 1, scale 1 and no controller cinfo pointer.
The layout is published by `serde-hkx` and the default values also appear in
Nemesis character XML. Identical fields across humans, creatures and objects
do not establish actor-specific collision geometry. They are not used as a
fallback capsule. CMB-33 owns runtime-generated geometry, while CMB-28 owns
host body installation and synchronization. This decoder adds no nodes or
server bodies, and does not implement ragdoll collision.

Validation at completion: the combined `tools/lint.sh` gate passed with 1,209
reported Rust results and 11 Python tests, with no optional-tool skips.
The installed physics suite passed 50 tests, including the skeleton survey
and the 22,047-mesh regression. The HKX metadata survey read 353 character
objects. Synthetic checks cover filter preservation, capsule extent and
transform conversion, truncation, malformed values and incomplete compounds.

## Runtime-generated actor controller geometry, CMB-33

`shape::generated_controller_shape(scale)` builds a capsule for a skeleton with
no authored `CHAR_CONTROLLER` phantom. The template height and radius, 1.7 and
0.4 metres, are the values `character_metadata_survey` found on all 353
installed `hkbCharacterData` objects (CMB-29, above): Bethesda ships one
uniform template and relies on runtime scale to size it per actor, so the
generator reproduces the template and lets the caller supply the scale rather
than inventing its own constant. `shape::controller_shape(collected, scale)`
is the entry point: it returns the largest authored phantom's transform and
shape by AABB diagonal when `collected.phantoms` is non-empty, or the
identity transform with a generated capsule otherwise.
`Collected::has_authored_controller` exposes the same non-empty check for a
caller that only needs to know which path will run.

Picking the largest phantom by extent, rather than by Havok material, is a
known gap. The [Dynamic Collision Adjustment
source](https://github.com/ersh1/DynamicCollisionAdjustment/blob/main/src/AdjustmentHandler.cpp)
cited under CMB-29 separates the character bumper from the main hit shape by
material ID, but no numeric bumper material ID turned up anywhere in the local
corpus (`MaterialIDs.h`, `nifxml/nif.xml`, `nifly` were all checked). The
human skeleton's survey result, one bumper and no main shape, means the
extent heuristic and a material check would agree on every skeleton observed
so far, but a skeleton that authors both a bumper and a larger non-controller
shape under the same phantom list could pick the wrong one. Finding the
bumper material ID and switching the selection to it is follow-up work, not
done here.

Scale comes from `game::World::actor_controller_scale`, documented in
`docs/game.md`. This crate takes scale as a plain `f32` and does not read
NPC or RACE records itself, keeping `soe_physics` free of game-state
dependencies as the crate split in the module doc above requires. A
non-finite or non-positive scale falls back to 1.0 inside
`generated_controller_shape` rather than producing degenerate geometry.

`godot_host::DataSource::actor_controller_shape` is the only caller, and it
has no caller of its own yet. CMB-28, host body installation, has not
started, so a generated capsule does not currently reach a running actor.
Verification against a subset of installed skeletons, called for by CMB-33's
done check, is left to the verifier and breaker that test this change, since
this session added no new tests per its task instructions. The existing
`controller_geometry_survey` and `character_metadata_survey` tests, both
described above, remain the tests that exercise the source data these
functions build from.
