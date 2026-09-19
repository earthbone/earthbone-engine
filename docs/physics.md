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
  variants, `bhkRigidBody` and `bhkRigidBodyT`, box, sphere, capsule,
  convex vertices, `bhkTransformShape` and `bhkConvexTransformShape`,
  `bhkListShape`, `bhkMoppBvTreeShape` (the MOPP tree itself is Havok's
  closed code and is skipped, the wrapped shape is used),
  `bhkCompressedMeshShape` and its data with chunks, strips, big
  triangles and per chunk transforms. Everything else comes back as
  `Bhk::Other(name)`.
- `shape`: walks the scene graph the way `soe_nif::mesh` does, resolves
  each collision object to a `Collider` with a world `Iso` transform and a
  `Shape` (box, sphere, capsule, convex hull, triangle mesh, compound),
  converted from Havok metres to game units at 69.99125 units per metre.
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
- `bhkNiTriStripsShape`, `bhkCylinderShape`, `bhkPlaneShape`, phantoms,
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
