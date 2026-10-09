// SPDX-License-Identifier: GPL-3.0-or-later

//! RND-27 city coverage and parent LOD from the local UESP WRLD definition.

use super::*;
use soe_esm::records::wrld::{flags, World};
use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin};

fn fixture(small: bool, parent_flags: u16) -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    for world in [0x100u32, 0x200] {
        let mut record = Record::new(tag::WRLD, world)
            .zsub(tag::EDID, if world == 0x100 { "Parent" } else { "City" });
        if world == 0x200 {
            record = record
                .sub(b"DATA", [u8::from(small)])
                .sub(b"WNAM", 0x100u32.to_le_bytes())
                .sub(b"PNAM", parent_flags.to_le_bytes());
        }
        top.entries.push(Entry::Record(plugin.push_record(record)));
        let mut children = Group::new(GroupKind::WorldChildren(world));
        children.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::CELL, world + 1)
                    .sub(b"DATA", [0, 0])
                    .sub(b"XCLC", [0; 12]),
            ),
        ));
        let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
        let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
        for (offset, x, y) in [(2, 0i32, 0i32), (3, 3, 0), (4, -2, -1)] {
            let id = world + offset;
            sub.entries.push(Entry::Record(
                plugin.push_record(
                    Record::new(tag::CELL, id)
                        .zsub(tag::EDID, &format!("Cell{id}"))
                        .sub(b"DATA", [0, 0])
                        .sub(b"XCLC", [x.to_le_bytes(), y.to_le_bytes(), [0; 4]].concat()),
                ),
            ));
            if world == 0x100 {
                let mut cell = Group::new(GroupKind::CellChildren(id));
                let mut temporary = Group::new(GroupKind::CellTemporary(id));
                temporary.entries.push(Entry::Record(
                    plugin.push_record(
                        Record::new(tag::LAND, id + 0x1000).sub(
                            b"VHGT",
                            soe_esm::records::land::Heights::encode(
                                &vec![0.0; soe_esm::records::land::VERTS],
                            )
                            .to_bytes(),
                        ),
                    ),
                ));
                cell.entries.push(Entry::Group(temporary));
                sub.entries.push(Entry::Group(cell));
            }
        }
        block.entries.push(Entry::Group(sub));
        children.entries.push(Entry::Group(block));
        top.entries.push(Entry::Group(children));
    }
    plugin.groups.push(top);
    let mut interiors = Group::new(GroupKind::Top(tag::CELL));
    interiors.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(tag::CELL, 0x50)
                .zsub(tag::EDID, "Cell515")
                .sub(b"DATA", [1, 0]),
        ),
    ));
    plugin.groups.push(interiors);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in &BASE_MASTERS[1..] {
        std::fs::write(
            directory.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory.path()).unwrap();
    (directory, source)
}

#[test]
fn rnd_27_small_world_loads_all_grid_cells_and_persistent_cell() {
    let (_directory, source) = fixture(true, 3);
    let cell = source.resolve_cell("Cell514").unwrap();
    let city = source.resolve_world("City").unwrap().0;
    let cells = source.exterior_cells_at(city, (0, 0), 0);
    assert_eq!(
        source
            .resolve_exterior_block("Cell514", 0)
            .unwrap()
            .cells
            .len(),
        1
    );
    assert_eq!(
        cells
            .iter()
            .map(|cell| (cell.x, cell.y))
            .collect::<Vec<_>>(),
        [(-2, -1), (0, 0), (3, 0)]
    );
    let loaded = source.load_location(cell, [0.0; 3], 0).unwrap();
    assert_eq!(loaded.cells.len(), 4);
    assert!(loaded.cells.iter().any(|(id, _)| id.object == 0x201));
    assert_eq!(
        source
            .exterior_cells_at(city, (3, 0), 0)
            .iter()
            .map(|cell| cell.id)
            .collect::<Vec<_>>(),
        cells.iter().map(|cell| cell.id).collect::<Vec<_>>()
    );
}

#[test]
fn rnd_27_exterior_selection_ignores_a_shared_interior_editor_id() {
    let (_directory, source) = fixture(true, 3);
    assert_eq!(source.resolve_cell("Cell515").unwrap().object, 0x50);
    let block = source.resolve_exterior_block("Cell515", 0).unwrap();
    assert_eq!(block.world.object, 0x200);
    assert_eq!(
        block
            .cells
            .iter()
            .map(|cell| cell.id.object)
            .collect::<Vec<_>>(),
        [0x203]
    );
}

#[test]
fn rnd_27_ordinary_world_keeps_radius_and_rejects_invalid_destinations() {
    let (_directory, source) = fixture(false, 3);
    let cell = source.resolve_cell("Cell514").unwrap();
    assert_eq!(
        source
            .resolve_exterior_block("Cell514", 0)
            .unwrap()
            .cells
            .len(),
        1
    );
    assert_eq!(
        source.load_location(cell, [0.0; 3], 0).unwrap().cells.len(),
        2
    );
    assert!(source.load_location(cell, [f32::NAN; 3], 0).is_err());
    assert!(source.load_location(cell, [0.0; 3], -1).is_err());
    assert!(!source.is_small_world(GlobalFormId {
        file: 0,
        object: 0xffff
    }));
    assert!(!source.is_small_world(cell));
}

