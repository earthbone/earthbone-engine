# soe-navmesh: the shipped navmeshes as a graph, and new ones from Recast

Two halves, one graph type between them.

## The vanilla half

`soe-esm` already parses a `NAVM` record's `NVNM` block into vertices, triangles,
edge links and door links (`records::navm::Geometry`). This crate turns a set of
those into `graph::NavGraph`, where a node is a triangle in a mesh and a triangle
has up to three neighbours, one across each edge, possibly in another mesh.

The rule for the edge fields comes from xEdit's `wbEdgeToStr` in
`wbDefinitionsCommon.pas`: each triangle carries three signed 16-bit edge values
and a flags word. If bit `e` of the flags is set (bit 0 for edge 0-1, bit 1 for
edge 1-2, bit 2 for edge 2-0), edge `e` is an index into the record's edge link
array, and the link names another navmesh by FormID plus a triangle in it.
Otherwise the value is the index of the adjacent triangle in the same navmesh, or
`-1` for a border. Door links say which triangle a door reference stands on and
are counted, not walked. Triangles with the deleted flag (0x8) stay in the graph
so links through them still resolve, and `A*` skips them.

`vanilla::Builder` takes navmeshes under any key (a raw FormID for one file, a
packed `GlobalFormId` for a load order) with a closure that maps the raw FormID an
edge link names to a key, defers cross-mesh links until every mesh is in, and
reports how many resolved. `vanilla::from_load_order` does that for every winning
`NAVM` a filter accepts, resolving FormIDs through the load order, which is how
a patch that overrides one navmesh still links to the vanilla ones beside it.

## The graph

`NavGraph::find_path` is `A*` over triangle adjacency with centroid distance as the
cost and the heuristic. Each centroid segment is charged using the flags on the
triangle it enters. The default `PathCosts` policy makes a water triangle cost ten
times its centroid distance and a preferred triangle cost half. These are configurable
engine policy defaults, not measured Skyrim setting values. Callers can supply a
different valid policy through `find_path_with_costs`. The heuristic is scaled by the
lowest multiplier, including a triangle that is both water and preferred, so it remains
admissible. xEdit supplies the water and preferred bit values through
`wbNavmeshTriangleFlags`.
`find_path_between` locates two world points first
(`locate`: the triangle whose XY contains the point and whose surface is nearest
in Z, meshes pruned by their bounds, with `nearest` as a fallback for points just
off the mesh). A `Path` carries the selected triangles, the search cost, and a pulled polyline.
`cost` stays the sum of centroid distances used by `A*`. It is not the polyline
length. The funnel selects shortest XY corners within each contiguous corridor,
then intersects every portal to retain triangle-plane height changes. Each point
has a `point_nodes` owner, so cell changes do not depend on waypoint counts.
`reachable` floods from a node for connectivity checks.

The algorithm is adapted from local recastnavigation's
`Detour/Source/DetourNavMeshQuery.cpp`, `findStraightPath` and `appendPortals`
with all crossings. This Rust adaptation retains the source notice in
`crates/navmesh/LICENSE-Detour.txt`. Detour's full query requires its own mesh
representation. Keeping a small adaptation preserves native NAVM triangles,
resolved links and the existing graph rather than introducing another navmesh.
No dependency was added. Detour's `dtQueryFilter::getCost` applies the current
polygon's area multiplier to each traversed segment. This graph uses the entered
triangle's flag multiplier because its centroid adjacency does not retain Detour's
portal segment state. xEdit's `wbNavmeshTriangleFlags` supplies the NAVM bit values.

Portal winding comes from the departing triangle. Cross-mesh portals use the
shared span of collinear edges, with a 0.01-unit position/height tolerance.
The graph also permits arbitrary directed links without shared geometry. Those
split smoothing into separate corridors with explicit departure and arrival
points. Such gap segments are not claimed to lie on the walkable surface.
Exact requested endpoints stay unchanged. If `nearest` selects an off-mesh
endpoint, its approach or departure segment has the same limitation. Degenerate
funnel/intersection cases retain portal midpoints for that corridor. String
pulling minimizes XY travel, not geodesic distance over a sloped surface.

## The generation half

`generate::generate(scene, agent)` runs Recast's solo-mesh pipeline through
`rerecast` 0.4.0, a Rust port of recastnavigation with no engine dependency
(MIT or Apache-2.0, only `glam`, `slotmap`, `bitflags`, `thiserror` and `anyhow`
behind it, and it builds with the Homebrew toolchain here). The stages are
rasterize, filter low hanging obstacles, ledges and low ceilings, compact, erode
by the agent radius, distance field, watershed regions, contours, convex
polygons. The detail mesh stage is skipped, polygon vertices already sit within
one cell height of the surface.

