// SPDX-License-Identifier: GPL-3.0-or-later

//! Synthetic records through the engine's builders, following the travel
//! fixture in game/tests/packages.rs. No private copy of actor state.

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, package, Game};
use soe_godot_host::mesh::MeshData;
use soe_navmesh::{NavGraph, NodeId};
use soe_papyrus::asm::{i, ident, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{ObjectId, Opcode, Vm};

const CELL: u32 = 0x800;
const BASE: u32 = 0x801;
const TEMPLATE: u32 = 0x802;
const PACKAGE: u32 = 0x803;
const MARKER: u32 = 0x804;
const NAV: u32 = 0x805;
const SANDBOX_BASE: u32 = 0x806;
const SANDBOX_TEMPLATE: u32 = 0x807;
const SANDBOX_PACKAGE: u32 = 0x808;
const ACTOR: u32 = 0x1000;
const FAR: f32 = 100_000_000.0;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

pub(super) fn scripts(vm: &mut Vm, count: usize) -> Result<Vec<ObjectId>, String> {
    let mut function = FunctionBuilder::new("Update", "None");
    for _ in 0..32 {
        function = function.ins(Opcode::IAdd, vec![ident("work"), ident("work"), i(1)]);
    }
    let script = ScriptBuilder::new("Stress", "")
        .variable("work", "Int", i(0))
        .variable("completed", "Int", i(0))
        .function(
            function
                .ins(
                    Opcode::IAdd,
                    vec![ident("completed"), ident("completed"), i(1)],
                )
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    vm.load_bytes(&script).map_err(|e| e.to_string())?;
    (0..count)
        .map(|n| {
            let object = vm.create_object(&format!("stress{n}"));
            vm.attach(object, "Stress").map_err(|e| e.to_string())?;
            Ok(object)
        })
        .collect()
}

fn placed(kind: [u8; 4], id: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data: Vec<u8> = position.into_iter().flat_map(f32::to_le_bytes).collect();
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&kind), id)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

pub(super) fn actors(count: usize) -> Result<(Game, Vec<GlobalFormId>), String> {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::build_template(TEMPLATE, "StressTravelTemplate");
    let template = package::close_header(template, None);
    let template = package::with_location_input(template, 0, package::Location::NearSelf, 0);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Travel", &[0], true);
    let template = package::with_input_name(template, 0, "Place to Travel", true);
    plugin.push_top(package::with_actions(template));
    let sandbox_template = package::build_template(SANDBOX_TEMPLATE, "StressSandboxTemplate");
    let sandbox_template = package::close_header(sandbox_template, None);
    let sandbox_template =
        package::with_location_input(sandbox_template, 0, package::Location::NearSelf, 200);
    let sandbox_template = package::finish(sandbox_template);
    let sandbox_template = package::with_procedure(sandbox_template, "Sandbox", &[0], true);
    let sandbox_template = package::with_input_name(sandbox_template, 0, "Location", true);
    plugin.push_top(package::with_actions(sandbox_template));
    let travel = package::build_record(PACKAGE, "StressTravel", 0, package::Schedule::default());
    let travel = package::close_header(travel, Some(TEMPLATE));
    let travel =
        package::with_location_input(travel, 0, package::Location::NearReference(MARKER), 0);
    plugin.push_top(package::with_actions(package::finish(travel)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "StressActor",
            "Stress actor",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        npc::build_record(
            SANDBOX_BASE,
            "StressSandboxActor",
            "Stress sandbox actor",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"PKID"), SANDBOX_PACKAGE.to_le_bytes()),
    );
    let sandbox = package::build_record(
        SANDBOX_PACKAGE,
        "StressSandbox",
        0,
        package::Schedule::default(),
    );
    let sandbox = package::close_header(sandbox, Some(SANDBOX_TEMPLATE));
    let sandbox = package::with_location_input(sandbox, 0, package::Location::NearSelf, 200);
    plugin.push_top(package::with_actions(package::finish(sandbox)));
    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    let geometry = Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(CELL),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [FAR, 0.0, 0.0],
            [FAR, 1000.0, 0.0],
            [0.0, 1000.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, 1],
                flags: 0,
                cover_flags: 0,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [FAR, 1000.0],
            min: [0.0; 3],
            max: [FAR, 1000.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    };
    let records = [
        placed(*b"REFR", MARKER, BASE, [FAR - 100.0, 500.0, 0.0]),
        Record::new(T(b"NAVM"), NAV).sub(T(b"NVNM"), geometry.to_bytes()),
    ];
    for record in records {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    for floor in 1..=3 {
        let geometry = lookup_floor(floor as f32 * 10_000.0);
        temporary.entries.push(Entry::Record(plugin.push_record(
            Record::new(T(b"NAVM"), NAV + 0x100 + floor).sub(T(b"NVNM"), geometry.to_bytes()),
        )));
    }
    for n in 0..count {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(placed(
                *b"ACHR",
                ACTOR + n as u32,
                if n % 2 == 0 { BASE } else { SANDBOX_BASE },
                [10.0, 100.0 + (n % 800) as f32, 0.0],
            ))));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block
        .entries
        .extend([Entry::Record(cell), Entry::Group(children)]);
    plugin.groups.push(block);
    let mut order = LoadOrder::new();
    order.add("Stress.esm", plugin).map_err(|e| e.to_string())?;
    let mut game = Game::new(order);
    let ids: Vec<_> = (0..count)
        .map(|n| GlobalFormId {
            file: 0,
            object: ACTOR + n as u32,
        })
        .collect();
    for &id in &ids {
        if !game.world_mut().drive_actor(id) {
            return Err(format!("could not drive actor {id}"));
        }
    }
    soe_game::ai::nav_graph(&mut game.world_mut());
    let shared = game.shared_world();
    for &id in &ids {
        if shared
            .borrow_mut()
            .evaluate_package(id, &mut |name, class| game.vm.create_object_of(name, class))
            .is_none()
        {
            return Err(format!("could not select a package for actor {id}"));
        }
    }
    Ok((game, ids))
}

fn lookup_floor(height: f32) -> Geometry {
    let mut vertices = Vec::with_capacity(40_000);
    let mut triangles = Vec::with_capacity(20_000);
    for x in 0..100 {
        for y in 0..100 {
            let v = vertices.len() as u16;
            let t = triangles.len() as i16;
            let left = x as f32 * FAR / 100.0;
            let right = (x + 1) as f32 * FAR / 100.0;
            let bottom = y as f32 * 10.0;
            vertices.extend([
                [left, bottom, height],
                [right, bottom, height],
                [right, bottom + 10.0, height],
                [left, bottom + 10.0, height],
            ]);
            triangles.extend([
                Triangle {
                    vertices: [v, v + 1, v + 2],
                    edges: [-1, -1, t + 1],
                    flags: 0,
                    cover_flags: 0,
                },
                Triangle {
                    vertices: [v, v + 2, v + 3],
                    edges: [t, -1, -1],
                    flags: 0,
                    cover_flags: 0,
                },
            ]);
        }
    }
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(CELL),
        vertices,
        triangles,
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [FAR, 1000.0],
            min: [0.0, 0.0, height],
            max: [FAR, 1000.0, height],
            cells: vec![(0..20_000).collect()],
        },
    }
}