#[test]
fn rnd_27_small_world_parent_land_and_lod_flags_remain_independent() {
    for parent_flags in [0, 1, 2, 3] {
        let (_directory, source) = fixture(true, parent_flags);
        let city = source.resolve_world("City").unwrap().0;
        let loaded = source
            .load_location(source.resolve_cell("Cell514").unwrap(), [0.0; 3], 0)
            .unwrap();
        assert_eq!(loaded.cells.len(), 4);
        let land = loaded
            .cells
            .iter()
            .flat_map(|(_, cell)| &cell.surfaces)
            .filter(|surface| {
                source
                    .order
                    .winner(surface.reference)
                    .is_some_and(|record| record.tag == tag::LAND)
            })
            .count();
        assert_eq!(land, if parent_flags & 1 != 0 { 3 } else { 0 });
        assert_eq!(
            source.lod_worldspace(city).unwrap().1,
            if parent_flags & 2 != 0 {
                "Parent"
            } else {
                "City"
            }
        );
    }
}

#[test]
fn rnd_27_installed_whiterun_records_and_lod_coverage() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let owner = source.order.winner_ref(city).unwrap();
    let world = World::from_record(source.order.record(owner), true).unwrap();
    assert_ne!(world.flags & flags::SMALL_WORLD, 0);
    assert_eq!(world.parent_flags.unwrap() & 3, 3);
    let parent = source.lod_worldspace(city).unwrap();
    assert_eq!(parent.1, "Tamriel");
    let index = source.cell_index();
    let all: HashSet<_> = index
        .cells()
        .filter_map(|(cell, _)| {
            index
                .grid_of(cell)
                .filter(|grid| grid.world == city)
                .map(|_| cell)
        })
        .collect();
    let selected = source.exterior_cells_at(city, (0, 0), 2);
    assert!(all.len() > 9);
    let actual: HashSet<_> = selected.iter().map(|cell| cell.id).collect();
    assert!(
        actual == all,
        "extra {:?}, missing {:?}",
        actual.difference(&all).collect::<Vec<_>>(),
        all.difference(&actual).collect::<Vec<_>>()
    );
    let lod_bases: HashSet<_> = source
        .order
        .winners_of(tag::STAT)
        .filter_map(|(id, record)| {
            let model = models::model_for(record)?.to_ascii_lowercase();
            (model.contains("whiterun") && model.contains("lod")).then_some(id)
        })
        .collect();
    let gate = crate::streaming::CellCoord::new(5, -3);
    let settings = source.lod_settings(&parent.1).unwrap().unwrap();
    let mut checked = 0;
    let mut far = HashSet::new();
    for (id, record) in source.order.winners_of(tag::REFR) {
        let owner = source.order.winner_ref(id).unwrap();
        let Some(base) = record
            .find(Tag::new(b"NAME"))
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
        else {
            continue;
        };
        if !lod_bases.contains(&source.order.resolve(owner.file, base)) {
            continue;
        }
        let refr = Refr::from_record(record).unwrap();
        let Some(cell) = refr.cell.map(|cell| source.order.resolve(owner.file, cell)) else {
            continue;
        };
        let Some(grid) = index.grid_of(cell).filter(|grid| grid.world == parent.0) else {
            continue;
        };
        checked += 1;
        let coordinate = crate::streaming::CellCoord::new(grid.x, grid.y);
        if coordinate.ring_distance(gate) > 2 {
            let block = crate::terrain_lod::LodBlockCoord::containing_from(
                coordinate,
                crate::terrain_lod::LodLevel::L4,
                settings.sw_cell,
            );
            assert!(
                block.ring_distance(gate) > 2,
                "city placement {id} falls in an excluded near block"
            );
            far.insert(block);
        }
        assert!(
            source
                .vfs
                .read(&format!(
                    "meshes/{}",
                    models::model_for(
                        source
                            .order
                            .winner(source.order.resolve(owner.file, refr.base))
                            .unwrap()
                    )
                    .unwrap()
                ))
                .is_ok(),
            "city LOD model for {id}"
        );
    }
    assert!(
        checked >= 18,
        "full city LOD placement survey, found {checked}"
    );
    assert!(
        !far.is_empty(),
        "city skyline includes distant castle cells"
    );
    for block in far {
        let loaded = source
            .load_object_lod_block(parent.0, &parent.1, block)
            .unwrap()
            .unwrap_or_else(|| panic!("missing city BTO {block:?}"));
        assert!(loaded.triangles > 0, "city BTO {block:?} is empty");
    }
    eprintln!(
        "RND-27 city grid={} Tamriel city LOD placements={checked}",
        all.len()
    );
}