Input is `scene::Scene`, a triangle soup in Skyrim's frame: `add_nif_mesh`
places a `soe-nif` mesh through its own transform and a reference transform,
`add_land` lays one cell's 33 by 33 `LAND` heights at 128-unit spacing from the
cell origin (row 0 south, as `Land::heights.decode` gives), `add_box` is for
tests. The NIF collision blocks are still opaque in `soe-nif`, so render
geometry stands in for collision, which is what the Creation Kit's generator
uses as well.

`Agent` is in game units. The defaults (height 128, radius 24, climb 40, slope
50 degrees, 12-unit cells, 8-unit cell height) describe a human actor and take a
flat cell to a navmesh in about a quarter of a second in a debug build. A
world-sized scene at that cell size is refused by `max_columns` rather than
attempted.

Recast is Y up. Skyrim `(x, y, z)` maps to Recast `(x, z, -y)`, the same map the
Godot host uses, and polygons come back through the inverse. Recast winds its
polygons clockwise seen from +Y, which the mirrored axis turns into
counter-clockwise seen from +Z, so the order is kept.

The output `Navmesh` (vertices, convex polygons, per-edge neighbours) becomes a
`NavGraph` through `to_graph` (fan triangulation, adjacency from shared edges)
and an `NVNM` block through `to_geometry`, with in-mesh edges and a grid of
the requested divisor. `stitch::stitch_cells` supplies cross-cell edge links. The block round-trips through
`Geometry::parse` byte for byte, so it can sit in a `NAVM` record written by
`soe-esm`. That is the piece a mod with a broken or missing navmesh needs and the
Creation Kit used to be the only source of.

## Tests

- Unit tests in each module: adjacency from shared edges, `A*` on a strip, a
  blocked triangle cutting it, locate and nearest, links joining two meshes, a
  dangling link ignored, in-mesh edges and cross-mesh links resolving from
  synthetic `NVNM` geometry, unresolved and malformed links counted as borders,
  water avoidance and preferred route costs, retained NAVM flags, the axis map,
  agent validation, a flat cell producing upward polygons, the
  `NVNM` export round trip with mutual edges.
- `tests/synthetic.rs`, the plan's done check: a flat cell with a box in the
  middle produces a navmesh whose path between two points on either side never
  enters the box footprint (sampled every ten units along the polyline), a wall
  across the cell splits it so the path fails, and a 20-unit kerb under the
  40-unit climb is walked over.
- `tests/real_files.rs`, gated on `SKYRIM_DATA`, every `NAVM` under Tamriel
  from the five base masters:

```text
SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data" \
  cargo test -p soe-navmesh --release --test real_files -- --nocapture
```

Result on 2026-09-06:

| | |
|---|---|
| Navmeshes under Tamriel with geometry | 13,011 |
| Triangles | 2,271,709 |
| Edge links | 187,216 |
| Resolved | 187,193 |
| Unresolved | 23 |
| Door links | 558 |
| Graph build time (release) | 0.24 s |
| Largest navmesh | 1,296 triangles, path of 59 across it |
| Cross-mesh paths | 50 of 50 |
| Reachable from the largest navmesh | 2,101,384 triangles across 9,221 navmeshes |

The 23 unresolved links point at navmeshes that are not winners under Tamriel
(interior cells reached through load doors, or Creation Club files not in the
five masters). The single reachable region of 2.1 million triangles is the
overworld, the rest are islands: mountain tops, sealed areas, and cells whose
links go through interiors.

## Live run (2026-09-14)

The gated test stitches every vanilla `NAVM` record into one graph. 13011 meshes,
2271709 triangles, 0 deleted triangles, 187216 edge links, 187193 resolved and 23
unresolved (the interior and Creation Club edges noted above), 558 door links,
built in 0.20 seconds. The largest navmesh in Tamriel has 1296 triangles, a path
across it costs 5205 over 59 triangles. All fifty cross mesh paths tried succeeded.
Following links from the largest navmesh reaches 2101384 triangles across 9221
navmeshes, the single connected overworld region described above. No code change
was needed.

## Not done yet

- No large-creature path policy. Its NAVM flag is retained, but actor size is not
  yet part of navigation queries.
- No dynamic obstacles or door state, the graph is static.
- Generation reads render meshes, not `bhk` collision, until `soe-nif` decodes
  those. Interiors are not tested against real data, only Tamriel's exteriors.
- Generated `NVNM` does not author cover triangles or location CRC. NAVI info
  maps and authored-neighbour regeneration belong to the editor/export work.