pub(super) fn navigation() -> NavGraph {
    let vertices = (0..=128)
        .flat_map(|x| [[x as f32 * 100.0, 0.0, 0.0], [x as f32 * 100.0, 100.0, 0.0]])
        .collect();
    let triangles: Vec<_> = (0..128)
        .flat_map(|x| {
            [
                [2 * x, 2 * x + 2, 2 * x + 1],
                [2 * x + 1, 2 * x + 2, 2 * x + 3],
            ]
        })
        .collect();
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &triangles);
    graph
}

pub(super) fn mesh(seed: u64) -> MeshData {
    let mut mesh = MeshData::default();
    let height = (seed % 31) as f32;
    for x in 0..64 {
        let base = mesh.positions.len() as u32;
        mesh.positions.extend([
            [x as f32, 0.0, height],
            [x as f32 + 1.0, 0.0, height],
            [x as f32, 1.0, height],
        ]);
        mesh.indices.extend([base, base + 1, base + 2]);
    }
    mesh
}

pub(super) fn query(graph: &NavGraph, seed: u64, tick: u64, query: usize) -> Result<usize, String> {
    let offset = (seed.wrapping_add(tick).wrapping_add(query as u64) % 64) as u32;
    let path = graph
        .find_path(NodeId::new(0, offset), NodeId::new(0, 255 - offset))
        .ok_or("connected stress graph had no path")?;
    if path.nodes.len() < 128 {
        return Err("navigation workload did not traverse its corridor".into());
    }
    Ok(path.nodes.len())
}
