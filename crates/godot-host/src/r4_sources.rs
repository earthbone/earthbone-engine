// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed furniture and water evidence from xEdit fields and nifly extra data.

use soe_esm::Tag;

#[test]
fn r4_furniture_and_water_source_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let order = source.order();
    let mut world = soe_game::World::from_shared_order(source.shared_order());
    let player = world.player();
    for raw in [0x3b071, 0x2f793, 0xcae0e] {
        let mut id = Some(soe_esm::GlobalFormId {
            file: 0,
            object: raw,
        });
        while let Some(current) = id {
            let Some(record) = world.record(current) else {
                break;
            };
            println!(
                "IDLE_CHAIN {raw:x} {current} {:?} {:?}",
                record.editor_id(),
                record.all(Tag(*b"CTDA")).collect::<Vec<_>>()
            );
            id = record
                .find(Tag(*b"ANAM"))
                .and_then(|data| data.get(..4))
                .and_then(|data| {
                    let raw = u32::from_le_bytes(data.try_into().ok()?);
                    (raw != 0)
                        .then(|| world.resolve_from(current, raw))
                        .flatten()
                });
        }
    }
    for name in [
        "WhiterunBanneredMare",
        "WarehouseFurniture",
        "WarehouseCrafting",
        "testL",
    ] {
        let cell = source.resolve_cell(name).unwrap();
        let record = order.winner(cell).unwrap();
        println!(
            "CELL {name} {cell} {:?}",
            record
                .subrecords
                .iter()
                .map(|s| (s.tag, &s.data))
                .collect::<Vec<_>>()
        );
        for &id in source.cell_index().references(cell) {
            let owner = order.winner_ref(id).unwrap();
            let record = order.record(owner);
            if record.tag != Tag(*b"REFR") {
                continue;
            }
            let Ok(reference) = soe_esm::records::refr::Refr::from_record(record) else {
                continue;
            };
            let base = order.resolve(owner.file, reference.base);
            let Some(record) = order.winner(base).filter(|r| r.tag == Tag(*b"FURN")) else {
                continue;
            };
            println!(
                "FURN {id} base={base} transform={:?} fields={:?}",
                reference.transform,
                record
                    .subrecords
                    .iter()
                    .filter(|s| [
                        Tag(*b"EDID"),
                        Tag(*b"MODL"),
                        Tag(*b"MNAM"),
                        Tag(*b"FNPR"),
                        Tag(*b"KNAM"),
                        Tag(*b"FNMK")
                    ]
                    .contains(&s.tag))
                    .map(|s| (s.tag, &s.data))
                    .collect::<Vec<_>>()
            );
            if let Some(model) = record.find(Tag(*b"MODL")) {
                let model = soe_esm::cursor::zstring(model);
                let path = format!("meshes/{}", model.replace('\\', "/"));
                let bytes = source.vfs().read(&path).unwrap();
                let nif = soe_nif::parse(&bytes).unwrap();
                let animation =
                    soe_nif::object_animation::ObjectAnimation::from_file(&nif).unwrap();
                println!(
                    "OBJECT {path} sequences={:?} animated={}",
                    animation.sequence_names().collect::<Vec<_>>(),
                    animation.is_animated()
                );
                for block in &nif.blocks {
                    if let soe_nif::blocks::Block::Unknown { type_name, data } = block {
                        if type_name.contains("Furniture") {
                            println!("MARKERS {path} {type_name} {data:?}");
                        }
                        if type_name == "BSBehaviorGraphExtraData" {
                            let index = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
                            println!("OBJECT_GRAPH {path} {}", nif.header.strings[index]);
                        }
                    }
                }
            }
            for animation in [1, 2, 4] {
                if world
                    .reserve_furniture_animation_marker(id, player, animation)
                    .is_some()
                {
                    match crate::furniture::prepare(&mut world, source.vfs(), player) {
                        Ok(Some(use_)) => println!(
                            "ENTRY {name} {id} type={animation} event={} position={:?} heading={}",
                            use_.event, use_.position, use_.heading
                        ),
                        other => println!(
                            "ENTRY_ERROR {name} {id} type={animation} {}",
                            match other {
                                Err(error) => error,
                                _ => "not ready".to_owned(),
                            }
                        ),
                    }
                    world.stand_from_furniture(player);
                    break;
                }
            }
        }
    }
    for (id, record) in order.winners_of(Tag(*b"IDLE")) {
        if let Some(name) = record.editor_id().filter(|n| {
            n.contains("Chair")
                || n.contains("Blacksmith")
                || n.contains("Alchemy")
                || n.contains("Cooking")
                || n.contains("Bed")
                || n.contains("Furniture")
        }) {
            println!(
                "IDLE {id} {name} event={:?} parent={:?} conditions={:?}",
                record.find(Tag(*b"ENAM")).map(soe_esm::cursor::zstring),
                record.find(Tag(*b"ANAM")),
                record.all(Tag(*b"CTDA")).collect::<Vec<_>>()
            );
        }
    }
    water_fixtures(&source);
}

