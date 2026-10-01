// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Handle, Pending};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const CHEST: u32 = 0x101;
const KEY: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn lock(level: u8, key: u32) -> Vec<u8> {
    let mut data = vec![0; 20];
    data[0] = level;
    data[4..8].copy_from_slice(&key.to_le_bytes());
    data
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"CONT"), 0x11),
        Record::new(T(b"KEYM"), KEY),
        Record::new(T(b"KEYM"), KEY + 1),
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock(255, KEY)),
        Record::new(T(b"REFR"), CHEST)
            .sub(T(b"NAME"), 0x11_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock(0, KEY)),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn events(game: &Game) -> Vec<String> {
    game.world()
        .pending
        .iter()
        .filter_map(|event| match event {
            Pending::Event { name, .. } => Some(name.clone()),
            Pending::Call { .. }
            | Pending::CallScript { .. }
            | Pending::BindEffect { .. }
            | Pending::BindReference { .. }
            | Pending::DeferredDeath { .. }
            | Pending::ActorDeath { .. } => None,
        })
        .collect()
}

fn call(game: &mut Game, target: GlobalFormId, name: &str, args: &[Value]) -> Value {
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "ObjectReference")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for (index, arg) in args.iter().enumerate() {
        let name = format!("arg{index}");
        fun = fun.param(
            &name,
            if matches!(arg, Value::Bool(_)) {
                "Bool"
            } else {
                "Int"
            },
        );
        operands.push(ident(&name));
    }
    let script = format!("Probe{name}");
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            fun.ins(Opcode::CallMethod, operands)
                .ins(Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of(Handle::Form(target)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn locks_load_at_zero_and_key_only_levels_and_use_the_right_inventory_key() {
    let mut game = Game::new(order());
    let player = game.player();
    for (target, level) in [(id(DOOR), 255), (id(CHEST), 0)] {
        assert!(game.world_mut().reference(target).unwrap().locked);
        assert_eq!(
            game.world_mut().reference(target).unwrap().lock_level,
            level
        );
        assert!(!game.activate_no_settle(target, player));
        assert!(!game.world_mut().reference(target).unwrap().opened);
        game.world_mut().add_item(player, id(KEY + 1), 1);
        assert!(!game.activate_no_settle(target, player));
        game.world_mut().add_item(player, id(KEY), 1);
        game.world_mut().pending.clear();
        assert!(game.activate_no_settle(target, player));
        assert_eq!(events(&game), ["OnLockStateChanged", "OnActivate"]);
        assert!(!game.world_mut().reference(target).unwrap().locked);
        assert!(game.world_mut().reference(target).unwrap().opened);
        assert_eq!(game.world_mut().item_count(player, id(KEY)), 1);
        game.world_mut().remove_item(player, id(KEY), 1, None);
    }
}

#[test]
fn blocked_activation_does_not_unlock_but_default_only_still_requires_a_key() {
    let mut game = Game::new(order());
    let player = game.player();
    game.object_of(Handle::Form(id(DOOR)));
    game.object_of(Handle::Form(player));
    game.world_mut()
        .reference(id(DOOR))
        .unwrap()
        .activation_blocked = true;
    game.world_mut().add_item(player, id(KEY), 1);
    assert!(!game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(events(&game), ["OnActivate"]);
    game.world_mut().pending.clear();
    game.world_mut().remove_item(player, id(KEY), 1, None);
    assert!(!game
        .world_mut()
        .activate(id(DOOR), player, false, &mut |_, _| panic!(
            "allocated known form"
        )));
    assert!(events(&game).is_empty());
    game.world_mut().add_item(player, id(KEY), 1);
    assert!(game
        .world_mut()
        .activate(id(DOOR), player, false, &mut |_, _| panic!(
            "allocated known form"
        )));
    assert_eq!(events(&game), ["OnLockStateChanged"]);
}

#[test]
fn locking_closes_the_object_and_repeated_calls_emit_events() {
    let mut game = Game::new(order());
    game.object_of(Handle::Form(id(DOOR)));
    game.world_mut().reference(id(DOOR)).unwrap().opened = true;
    for _ in 0..2 {
        game.world_mut()
            .set_locked(id(DOOR), true, &mut |_, _| panic!("allocated known form"));
    }
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().opened);
    assert_eq!(events(&game), ["OnLockStateChanged", "OnLockStateChanged"]);
    game.world_mut()
        .set_locked(id(DOOR), false, &mut |_, _| panic!("allocated known form"));
    game.world_mut()
        .set_lock_level(id(DOOR), 50, &mut |_, _| panic!("allocated known form"));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().reference(id(DOOR)).unwrap().lock_level, 50);
    assert_eq!(events(&game).len(), 4);
}

#[test]
fn native_queries_and_unlock_state_survive_engine_save_restore() {
    let mut game = Game::new(order());
    let player = game.player();
    let key = call(&mut game, id(DOOR), "GetKey", &[]);
    assert_eq!(game.world().form_arg(&key), Some(id(KEY)));
    assert_eq!(
        call(&mut game, id(DOOR), "GetLockLevel", &[]),
        Value::Int(255)
    );
    assert_eq!(
        call(&mut game, id(DOOR), "IsLocked", &[]),
        Value::Bool(true)
    );
    call(&mut game, id(DOOR), "SetLockLevel", &[Value::Int(75)]);
    call(&mut game, id(DOOR), "Lock", &[Value::Bool(false)]);
    let saved = game.save_state().unwrap();
    call(&mut game, id(DOOR), "Lock", &[Value::Bool(true)]);
    game.load_state(&saved).unwrap();
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().reference(id(DOOR)).unwrap().lock_level, 75);
    assert!(game.activate_no_settle(id(DOOR), player));
}

