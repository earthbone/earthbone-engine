# soe-physics: Havok shapes from meshes, and a static collision world

Crate `crates/physics`, library `soe_physics`. Depends on `soe-nif` only,
`soe-bsa` for the gated tests.

## Jolt, and why it is not linked yet

The plan called for Jolt through its C bindings. `joltc-sys` 0.3.1 on
crates.io (Jolt 5.0.0, May 2024) builds JoltC with the `cmake` crate, and
this machine has no CMake, so the binding cannot build here. `jolt-rust`
is not published on crates.io. Rather than stub Jolt, the crate puts the
world behind the `PhysicsWorld` trait and ships `SimpleWorld`, a plain
Rust implementation of what the engine needs first: statics, a capsule
character controller, and rays. Jolt replaces `SimpleWorld` behind the
same trait once CMake is available, and the two layers below (block
decoding and shape collection) are what a Jolt backend consumes as well.

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

`examples/dump.rs` prints the colliders of one mesh, from a file or an
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

- Jolt as the world, once CMake is on the build machine.
- The convex shell radius (0.05 m on most shapes) is not added to boxes
  and hulls, so their surfaces sit 3.5 units inside where Havok puts them.
- `bhkNiTriStripsShape`, `bhkCylinderShape`, `bhkPlaneShape`, phantoms,
  constraints (hinge and ragdoll data are parsed as `Other`).
- Keyframed and animated statics (doors, drawbridges) are loaded at their
  rest pose only.
- Skyrim LE files are decoded by the same layouts but were not run
  against real data.
