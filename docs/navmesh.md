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
so links through them still resolve, and A* skips them.

`vanilla::Builder` takes navmeshes under any key (a raw FormID for one file, a
packed `GlobalFormId` for a load order) with a closure that maps the raw FormID an
edge link names to a key, defers cross-mesh links until every mesh is in, and
reports how many resolved. `vanilla::from_load_order` does that for every winning
`NAVM` a filter accepts, resolving FormIDs through the load order, which is how
a patch that overrides one navmesh still links to the vanilla ones beside it.

## The graph

`NavGraph::find_path` is A* over triangle adjacency with centroid distance as the
cost and the heuristic. `find_path_between` locates two world points first
(`locate`: the triangle whose XY contains the point and whose surface is nearest
in Z, meshes pruned by their bounds, with `nearest` as a fallback for points just
off the mesh). A `Path` carries the triangles crossed and a polyline: the start
point, the midpoint of every crossed edge, the end point. A segment from a
triangle's centroid to the midpoint of one of its own edges cannot leave the
triangle, so the polyline stays on the walkable surface. It is not the shortest
walk, that needs string pulling (the funnel algorithm), which is the next step
and the reason the cost of a path across a flat cell is four times the straight
line in the test output. `reachable` floods from a node for connectivity checks.

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
and an `NVNM` block through `to_geometry`, with in-mesh edges, no edge links,
and a grid of the requested divisor. The block round-trips through
`Geometry::parse` byte for byte, so it can sit in a `NAVM` record written by
`soe-esm`. That is the piece a mod with a broken or missing navmesh needs and the
Creation Kit used to be the only source of.

## Tests

- Unit tests in each module: adjacency from shared edges, A* on a strip, a
  blocked triangle cutting it, locate and nearest, links joining two meshes, a
  dangling link ignored, in-mesh edges and cross-mesh links resolving from
  synthetic `NVNM` geometry, unresolved and malformed links counted as borders,
  the axis map, agent validation, a flat cell producing upward polygons, the
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

- String pulling. Paths are correct, not short.
- No path costs from triangle flags (water, preferred, no large creatures).
  `flags` names them, A* ignores them.
- No dynamic obstacles or door state, the graph is static.
- Generation reads render meshes, not `bhk` collision, until `soe-nif` decodes
  those. Interiors are not tested against real data, only Tamriel's exteriors.
- The generated `NVNM` carries no edge links, cover triangles or location CRC,
  so a generated cell does not join its neighbours until a stitching pass
  matches border vertices, which is the next piece for a Creation Kit
  replacement.
- Tiled generation for anything bigger than a few cells, and the detail mesh.

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
