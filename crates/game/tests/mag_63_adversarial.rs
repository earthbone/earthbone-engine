#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const SUMMON_BASE: u32 = 0x501;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn mgef(form: u32, archetype: u32, associated: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 8, associated);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, 24);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    Record::new(T(b"MGEF"), form).sub(T(b"DATA"), data)
}

fn spell(form: u32, effect: u32, magnitude: f32, duration: u32) -> Record {
    let mut spit = vec![0; 36];
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), spit)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn fixture() -> (Game, Plugin) {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut acbs = vec![0; 24];
    put(&mut acbs, 8, 10);
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), acbs));
    let mut summon_acbs = vec![0; 24];
    put(&mut summon_acbs, 0, 0x4000);
    put(&mut summon_acbs, 8, 10);
    plugin.push_top(Record::new(T(b"NPC_"), SUMMON_BASE).sub(T(b"ACBS"), summon_acbs));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(mgef(EFFECT, 18, SUMMON_BASE));
    plugin.push_top(mgef(EFFECT + 1, 42, 0));
    plugin.push_top(spell(SPELL, EFFECT, 0.0, 30));
    plugin.push_top(spell(SPELL + 1, EFFECT + 1, 10.0, 0));
    let mut order = LoadOrder::new();
    order
        .add("Banish.esm", plugin.clone())
        .expect("fixture load order is valid");
    (Game::new(order), plugin)
}

fn cast(game: &mut Game, spell: u32, target: GlobalFormId) {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(spell),
        id(ACTOR),
        Some(id(ACTOR)),
        Some(target),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result.expect("fixture cast succeeds");
}

#[test]
fn mag_63_banish_at_exact_summon_level_dismisses_saved_summon() {
    let (mut original, content) = fixture();
    cast(&mut original, SPELL, id(ACTOR));
    let summoned = original
        .world()
        .active_effects()
        .find_map(|effect| effect.summoned)
        .expect("summon effect retains its spawned actor");
    let saved = original.save_state().expect("summoned actor state saves");
    let mut order = LoadOrder::new();
    order
        .add("Banish.esm", content)
        .expect("restored load order is valid");
    let mut restored = Game::new(order);
    restored
        .load_state(&saved)
        .expect("saved summon state loads");
    assert_eq!(
        restored
            .world()
            .active_effects()
            .find_map(|effect| effect.summoned),
        Some(summoned)
    );
    cast(&mut restored, SPELL + 1, summoned);
    let world = restored.world();
    let reference = world
        .reference_ref(summoned)
        .expect("saved summon reference exists");
    assert!(reference.deleted);
    assert!(!reference.enabled);
    assert!(
        world
            .active_effects()
            .all(|effect| effect.summoned != Some(summoned))
    );
}
