// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-17 adversarial enable-parent checks from UESP REFR XESP and CK Enable.

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::{Game, World};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode, Value,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(object: u32, record_flags: u32, parent: Option<(u32, u32)>) -> Record {
    let mut record = Record::new(tag::REFR, object)
        .with_flags(record_flags)
        .sub(b"NAME", 0x800_u32.to_le_bytes())
        .sub(b"DATA", [0; 24]);
    if let Some((parent, parent_flags)) = parent {
        record = record.sub(
            b"XESP",
            [parent.to_le_bytes(), parent_flags.to_le_bytes()].concat(),
        );
    }
    record
}

fn order(records: impl IntoIterator<Item = Record>) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag::STAT, 0x800));
    for record in records {
        plugin.push_top(record);
    }
    let mut load_order = LoadOrder::new();
    load_order.add("Skyrim.esm", plugin).unwrap();
    load_order
}

fn world(records: impl IntoIterator<Item = Record>) -> World {
    World::new(order(records))
}

fn papyrus_call(game: &mut Game, target: GlobalFormId, method: &str) {
    let script = format!("Rnd17Adversarial{method}");
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "ObjectReference")
                .ins(
                    Opcode::CallMethod,
                    vec![s(method), ident("target"), ident("None")],
                )
                .ins(Opcode::Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(target);
    let thread = game
        .vm
        .call_global(&script, "Run", &[Value::Object(object)])
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn rnd_17_papyrus_enable_disable_recomputes_unloaded_descendants() {
    let mut game = Game::new(order([
        placed(0x900, flags::INITIALLY_DISABLED, None),
        placed(0x901, 0, Some((0x900, 0))),
    ]));
    assert!(!game.world().reference_enabled(id(0x901)));
    papyrus_call(&mut game, id(0x900), "Enable");
    assert!(game.world().reference_enabled(id(0x901)));
    assert!(game.world().reference_ref(id(0x901)).is_none());
    papyrus_call(&mut game, id(0x900), "Disable");
    assert!(!game.world().reference_enabled(id(0x901)));
}

#[test]
fn rnd_17_opposite_flag_xors_at_each_parent_and_ignores_other_bits() {
    let mut world = world([
        placed(0x900, 0, None),
        placed(0x901, 0, Some((0x900, 0xFFFF_FFFE))),
        placed(0x902, 0, Some((0x901, 0xFFFF_FFFF))),
    ]);
    assert!(world.reference_enabled(id(0x901)));
    assert!(!world.reference_enabled(id(0x902)));
    world.set_reference_enabled(id(0x900), false);
    assert!(!world.reference_enabled(id(0x901)));
    assert!(world.reference_enabled(id(0x902)));
}

#[test]
fn rnd_17_bad_deleted_and_cyclic_parent_chains_are_hidden() {
    let world = world([
        placed(0x900, flags::DELETED, None),
        placed(0x901, 0, Some((0x900, 0))),
        placed(0x902, 0, Some((0x999, 0))),
        placed(0x903, 0, Some((0x904, 0))),
        placed(0x904, 0, Some((0x903, 1))),
        placed(0x905, 0, None).sub(b"XESP", [1, 2, 3]),
        placed(0x906, 0, Some((0, 0))),
    ]);
    for object in 0x901..=0x906 {
        assert!(!world.reference_enabled(id(object)), "{object:06X}");
    }
}

#[test]
fn rnd_17_disabled_parent_state_round_trips_and_repeated_calls_are_idempotent() {
    let mut world = world([
        placed(0x900, flags::INITIALLY_DISABLED, None),
        placed(0x901, 0, Some((0x900, 0))),
    ]);
    for _ in 0..3 {
        world.set_reference_enabled(id(0x900), true);
        assert!(world.reference_enabled(id(0x901)));
        world.set_reference_enabled(id(0x900), false);
        assert!(!world.reference_enabled(id(0x901)));
    }
    let bytes = world.save_state();
    let mut restored = self::world([
        placed(0x900, flags::INITIALLY_DISABLED, None),
        placed(0x901, 0, Some((0x900, 0))),
    ]);
    restored.load_state(&bytes).unwrap();
    assert!(!restored.reference_enabled(id(0x901)));
}

#[test]
fn rnd_17_parent_form_ids_resolve_in_the_winning_plugin_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(placed(0x900, 0, None));
    base.push_top(placed(0x901, 0, Some((0x900, 0))));
    let mut update = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    update.push_top(placed(0x0100_0900, flags::INITIALLY_DISABLED, None));
    update.push_top(placed(0x901, 0, Some((0x0100_0900, 0))));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Update.esm", update).unwrap();
    assert!(!World::authored_reference_enabled(&order, id(0x901)));
    let world = World::new(order);
    assert!(!world.reference_enabled(id(0x901)));
}

