// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved ownership and lock state through shared reference queries and activation.

#[path = "../common/legacy.rs"]
mod legacy;

use super::*;
use soe_game::Handle;
use soe_save::{reference::Field, ChangeForm};

#[test]
fn installed_lock_byte_mapping_matches_authored_levels() {
    let (Some(data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping lock schema survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(Path::new(&data).join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut locks, mut authored, mut matched) = (0, 0, 0);
    let mut bytes = std::collections::BTreeMap::new();
    for path in &paths {
        let save = Save::open(path).unwrap();
        let map = SavedFormMap::new(&save, &order).unwrap();
        for change in &save.change_forms {
            let Some(extra) = change.reference_extra_data(save.form_version).unwrap() else {
                continue;
            };
            for entry in extra.entries {
                if entry.kind != 42 {
                    continue;
                }
                let [Field::Byte(level), Field::Byte(flags), Field::Reference(_), Field::Word(_), Field::Word(_)] =
                    entry.fields.as_slice()
                else {
                    panic!("lock schema")
                };
                locks += 1;
                *bytes.entry((*level, *flags)).or_insert(0) += 1;
                let id = map.reference(change.form).unwrap().unwrap();
                if let Some(raw) = order.winner(id).and_then(|r| r.find(Tag(*b"XLOC"))) {
                    let lock = soe_esm::records::refr::Lock::from_bytes(raw).unwrap();
                    authored += 1;
                    matched += usize::from(lock.level == *level);
                }
            }
        }
    }
    eprintln!("lock schema survey: {locks} locks, {authored} authored, {matched} level matches, byte pairs {bytes:?}");
    assert!(locks >= 1000);
    assert!(authored >= 1000);
    assert!(
        matched * 100 >= authored * 90,
        "at least 90 percent of saved levels match authored levels"
    );
}

fn fixture_order() -> LoadOrder {
    let mut order = LoadOrder::new();
    for (name, light) in [
        ("Skyrim.esm", false),
        ("Extra.esl", true),
        ("B.esp", false),
        ("A.esp", false),
        ("Light.esl", true),
    ] {
        let mut plugin = Plugin::new(FileHeader {
            flags: if light { flags::LIGHT_MASTER } else { 0 },
            ..Default::default()
        });
        for (tag, object) in [
            (b"NPC_", 7),
            (b"DOOR", 0x10),
            (b"CONT", 0x11),
            (b"KEYM", 0x800),
            (b"KEYM", 0x801),
            (b"NPC_", 0x811),
            (b"FACT", 0x810),
            (b"GLOB", 0x812),
        ] {
            plugin.push_top(Record::new(Tag(*tag), object));
        }
        for (target, base) in [(0x100, 0x10_u32), (0x101, 0x11), (0x102, 0x11)] {
            let mut lock = vec![0; 20];
            lock[0] = 75;
            lock[4..8].copy_from_slice(&0x800_u32.to_le_bytes());
            plugin.push_top(
                Record::new(Tag(*b"REFR"), target)
                    .sub(Tag(*b"NAME"), base.to_le_bytes())
                    .sub(Tag(*b"XOWN"), 0x810_u32.to_le_bytes())
                    .sub(Tag(*b"XLOC"), lock),
            );
        }
        order.add(name, plugin).unwrap();
    }
    order
}

fn change(target: u32, owner: u32, level: u8, lock_flags: u8, key: u32) -> ChangeForm {
    let mut data = vec![8, 33];
    reference(owner).write(&mut data);
    data.extend([42, level, lock_flags]);
    reference(key).write(&mut data);
    data.extend(0xAABB_CCDD_u32.to_le_bytes());
    data.extend(0x1234_5678_u32.to_le_bytes());
    ChangeForm {
        form: reference(target),
        change_flags: (1 << 6) | (1 << 12),
        type_field: 0,
        version: 0,
        data,
        uncompressed_len: 0,
    }
}

fn call(game: &mut Game, target: GlobalFormId, name: &str) -> Value {
    let script = format!("ExtraProbe{name}");
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("target", "ObjectReference")
                .local("result", "Var")
                .ins(
                    Opcode::CallMethod,
                    vec![s(name), ident("target"), ident("result")],
                )
                .ins(Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of(Handle::Form(target));
    let thread = game
        .vm
        .call_global(&script, "Run", &[Value::Object(object)])
        .unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn sav_16_restored_extras_drive_natives_activation_and_engine_roundtrip() {
    let mut save = save();
    save.form_id_array = vec![0x0100_0811, 0xFE00_0801, 0x0200_0810, 0x0100_0812];
    save.change_forms = vec![change(0x400100, 1, 0, 1, 2), change(0x400101, 3, 255, 0, 0)];
    save.change_forms[0].data[0] = 16;
    save.change_forms[0].data.push(35);
    save.change_forms[0].data.extend((-7_i32).to_le_bytes());
    save.change_forms[0].data.push(34);
    reference(4).write(&mut save.change_forms[0].data);
    let mut game = Game::new(fixture_order());
    let player = game.player();
    for object in [0x100, 0x101, 0x102] {
        game.world_mut().reference(id(0, object)).unwrap();
    }
    let untouched = game.world().reference_ref(id(0, 0x102)).unwrap().clone();
    let report = game
        .world_mut()
        .restore_vanilla_reference_extras(&save)
        .unwrap();
    assert_eq!((report.references, report.owners, report.locks), (2, 2, 2));
    assert!(game.world().pending.is_empty());
    assert_eq!(game.world().reference_ref(id(0, 0x102)), Some(&untouched));
    assert_eq!(
        game.world().reference_ref(id(0, 0x100)).unwrap().owner_rank,
        Some(-7)
    );
    assert_eq!(
        game.world()
            .reference_ref(id(0, 0x100))
            .unwrap()
            .owner_global,
        Some(id(3, 0x812))
    );
    let bytes = game.save_state().unwrap();
    let hash = game.world().state_hash();
    let mut loaded = Game::new(fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.world().state_hash(), hash);
    assert_eq!(loaded.save_state().unwrap(), bytes);
    for game in [&mut game, &mut loaded] {
        let actor_owner = Value::Object(game.object_of(Handle::Form(id(3, 0x811))));
        let faction_owner = Value::Object(game.object_of(Handle::Form(id(2, 0x810))));
        let key = Value::Object(game.object_of(Handle::Form(id(4, 0x801))));
        assert_eq!(call(game, id(0, 0x100), "GetActorOwner"), actor_owner);
        assert_eq!(call(game, id(0, 0x100), "GetFactionOwner"), Value::None);
        assert_eq!(call(game, id(0, 0x101), "GetFactionOwner"), faction_owner);
        assert_eq!(call(game, id(0, 0x101), "GetActorOwner"), Value::None);
        assert_eq!(call(game, id(0, 0x100), "GetKey"), key);
        assert_eq!(call(game, id(0, 0x101), "GetKey"), Value::None);
        assert_eq!(call(game, id(0, 0x100), "GetLockLevel"), Value::Int(0));
        assert_eq!(call(game, id(0, 0x101), "GetLockLevel"), Value::Int(255));
        assert_eq!(call(game, id(0, 0x100), "IsLocked"), Value::Bool(true));
        assert_eq!(call(game, id(0, 0x101), "IsLocked"), Value::Bool(false));
        game.world_mut().add_item(player, id(0, 0x800), 1);
        assert!(
            !game.activate_no_settle(id(0, 0x100), player),
            "authored key cannot unlock saved key override"
        );
        game.world_mut().add_item(player, id(4, 0x801), 1);
        assert!(game.activate_no_settle(id(0, 0x100), player));
        assert_eq!(game.world_mut().item_count(player, id(4, 0x801)), 1);
        assert!(game.world().reference_ref(id(0, 0x100)).unwrap().opened);
        assert!(game.activate_no_settle(id(0, 0x101), player));
    }
}

#[test]
fn extras_failure_after_a_valid_entry_leaves_all_state_and_events_unchanged() {
    let mut world = World::new(fixture_order());
    for object in [0x100, 0x101] {
        world.reference(id(0, object)).unwrap();
    }
    let mut save = save();
    let good = change(0x400100, 0x400811, 50, 1, 0x400801);
    let last = change(0x400101, 0x400810, 25, 0, 0);
    let mut cases = vec![];
    let mut truncated = last.clone();
    truncated.data.pop();
    cases.push(truncated);
    let mut duplicate = last.clone();
    duplicate.data[0] = 12;
    duplicate.data.push(33);
    reference(0).write(&mut duplicate.data);
    cases.push(duplicate);
    cases.push(change(0x400101, 0x400800, 0, 1, 0));
    cases.push(change(0x400101, 0x400810, 0, 1, 0x400811));
    cases.push(change(0x400101, 0x400FFE, 0, 1, 0));
    cases.push(change(0x400FFE, 0, 0, 1, 0));
    cases.push(good.clone());
    let before = world.save_state();
    let hash = world.state_hash();
    for bad in cases {
        save.change_forms = vec![good.clone(), bad];
        assert!(world.restore_vanilla_reference_extras(&save).is_err());
        assert_eq!(world.save_state(), before);
        assert_eq!(world.state_hash(), hash);
        assert!(world.pending.is_empty());
    }
}

#[test]
fn removed_and_explicit_null_extras_clear_authored_state_but_omitted_changes_keep_it() {
    let mut world = World::new(fixture_order());
    for object in [0x100, 0x101, 0x102] {
        world.reference(id(0, object)).unwrap();
    }
    let mut save = save();
    let mut removed = change(0x400100, 0, 0, 0, 0);
    removed.data = vec![0];
    let mut omitted = removed.clone();
    omitted.form = reference(0x400102);
    omitted.change_flags = 0;
    omitted.data.clear();
    save.change_forms = vec![removed, change(0x400101, 0, 100, 1, 0), omitted];
    world.restore_vanilla_reference_extras(&save).unwrap();
    for object in [0x100, 0x101] {
        assert_eq!(world.reference_ref(id(0, object)).unwrap().owner, None);
        assert_eq!(world.key(id(0, object)), None);
    }
    assert!(!world.reference_ref(id(0, 0x100)).unwrap().locked);
    assert!(world.reference_ref(id(0, 0x101)).unwrap().locked);
    assert_eq!(world.key(id(0, 0x102)), Some(id(0, 0x800)));
    let bytes = world.save_state();
    world.restore_vanilla_reference_extras(&save).unwrap();
    assert_eq!(world.save_state(), bytes, "restoration is idempotent");
}

#[test]
fn engine_extras_reject_truncation_duplicates_and_bad_flags_atomically_and_read_version_19() {
    let mut world = World::new(fixture_order());
    world.reference(id(0, 0x100)).unwrap();
    world.reference(id(0, 0x101)).unwrap();
    // Both references' own empty save-section tables (version 37) are
    // stripped here, since version 19 predates them.
    let mut legacy = legacy::without_ref_sections(&mut world, &[id(0, 0x100), id(0, 0x101)]);
    // The extra 6 bytes beyond the old 37 are detection's now-unconditional
    // empty pool count and the two world-level save sections version 37
    // added, all always present at the tail regardless of the simulated
    // version.
    legacy.truncate(legacy.len() - 73);
    legacy[4..6].copy_from_slice(&19_u16.to_le_bytes());
    let mut save = save();
    save.change_forms = vec![change(0x400100, 0x400811, 0, 5, 0x400801)];
    world.restore_vanilla_reference_extras(&save).unwrap();
    let before = world.save_state();
    let mut bad_flags = before.clone();
    let at = bad_flags.len() - 78;
    bad_flags[at] |= 1;
    // `legacy` had both references' own save-section tables stripped above
    // (2 bytes each), which `before` still carries, so add the 4 bytes back
    // to land on the same absolute offset, where the reference extras block
    // begins.
    let start = legacy.len() + 4;
    // The extras block is followed by the package cursor block, so repeat the
    // entry inside the extras block rather than past the end of the save.
    let entry = &before[start + 4..before.len() - 69];
    let mut duplicate = before[..start].to_vec();
    duplicate.extend_from_slice(&2_u32.to_le_bytes());
    duplicate.extend_from_slice(entry);
    duplicate.extend_from_slice(entry);
    duplicate.extend_from_slice(&before[before.len() - 69..]);
    assert!(world
        .load_state(&duplicate)
        .unwrap_err()
        .to_string()
        .contains("duplicate reference extras"));
    let mut bad_key = before.clone();
    let at = bad_key.len() - 57;
    bad_key[at..at + 4].copy_from_slice(&0x811_u32.to_le_bytes());
    let mut cases = vec![bad_flags, duplicate, bad_key];
    for cut in 1..=28 {
        cases.push(before[..before.len() - cut].to_vec());
    }
    for malformed in cases {
        assert!(world.load_state(&malformed).is_err());
        assert_eq!(world.save_state(), before);
    }
    world.load_state(&legacy).unwrap();
    assert!(world
        .reference_ref(id(0, 0x100))
        .unwrap()
        .lock_extra
        .is_none());
    assert_eq!(world.key(id(0, 0x100)), Some(id(0, 0x800)));
}

#[test]
fn installed_all_ownership_and_lock_extras_restore_and_persist() {
    let (Some(data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!(
            "SKYRIM_DATA or SOE_TEST_SAVES unset, skipping reference extras restoration survey"
        );
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(Path::new(&data).join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let order = Arc::new(order);
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut references, mut owners, mut locks, mut leveled) = (0, 0, 0, 0);
    for path in &paths {
        let save = Save::open(path).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        world.restore_vanilla_reference_placements(&save).unwrap();
        let report = world
            .restore_vanilla_reference_extras(&save)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        references += report.references;
        owners += report.owners;
        locks += report.locks;
        leveled += report.leveled_locks.len();
        assert!(world.pending.is_empty());
        let bytes = world.save_state();
        let hash = world.state_hash();
        let mut loaded = World::from_shared_order(Arc::clone(&order));
        loaded.load_state(&bytes).unwrap();
        assert_eq!(loaded.state_hash(), hash);
        assert_eq!(loaded.save_state(), bytes);
        let mapping = SavedFormMap::new(&save, &order).unwrap();
        for change in &save.change_forms {
            if !matches!(change.type_index(), 0 | 1) {
                continue;
            }
            let id = mapping.reference(change.form).unwrap().unwrap();
            let state = world.reference_ref(id).unwrap();
            let restored = loaded.reference_ref(id).unwrap();
            assert_eq!(state, restored);
            assert_eq!(world.key(id), loaded.key(id));
        }
    }
    eprintln!("reference extras survey: {} saves, {references} references, {owners} ownership changes, {locks} locks, {leveled} leveled locks", paths.len());
    assert!(references >= 1000);
    assert!(owners >= 100);
    assert!(locks >= 1000);
}

#[test]
fn installed_extra_presence_audit() {
    let Some(saves) = std::env::var_os("SOE_TEST_SAVES") else {
        return;
    };
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    let mut missed = std::collections::BTreeMap::new();
    let mut failed = std::collections::BTreeMap::new();
    for path in paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let extra = match change.reference_extra_data(save.form_version) {
                Ok(Some(extra)) => extra,
                Ok(None) => continue,
                Err(error) => {
                    *failed
                        .entry((change.type_index(), change.change_flags, error.to_string()))
                        .or_insert(0) += 1;
                    continue;
                }
            };
            for entry in extra.entries {
                if matches!(entry.kind, 33..=35 | 42)
                    && change.change_flags & (1 << 6) == 0
                    && !(change.type_index() == 0 && change.change_flags & (1 << 12) != 0)
                {
                    *missed
                        .entry((change.type_index(), change.change_flags, entry.kind))
                        .or_insert(0) += 1;
                }
            }
        }
    }
    eprintln!("unflagged related extras: {missed:?}, undecodable changes: {failed:?}");
    assert!(
        failed.is_empty(),
        "all top-level extra lists must decode before full restoration can succeed"
    );
}

#[test]
fn generic_extra_lists_restore_ownership_and_keys_without_specific_change_bits() {
    let mut world = World::new(fixture_order());
    for object in [0x100, 0x101] {
        world.reference(id(0, object)).unwrap();
    }
    let mut save = save();
    let mut promoted = change(0x400100, 0x400811, 25, 1, 0x400801);
    promoted.change_flags = 1 << 31;
    let mut actor = change(0x400101, 0x400811, 0, 0, 0);
    actor.type_field = 1;
    actor.change_flags = 1 << 31;
    actor.data.truncate(5);
    actor.data[0] = 4;
    actor.data.splice(..0, [0; 8]);
    save.change_forms = vec![promoted, actor];
    let report = world.restore_vanilla_reference_extras(&save).unwrap();
    assert_eq!((report.references, report.owners, report.locks), (2, 2, 1));
    assert_eq!(
        world.reference_ref(id(0, 0x100)).unwrap().owner,
        Some(id(0, 0x811))
    );
    assert_eq!(
        world.reference_ref(id(0, 0x101)).unwrap().owner,
        Some(id(0, 0x811))
    );
    assert_eq!(world.key(id(0, 0x100)), Some(id(0, 0x801)));
    assert_eq!(world.key(id(0, 0x101)), Some(id(0, 0x800)));
}

#[test]
fn actor_merchant_container_flag_does_not_clear_lock_or_owner_state() {
    let mut world = World::new(fixture_order());
    world.reference(id(0, 0x100)).unwrap();
    let mut save = save();
    let mut merchant = change(0x400100, 0, 0, 0, 0);
    merchant.type_field = 1;
    merchant.change_flags = 1 << 12;
    merchant.data = vec![0; 9];
    save.change_forms = vec![merchant];
    let before = world.save_state();
    let report = world.restore_vanilla_reference_extras(&save).unwrap();
    assert_eq!(report.references, 0);
    assert_eq!(world.save_state(), before);
}

#[test]
fn later_unsupported_package_data_leaves_prepared_ownership_and_locks_unapplied() {
    let mut world = World::new(fixture_order());
    for object in [0x100, 0x101] {
        world.reference(id(0, object)).unwrap();
    }
    let mut save = save();
    let mut actor = change(0x400101, 0, 0, 0, 0);
    actor.type_field = 1;
    actor.change_flags = 1 << 31;
    actor.data = vec![4, 108, 18, 0];
    WString::from_text("Sequence").write(&mut actor.data);
    actor.data.splice(..0, [0; 8]);
    save.change_forms = vec![change(0x400100, 0x400811, 25, 1, 0x400801), actor];
    let before = world.save_state();
    assert!(world
        .restore_vanilla_reference_extras(&save)
        .unwrap_err()
        .to_string()
        .contains("unsupported PackageData"));
    assert_eq!(world.save_state(), before);
    assert!(world.pending.is_empty());
}
