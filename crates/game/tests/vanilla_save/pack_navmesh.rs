// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use soe_save::ChangeForm;

fn world_and_save() -> (World, Save) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(soe_game::package::build_record(
        0x840,
        "SavedPack",
        0,
        soe_game::package::Schedule::default(),
    ));
    plugin.push_top(
        Record::new(Tag(*b"NAVM"), 0x841)
            .zsub(Tag(*b"EDID"), "SavedNavmesh")
            .zsub(Tag(*b"FULL"), "Original navmesh"),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let world = World::new(order);
    let mut save = save();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.light_plugins.clear();
    save.change_forms = vec![
        ChangeForm {
            form: reference(0x400840),
            change_flags: 1 | (1 << 30) | (1 << 31),
            type_field: 32,
            version: 78,
            uncompressed_len: 0,
            data: vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6],
        },
        ChangeForm {
            form: reference(0x400841),
            change_flags: 1 | (1 << 1) | (1 << 2),
            type_field: 33,
            version: 78,
            uncompressed_len: 0,
            data: vec![0x17, 0x28, 0x39, 0x4A, 0x5B, 0x6C],
        },
    ];
    (world, save)
}

#[test]
fn sav_48_pack_navmesh_restore_is_atomic_and_survives_engine_save_load() {
    let (mut world, save) = world_and_save();
    let pack_id = id(0, 0x840);
    let navm_id = id(0, 0x841);

    assert_eq!(world.restore_vanilla_pack_navmesh(&save).unwrap(), 2);
    assert!(soe_game::package::Package::from_record(world.record(pack_id).unwrap()).is_ok());
    let package = soe_game::ai::package(&world, pack_id).unwrap();
    assert!(package.saved_waiting);
    assert!(package.saved_never_run);
    assert_eq!(
        world.restored_pack_extra(pack_id),
        Some(soe_game::world::PackRecordExtra {
            form_flags: Some((0xB4A3_9281, 0xD6C5)),
            waiting: true,
            never_run: true,
        })
    );
    assert_eq!(world.name_of(navm_id).as_deref(), Some("Original navmesh"));
    assert_eq!(
        world.restored_navmesh_extra(navm_id),
        Some(soe_game::world::NavmeshRecordExtra {
            form_flags: Some((0x4A39_2817, 0x6C5B)),
            value_changed: true,
            full_name_changed: true,
        })
    );

    let bytes = world.save_state();
    let (mut loaded, _) = world_and_save();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(
        loaded.restored_pack_extra(pack_id),
        world.restored_pack_extra(pack_id)
    );
    assert_eq!(
        loaded.restored_navmesh_extra(navm_id),
        world.restored_navmesh_extra(navm_id)
    );
    assert!(soe_game::package::Package::from_record(loaded.record(pack_id).unwrap()).is_ok());
    let package = soe_game::ai::package(&loaded, pack_id).unwrap();
    assert!(package.saved_waiting);
    assert!(package.saved_never_run);
    assert_eq!(loaded.name_of(navm_id).as_deref(), Some("Original navmesh"));

    let (mut atomic, mut malformed) = world_and_save();
    malformed.change_forms[1].data.pop();
    assert!(atomic.restore_vanilla_pack_navmesh(&malformed).is_err());
    assert!(atomic.restored_pack_extra(pack_id).is_none());
    assert!(atomic.restored_navmesh_extra(navm_id).is_none());
}

#[test]
fn prf_36_cached_package_reads_live_restored_markers_without_saving_definitions() {
    let (mut world, save) = world_and_save();
    let pack = id(0, 0x840);
    let before = world.save_state();
    let original = soe_game::ai::package(&world, pack).unwrap();
    assert!(!original.saved_waiting && !original.saved_never_run);
    assert_eq!(world.save_state(), before);
    world.restore_vanilla_pack_navmesh(&save).unwrap();
    let restored = soe_game::ai::package(&world, pack).unwrap();
    assert!(restored.saved_waiting && restored.saved_never_run);
}

#[test]
fn sav_48_installed_pack_and_navmesh_records_accept_restored_markers() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping installed PACK/NAVM restore");
        return;
    };
    let plugin =
        Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let pack = order
        .winners_of(Tag(*b"PACK"))
        .next()
        .map(|(id, _)| id)
        .expect("Skyrim.esm contains PACK records");
    let navmesh = order
        .winners_of(Tag(*b"NAVM"))
        .next()
        .map(|(id, _)| id)
        .expect("Skyrim.esm contains NAVM records");
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = vec![
        ChangeForm {
            form: reference(0x400000 | pack.object),
            change_flags: 1 << 30,
            type_field: 32,
            version: 78,
            uncompressed_len: 0,
            data: vec![],
        },
        ChangeForm {
            form: reference(0x400000 | navmesh.object),
            change_flags: 1 << 2,
            type_field: 33,
            version: 78,
            uncompressed_len: 0,
            data: vec![],
        },
    ];
    let mut world = World::new(order);
    assert_eq!(world.restore_vanilla_pack_navmesh(&saved).unwrap(), 2);
    assert!(world.restored_pack_extra(pack).unwrap().waiting);
    assert!(
        world
            .restored_navmesh_extra(navmesh)
            .unwrap()
            .full_name_changed
    );
}