- Scene geometry selection for world-sized generation, and the detail mesh.

## Deleted navmeshes (2026-09-18, branch `dirty-records`)

The shipped DLC masters delete navmeshes (LOOT counts 59 in Dawnguard.esm, 5 in
HearthFires.esm, 1 in Dragonborn.esm, 3 in Update.esm for the 1.6.1170 build),
and the vanilla engine crashes when a neighbour's edge link still points at
one, which is why xEdit's cleaning removes them. The graph handles it in place:
`from_load_order` skips a winner that carries the deleted flag and marks its key
on the `Builder`, and at `build` every pending link to a marked key becomes a
border. `Stats::deleted_meshes` and `Stats::repaired_links` count both, apart
from `unresolved_links`, which keeps its meaning of a link to a navmesh nobody
loaded. `edge_links` equals resolved plus unresolved plus repaired.

`tests/dirty.rs` builds two linked squares and a patch that deletes one, checks
the survivor keeps its other edges, the link count moves from resolved to
repaired, the cell index files only the survivor, and the esm dirty report
counts the deletion.

### Live run (2026-09-18)

Tamriel over the five masters: 13,011 meshes and 2,271,709 triangles as before,
66 deleted navmeshes now named as deleted, and 0 links repaired. The edge link
split is unchanged at 187,216 links, 187,193 resolved and 23 unresolved. Nothing
under Tamriel aims an edge at a navmesh the DLC deleted, which is what the
Creation Kit produces: deleting a navmesh re-finalizes its neighbours and
rewrites their links in the same plugin.

The 66 were already absent from the graph before this branch, because a deleted
record is shipped without its `NVNM` block and the builder skipped it for having
no geometry. What changed is that they are counted, their keys are known, and a
link naming one is cut and reported instead of counted as unresolved. The five
remaining deleted navmeshes of the 71 the dirty report counts sit in other
worldspaces, outside this filter.

## Query indexing and oracle tests (2026-09-19)

The graph lazily builds a median-split tree over mesh bounds. The layout follows
the bounding volume tree in the local Recast/Detour sources,
`DetourNavMeshBuilder.cpp` and `DetourNavMeshQuery.cpp`. Detour's quantized tile
polygons do not match this engine's triangle and centroid API, so the index
retains floating-point bounds and the existing query semantics. Bounds include
rounded centroids. Adding a mesh invalidates the index. Equal distances still
choose the first mesh and triangle, regardless of traversal order.

A second lazy cache groups weakly connected meshes. Different groups cannot
have a route. Sharing a group does not imply reachability, since links can be
directed and triangles within a mesh can be disconnected. The cache includes
blocked triangles, making rejection conservative across block-state changes.
Adding meshes or links invalidates it. `A*` skips stale heap entries and rejects
out-of-range endpoints.

`graph/query_tests.rs` compares indexed point selection with exhaustive search
and `A*` with an independent Dijkstra oracle over generated directed, blocked
graphs. Each property runs 192 cases. A fixed test requires both point searches
to prune at least 90 percent of 1,024 separated meshes. Tie handling and cache
invalidation have explicit tests. This is a mesh-level index, not a triangle
index or hierarchical pathfinder. See `docs/performance.md` for measurements.

## String pulling validation

Synthetic checks fix tolerances before execution. A flat four-tile route must
have length 3.8 within 0.0001 units and beat its midpoint route by one percent.
A bent corridor must reach its analytic corner distance in either winding.
Sampling each segment verifies triangle containment and ridge height. Partial
seams, one-way links, explicit gaps, coincident endpoints, off-mesh endpoints
and a zero-width portal have separate checks. The existing Dijkstra oracle
still checks `A*` costs independently of smoothing.

The game driver consumes waypoint ownership directly. Its regression checks
cell assignment at two seams and at the departure of a gap. Existing package
travel, save/load and generated obstacle tests remain part of the gate.

The installed-data survey samples up to 100 connected non-gap routes. It
requires at least 50, checks three interior samples of each segment against
its owner triangle, permits at most 0.25 units of numerical length difference,
and requires at least half the routes to shorten in XY. Those thresholds were
written before the first corpus run. Game assets remain external.

The first live survey passed on all 100 routes, with 99 shorter in XY.
All 50 cross-mesh queries still succeeded. It loaded 13,011 meshes and
2,271,709 triangles, with the same 187,193 resolved links and 23 unresolved
links as the prior baseline. The largest mesh's 59-triangle route retains its
5,205 centroid-distance cost. No frame-time improvement is claimed.

## Generated cell stitching, AI-10

