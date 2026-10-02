#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::DYNAMIC_FILE;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const BASE: u32 = 0x400;
const EXPLOSION: u32 = 0x401;

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(mgef_len: usize, expl_len: usize) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut effect_data = vec![0; mgef_len];
    put(&mut effect_data, 64, 43);
    put(&mut effect_data, 76, EXPLOSION);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 4);
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    let mut spit = vec![0; 36];
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 4);
    let mut efit = 1.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&10_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut explosion_data = vec![0; expl_len];
    put(&mut explosion_data, 16, BASE);
    plugin.push_top(Record::new(T(b"EXPL"), EXPLOSION).sub(T(b"DATA"), explosion_data));
    plugin.push_top(Record::new(T(b"ACTI"), BASE));
    let mut order = LoadOrder::new();
    assert!(order.add("ScriptedRef.esm", plugin).is_ok());
    Game::new(order)
}

fn cast(game: &mut Game) -> Result<(), soe_game::world::magic::MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        GlobalFormId {
            file: 0,
            object: SPELL,
        },
        GlobalFormId {
            file: 0,
            object: ACTOR,
        },
        Some(GlobalFormId {
            file: 0,
            object: ACTOR,
        }),
        Some(GlobalFormId {
            file: 0,
            object: ACTOR,
        }),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result.map(|_| ())
}

#[test]
fn mag_64_rejects_truncated_mgef_data_before_spawning() {
    let mut game = fixture(88, 52);
    assert!(cast(&mut game).is_err());
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(
        game.world()
            .reference_ref(GlobalFormId {
                file: DYNAMIC_FILE,
                object: 0x1001
            })
            .is_none()
    );
}

#[test]
fn mag_64_rejects_truncated_expl_data_before_spawning() {
    let mut game = fixture(152, 20);
    assert!(cast(&mut game).is_err());
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(
        game.world()
            .reference_ref(GlobalFormId {
                file: DYNAMIC_FILE,
                object: 0x1001
            })
            .is_none()
    );
}