#[test]
fn key_override_uses_the_winning_files_namespace() {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    patch.push_top(Record::new(T(b"KEYM"), 0x0100_0800));
    patch.push_top(
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock(50, 0x0100_0800)),
    );
    order.add("Keys.esp", patch).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let expected = GlobalFormId {
        file: 1,
        object: 0x800,
    };
    assert_eq!(game.world().key(id(DOOR)), Some(expected));
    game.world_mut().add_item(player, id(KEY), 1);
    assert!(!game.activate_no_settle(id(DOOR), player));
    game.world_mut().add_item(player, expected, 1);
    assert!(game.activate_no_settle(id(DOOR), player));
}

#[test]
fn a_locked_load_door_requires_its_key_and_keeps_the_unlock_after_unload() {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    let from = GlobalFormId {
        file: 1,
        object: 0x300,
    };
    let to = GlobalFormId {
        file: 1,
        object: 0x301,
    };
    for raw in [0x0100_0300, 0x0100_0301] {
        patch.push_top(Record::new(T(b"CELL"), raw).sub(T(b"DATA"), [1, 0]));
    }
    let landing = soe_esm::records::Transform {
        position: [20.0, 30.0, 40.0],
        rotation: [0.0; 3],
    };
    let mut door = Record::new(T(b"REFR"), DOOR)
        .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
        .sub(T(b"XLOC"), lock(255, KEY))
        .sub(
            T(b"XTEL"),
            [
                0x0100_0101_u32.to_le_bytes().as_slice(),
                &landing.to_bytes(),
            ]
            .concat(),
        );
    door.placement.cell = Some(0x0100_0300);
    patch.push_top(door);
    let mut far = Record::new(T(b"REFR"), 0x0100_0101).sub(T(b"NAME"), 0x10_u32.to_le_bytes());
    far.placement.cell = Some(0x0100_0301);
    patch.push_top(far);
    order.add("Doors.esp", patch).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().reference(player).unwrap().cell = Some(from);
    game.world_mut().change_cell(Some(from));
    assert!(!game.activate_deferred(id(DOOR), player));
    assert_eq!(game.world().loaded_cell(), Some(from));
    game.world_mut().add_item(player, id(KEY), 1);
    assert!(game.activate_deferred(id(DOOR), player));
    assert_eq!(game.world().loaded_cell(), Some(to));
    assert_eq!(
        game.world_mut().reference(player).unwrap().position,
        landing.position
    );
    game.world_mut().settle_unload();
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(KEY)), 1);
}

#[test]
fn installed_locks_and_keys_resolve_from_every_winning_reference() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let mut game = Game::from_shared_order(order);
    let locks: Vec<_> = game
        .world()
        .order()
        .winners_of(T(b"REFR"))
        .filter(|(_, r)| !r.is_deleted())
        .filter_map(|(id, record)| {
            record
                .find(T(b"XLOC"))
                .map(|bytes| (id, soe_esm::records::refr::Lock::from_bytes(bytes).unwrap()))
        })
        .collect();
    assert!(locks.len() > 100, "installed lock survey is too narrow");
    let mut keys = 0;
    let mut leveled = 0;
    for (reference, lock) in &locks {
        let mut world = game.world_mut();
        let state = world.reference(*reference).unwrap();
        assert!(state.locked, "{reference}");
        assert_eq!(state.lock_level, i32::from(lock.level), "{reference}");
        if lock.key != 0 {
            assert!(
                world.key(*reference).is_some(),
                "missing key on {reference}"
            );
            keys += 1;
        }
        leveled += usize::from(lock.flags & 4 != 0);
    }
    let player = game.player();
    let chest = locks
        .iter()
        .find_map(|(id, _)| {
            let mut world = game.world_mut();
            let base = world.base_of(*id)?;
            (world.tag_of(base) == Some(T(b"CONT"))).then_some((*id, world.key(*id)?))
        })
        .unwrap();
    assert!(!game.activate_no_settle(chest.0, player));
    game.world_mut().add_item(player, chest.1, 1);
    assert!(game.activate_no_settle(chest.0, player));
    assert_eq!(game.world_mut().item_count(player, chest.1), 1);
    eprintln!(
        "{} authored locks, {keys} keys, {leveled} leveled flags, real chest {} unlocked with {}",
        locks.len(),
        chest.0,
        chest.1
    );
}