`generate_cell(scene, agent, x, y)` builds one exterior cell through Recast's
padded tile pipeline. Supply the same scene, including geometry across cell
borders, and agent to every neighbouring tile. The ground voxel size rounds
down to divide 4096, so the default 12-unit request uses 4096/342 units.
All tiles share the scene's vertical origin. A border of the rounded actor
radius plus three voxels provides erosion context and is excluded from regions.
Missing neighbouring scene geometry remains an obstacle to generation.

`stitch::stitch_cells` accepts generated cells with their NAVM FormIDs in one
export namespace. It returns NVNM geometries sorted by FormID. Cardinal
neighbours match border positions within 0.01 units in XY. The caller supplies
an explicit finite, nonnegative `max_height_difference`, normally the agent's
voxel height, to accommodate contour quantization. This is a generation policy,
not an implementation of Bethesda step-height behaviour. Corner groups use the
lowest FormID's position and reject chains whose full extent exceeds either
tolerance. Polygons are checked before and after snapping and subdivision.
Unequal and partial spans gain vertices and split triangles so every portal
occupies a whole triangle edge. Links use kind 0, Portal, and point back to
the source triangle. Heights outside the chosen tolerance, holes and absent neighbours stay
unlinked. Triangles and their grid memberships are rebuilt together. The
ordinary `vanilla::Builder` loads these results, including after an NVNM
serialization round trip. No host nodes or dependencies were added.

The implementation follows local
`recastnavigation/RecastDemo/Source/Sample_TileMesh.cpp`, `buildTileMesh`,
for padding and border regions. Its neighbouring-geometry requirement is
retained. `recastnavigation/Detour/Source/DetourNavMesh.cpp`,
`findConnectingPolys` and `connectExtLinks`, supplies the border-span model.
Detour permits multiple links per polygon edge, while NVNM has one per
triangle edge. The Rust pass therefore splits spans before linking and does
not adopt Detour's separate mesh representation or step-height links.
`TES5Edit/Core/wbDefinitionsTES5.pas`, `wbNVNM` and NAVI, establishes that
per-triangle NVNM links and the NAVI info map are distinct outputs.
`TES5Edit/Core/wbDefinitionsCommon.pas`, `wbNavmeshEdgeLinkEnum`, names kind 0.
This API generates cell geometry, not a complete finalized plugin with NAVI.

`tests/stitch.rs` includes the `ai_10` done check: generated cells at negative
and positive coordinates share exact border vertices after export, and paths
cross both axes in both directions. Other fixtures cover unequal subdivision,
partial spans, missing neighbours, height gaps, an obstructed seam, malformed
inputs and unchanged inputs after rejection. A sloped two-cell fixture and
flat/sloped two-by-two fixtures cover contour quantization and shared corners.
Repeated vertices, vertical projected edges and over-wide corner chains reject
before export. Exported links are checked for
exact reciprocal vertices and triangle references, then round-trip through
NVNM before path queries. Reversed input order must produce identical output.
Real-game generation and editor integration are not exercised by these tests.

Validation at `63f3fae` on 2026-09-20: all nine stitching regressions passed.
The full `tools/lint.sh` gate passed 1,207 Rust tests and 11 Python tests,
including all three Clippy configurations. All optional tools ran. Tier 1
leaves installed-game and save-corpus inputs unset. The generated-cell tests
use synthetic scenes and do not claim real-game regeneration evidence.

## Runtime door associations

A vanilla NVNM door entry is a triangle index, PathingDoor CRC and door REFR
FormID. It does not name an edge slot or the far-side triangle. This is the
layout in local `TES5Edit/Core/wbDefinitionsTES5.pas`, `wbNVNM` Door Links.
`CommonLibSSE-NG/include/RE/B/BSNavmesh.h` also declares separate door-portal
and closed-door arrays, each associating a pathing door with a triangle.
Blocking all edges of that triangle would remove ordinary walkable space,
not just the doorway connection.

Local `recastnavigation/DetourTileCache/Include/DetourTileCache.h` requires
an explicit cylinder or box obstacle and rebuilds affected tiles. It does
not recover a swinging door's footprint from an NVNM door association.
Door collision geometry and its navigation-edge association must therefore
be established before an automatic blocking-door update can be claimed.
The game already stores lock and logical open state on the shared reference.
Those fields alone cannot identify which triangle edge the door obstructs.

AI-09 is blocked on HST-01, swinging-door animation and collision, and AI-17,
dynamic obstacle updates. Its acceptance check still requires closing one
blocking door to remove only its navigation link and opening to restore it.
Physical footprint binding and invalidation of already planned routes are
prerequisites. No door-edge heuristic or runtime implementation was added by
this research disposition.