#[test]
fn rnd_17_installed_whiterun_barricade_streams_after_marker_enable() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source =
        soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let order = source.shared_order();
    let (whiterun, _) = source.resolve_world("WhiterunWorld").unwrap();
    let mut candidate = None;
    for (id, record) in order.winners_of(tag::REFR) {
        let Some(owner) = order.winner_ref(id) else {
            continue;
        };
        let Some(cell) = record
            .placement
            .cell
            .map(|raw| order.resolve(owner.file, raw))
        else {
            continue;
        };
        let Some(cell_owner) = order.winner_ref(cell) else {
            continue;
        };
        let cell_record = order.record(cell_owner);
        if cell_record
            .placement
            .world
            .map(|raw| order.resolve(cell_owner.file, raw))
            != Some(whiterun)
        {
            continue;
        }
        let Some(base_raw) = record
            .find(soe_esm::Tag::new(b"NAME"))
            .and_then(|bytes| bytes.get(..4))
        else {
            continue;
        };
        let base = order.resolve(owner.file, u32::from_le_bytes(base_raw.try_into().unwrap()));
        if !order
            .winner(base)
            .and_then(|r| r.editor_id())
            .is_some_and(|name| name.to_ascii_lowercase().contains("barricade"))
        {
            continue;
        }
        let Some(parent_data) = record
            .find(soe_esm::Tag::new(b"XESP"))
            .filter(|data| data.len() >= 8)
        else {
            continue;
        };
        if parent_data[4] & 1 != 0 {
            continue;
        }
        let marker = order.resolve(
            owner.file,
            u32::from_le_bytes(parent_data[..4].try_into().unwrap()),
        );
        if order
            .winner(marker)
            .is_some_and(|r| r.flags & flags::INITIALLY_DISABLED != 0)
        {
            candidate = Some((id, cell, marker));
            break;
        }
    }
    let (barricade, cell, marker) = candidate.expect("Whiterun disabled-parent barricade");
    let barricade_record = order.winner(barricade).unwrap();
    let placement = barricade_record.find(soe_esm::Tag::new(b"DATA")).unwrap();
    let cell_record = order.winner(cell).unwrap();
    let coordinate = cell_record.find(soe_esm::Tag::new(b"XCLC")).unwrap();
    let field = |data: &[u8], offset: usize| {
        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    };
    let manifest = format!(
        "{{\"reference\":{},\"marker\":{},\"file\":{},\"cell\":\"{}\",\"x\":{},\"y\":{},\"px\":{},\"py\":{},\"pz\":{}}}",
        barricade.object,
        marker.object,
        barricade.file,
        cell_record.editor_id().unwrap(),
        field(coordinate, 0) as i32,
        field(coordinate, 4) as i32,
        f32::from_bits(field(placement, 0)),
        f32::from_bits(field(placement, 4)),
        f32::from_bits(field(placement, 8)),
    );
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
        });
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("rnd_17_whiterun.json"), manifest).unwrap();
    let mut world = World::from_shared_order(order);
    assert!(!world.reference_enabled(barricade));
    let streamed = source.stream_cell_id(cell).unwrap();
    assert!(!streamed
        .surfaces
        .iter()
        .any(|surface| surface.reference == barricade));
    let colliders = source.collide_cell_id(cell).unwrap();
    assert!(!colliders
        .references
        .values()
        .any(|reference| *reference == barricade));
    world.set_reference_enabled(marker, true);
    assert!(world.reference_enabled(barricade));
    assert!(!source
        .stream_reference(barricade)
        .unwrap()
        .surfaces
        .is_empty());
    assert!(source
        .placed_reference_colliders(barricade)
        .unwrap()
        .iter()
        .any(|placed| placed.collider.is_static()));
}
