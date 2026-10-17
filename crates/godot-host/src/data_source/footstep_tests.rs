// SPDX-License-Identifier: GPL-3.0-or-later

//! Footstep reproduction through the free walk location collision builder.

use super::*;
use soe_game::footsteps::{Footsteps, Gait};
use soe_physics::Shape;
use std::collections::BTreeSet;

fn painted_fixture(overlay: bool) -> (tempfile::TempDir, DataSource) {
    use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin};
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let names = ["Dirt", "Grass", "Stone", "Wood", "Snow", "Water"];
    for (i, name) in names.iter().enumerate() {
        plugin.push_top(Record::new(Tag(*b"MATT"), 0x100 + i as u32).zsub(b"MNAM", name));
        plugin.push_top(
            Record::new(tag::LTEX, 0x200 + i as u32).sub(b"MNAM", (0x100 + i as u32).to_le_bytes()),
        );
    }
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    top.entries
        .push(Entry::Record(plugin.push_record(Record::new(tag::WRLD, 1))));
    let mut children = Group::new(GroupKind::WorldChildren(1));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
    for x in 0_i32..2 {
        let cell = 10 + x as u32;
        sub.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::CELL, cell)
                    .sub(b"DATA", [0, 0])
                    .sub(b"XCLC", [x.to_le_bytes(), [0; 4], [0; 4]].concat()),
            ),
        ));
        let mut land = Record::new(tag::LAND, 20 + x as u32).sub(
            b"VHGT",
            soe_esm::records::land::Heights::encode(&vec![0.0; 1089]).to_bytes(),
        );
        for quadrant in 0_u8..4 {
            let index = (x as usize * 4 + usize::from(quadrant)) % names.len();
            let mut btxt = (0x200 + index as u32).to_le_bytes().to_vec();
            btxt.extend([quadrant, 0, 0, 0]);
            land = land.sub(b"BTXT", btxt);
        }
        if overlay {
            let mut atxt = 0x205_u32.to_le_bytes().to_vec();
            atxt.extend([0, 0, 0, 0]);
            let mut vtxt = Vec::new();
            for position in 0_u16..289 {
                vtxt.extend(position.to_le_bytes());
                vtxt.extend([0, 0]);
                vtxt.extend(1.0_f32.to_le_bytes());
            }
            land = land.sub(b"ATXT", atxt).sub(b"VTXT", vtxt);
        }
        let mut temporary = Group::new(GroupKind::CellTemporary(cell));
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(land)));
        let mut cell_group = Group::new(GroupKind::CellChildren(cell));
        cell_group.entries.push(Entry::Group(temporary));
        sub.entries.push(Entry::Group(cell_group));
    }
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    top.entries.push(Entry::Group(children));
    plugin.groups.push(top);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for master in &BASE_MASTERS[1..] {
        std::fs::write(
            directory.path().join(master),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory.path()).unwrap();
    (directory, source)
}

#[test]
fn r4_walk_location_rays_keep_six_painted_land_materials() {
    use soe_physics::{PhysicsWorld, Vec3};
    let (_directory, source) = painted_fixture(false);
    let names = ["Dirt", "Grass", "Stone", "Wood", "Snow", "Water"];
    for (i, name) in names.iter().enumerate() {
        let x = (i / 4) as f32 * 4096.0 + (i % 2) as f32 * 2048.0 + 512.0;
        let y = ((i % 4) / 2) as f32 * 2048.0 + 512.0;
        let cell = GlobalFormId {
            file: 0,
            object: 10 + (i / 4) as u32,
        };
        let location = source.load_location(cell, [x, y, 0.0], 0).unwrap();
        let hit = location
            .collision
            .world
            .raycast(Vec3::new(x, y, 8.0), Vec3::new(x, y, -40.0))
            .unwrap();
        assert_eq!(
            hit.material,
            Some(soe_game::footsteps::material_name_hash(name.as_bytes())),
            "{name} painted terrain returned the wrong material"
        );
    }
}

#[test]
fn r4_installed_whiterun_land_retains_surface_materials_and_descriptors() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, installed material check skipped");
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).unwrap();
    let cell = source.resolve_cell("WhiterunOrigin").unwrap();
    let location = source.load_location(cell, [0.0; 3], 0).unwrap();
    let steps = Footsteps::new(source.order());
    let set = steps.default_object(*b"DFTS").unwrap().unwrap();
    let mut materials_found = BTreeSet::new();
    let mut descriptors = BTreeSet::new();
    let mut triangles_found = 0;
    for placed in &location.collision.placed {
        if source.order().winner(placed.reference).unwrap().tag != tag::LAND {
            continue;
        }
        let Shape::TriangleMesh {
            triangles,
            materials,
            ..
        } = &placed.collider.shape
        else {
            panic!("LAND collision must retain its mesh");
        };
        triangles_found += triangles.len();
        assert_eq!(
            materials.len(),
            triangles.len(),
            "LAND {} lost its LTEX material",
            placed.reference
        );
        for hash in materials.iter().flatten().copied().collect::<BTreeSet<_>>() {
            let material = steps.material(hash).unwrap().unwrap();
            materials_found.insert(material);
            let impacts = steps
                .impacts(set, Gait::Walk, "FootLeft", material)
                .unwrap();
            for impact in impacts {
                for sound in impact.sounds.into_iter().flatten() {
                    eprintln!(
                        "R4 FOOTSTEP land={} material={} descriptor={}",
                        placed.reference, material, sound
                    );
                    descriptors.insert(sound);
                }
            }
        }
    }
    assert!(triangles_found > 0, "Whiterun must exercise LAND collision");
    assert!(
        materials_found.len() >= 2,
        "Whiterun must retain at least two painted materials: {materials_found:?}"
    );
    assert!(
        descriptors.len() >= 2,
        "painted materials must select different descriptors"
    );
}

#[test]
fn r4_alpha_paint_replaces_base_collision_material() {
    use soe_physics::{PhysicsWorld, Vec3};
    let (_directory, source) = painted_fixture(true);
    let location = source
        .load_location(
            GlobalFormId {
                file: 0,
                object: 10,
            },
            [512.0, 512.0, 0.0],
            0,
        )
        .unwrap();
    let hit = location
        .collision
        .world
        .raycast(Vec3::new(512.0, 512.0, 8.0), Vec3::new(512.0, 512.0, -40.0))
        .unwrap();
    assert_eq!(
        hit.material,
        Some(soe_game::footsteps::material_name_hash(b"Water"))
    );
}