fn water_fixtures(source: &crate::data_source::DataSource) {
    use soe_physics::{PhysicsWorld, Vec3};
    let mut fixtures = serde_json::Map::new();
    let cell = source.resolve_cell("testL").unwrap();
    let (height, water) = source
        .cell_water(cell)
        .expect("testL must have recorded water");
    let collision = source.collide_cell_id(cell).unwrap();
    let positions: Vec<_> = source
        .cell_index()
        .references(cell)
        .iter()
        .filter_map(|id| {
            let record = source.order().winner(*id)?;
            if record.tag != Tag(*b"REFR") {
                return None;
            }
            soe_esm::records::refr::Refr::from_record(record)
                .ok()?
                .transform
                .map(|pose| pose.position)
        })
        .collect();
    let min_x = positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = positions
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = positions
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let mut interior = None;
    'sample: for row in 1..32 {
        for col in 1..32 {
            let x = min_x + (max_x - min_x) * col as f32 / 32.0;
            let y = min_y + (max_y - min_y) * row as f32 / 32.0;
            let clear = [
                (0.0, 0.0),
                (-32.0, 0.0),
                (32.0, 0.0),
                (0.0, -32.0),
                (0.0, 32.0),
            ]
            .into_iter()
            .all(|(dx, dy)| {
                collision
                    .world
                    .raycast(
                        Vec3::new(x + dx, y + dy, height + 1000.0),
                        Vec3::new(x + dx, y + dy, height - 2000.0),
                    )
                    .is_some_and(|hit| hit.point.z < height - 160.0)
            });
            if clear {
                interior = Some([x, y, height - 100.0]);
                break 'sample;
            }
        }
    }
    let position =
        interior.expect("testL needs a 160 unit deep water sample with capsule clearance");
    fixtures.insert("testL".into(), serde_json::json!({"cell": "testL", "position": position, "water_height": height, "water": water.map(|id| id.to_string()), "radius": -1}));
    let origin = "WhiterunExterior01";
    let mut block = source.resolve_exterior_block(origin, 7).unwrap();
    block
        .cells
        .sort_by_key(|cell| (cell.x - block.origin.0).pow(2) + (cell.y - block.origin.1).pow(2));
    let mut river = None;
    'cells: for cell in &block.cells {
        let Some((height, water)) = source.cell_water(cell.id) else {
            continue;
        };
        let Some(land) = source
            .cell_index()
            .land(cell.id)
            .and_then(|id| source.order().winner(id))
            .and_then(|record| soe_esm::records::land::Land::from_record(record).ok())
            .and_then(|land| land.heights)
        else {
            continue;
        };
        let heights = land.decode();
        for row in 1..32 {
            for col in 1..32 {
                if [
                    heights[row * 33 + col],
                    heights[row * 33 + col - 1],
                    heights[row * 33 + col + 1],
                    heights[(row - 1) * 33 + col],
                    heights[(row + 1) * 33 + col],
                ]
                .into_iter()
                .all(|floor| floor < height - 160.0)
                {
                    let position = [
                        cell.x as f32 * 4096.0 + col as f32 * 128.0,
                        cell.y as f32 * 4096.0 + row as f32 * 128.0,
                        height - 100.0,
                    ];
                    let named = source
                        .order()
                        .winner(cell.id)
                        .and_then(soe_esm::Record::editor_id);
                    river = Some(
                        serde_json::json!({"cell": named.as_deref().unwrap_or(origin), "position": position, "water_height": height, "water": water.map(|id| id.to_string()), "radius": if named.is_some() { 1 } else { 7 }}),
                    );
                    break 'cells;
                }
            }
        }
    }
    fixtures.insert(
        "Whiterun".into(),
        river.expect("Whiterun river needs a 160 unit deep LAND sample"),
    );
    let output = serde_json::to_string_pretty(&fixtures).unwrap();
    println!("WATER_FIXTURES {output}");
    std::fs::write("/tmp/r4-water-fixtures.json", output).unwrap();
}
