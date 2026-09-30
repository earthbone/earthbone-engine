use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::MeleeHit;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const OTHER: u32 = 0x101;
const SOURCE: u32 = 0x102;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const ENCHANTMENT: u32 = 0x400;
const STRIKE: u32 = 0x401;
const WEAPON: u32 = 0x402;

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn mag_60_effect_does_not_benefit_a_different_attackers_weapon() {
    let mut enhance_data = vec![0; 152];
    put(&mut enhance_data, 8, ENCHANTMENT);
    put(&mut enhance_data, 16, u32::MAX);
    put(&mut enhance_data, 64, 39);
    put(&mut enhance_data, 80, 1);
    put(&mut enhance_data, 84, 3);
    let enhance = Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), enhance_data);
    let mut strike_data = vec![0; 152];
    put(&mut strike_data, 0, 1 << 2);
    put(&mut strike_data, 16, u32::MAX);
    put(&mut strike_data, 64, 0);
    put(&mut strike_data, 68, 24);
    let strike = Record::new(T(b"MGEF"), STRIKE).sub(T(b"DATA"), strike_data);
    let mut efit = 5.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&0_u32.to_le_bytes());
    let enchantment = Record::new(T(b"ENCH"), ENCHANTMENT).sub(T(b"ENIT"), vec![0; 8])
        .sub(T(b"EFID"), STRIKE.to_le_bytes()).sub(T(b"EFIT"), efit);
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);
    let mut spell_efit = 1.0_f32.to_le_bytes().to_vec();
    spell_efit.extend_from_slice(&0_u32.to_le_bytes());
    spell_efit.extend_from_slice(&10_u32.to_le_bytes());
    let spell = Record::new(T(b"SPEL"), SPELL).sub(T(b"SPIT"), spell_data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes()).sub(T(b"EFIT"), spell_efit);
    let mut weapon_data = vec![0; 10];
    put(&mut weapon_data, 4, 8_f32.to_bits());
    weapon_data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = 1;
    put(&mut weapon_dnam, 4, 1_f32.to_bits());
    put(&mut weapon_dnam, 8, 1.1_f32.to_bits());
    put(&mut weapon_dnam, 76, 6);
    put(&mut weapon_dnam, 88, u32::MAX);
    let weapon = Record::new(T(b"WEAP"), WEAPON).sub(T(b"DATA"), weapon_data).sub(T(b"DNAM"), weapon_dnam);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [ACTOR, OTHER, SOURCE] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x500_u32.to_le_bytes()).sub(T(b"DATA"), vec![0; 24]));
    }
    for record in [enhance, strike, enchantment, spell, weapon] { plugin.push_top(record); }
    let mut order = LoadOrder::new();
    order.add("EnhanceWeapon.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [ACTOR, OTHER, SOURCE] {
        game.world_mut().set_actor_value(soe_esm::GlobalFormId { file: 0, object: actor }, "health", 100.0).unwrap();
    }
    for actor in [ACTOR, OTHER] {
        game.world_mut().add_item(soe_esm::GlobalFormId { file: 0, object: actor }, soe_esm::GlobalFormId { file: 0, object: WEAPON }, 1);
        game.world_mut().equip_item(soe_esm::GlobalFormId { file: 0, object: actor }, soe_esm::GlobalFormId { file: 0, object: WEAPON }).unwrap();
    }
    let shared = game.shared_world();
    let applied = shared.borrow_mut().cast_spell(
        soe_esm::GlobalFormId { file: 0, object: SPELL }, soe_esm::GlobalFormId { file: 0, object: ACTOR },
        Some(soe_esm::GlobalFormId { file: 0, object: ACTOR }), Some(soe_esm::GlobalFormId { file: 0, object: ACTOR }),
        &mut |name, class| game.vm.create_object_of(name, class));
    applied.unwrap();
    let hit = game.melee_hit(MeleeHit {
        attacker: soe_esm::GlobalFormId { file: 0, object: OTHER },
        target: soe_esm::GlobalFormId { file: 0, object: SOURCE },
        weapon: soe_esm::GlobalFormId { file: 0, object: WEAPON }, is_bash: false, is_power_attack: false,
    }).unwrap();
    assert!((hit.weapon_damage - 10.0).abs() < 0.001);
}

#[test]
fn mag_60_supports_a_valid_non_health_associated_enchantment_on_melee_hit() {
    const ENCHANTMENT: u32 = 0x400;
    const STRIKE: u32 = 0x401;
    const WEAPON: u32 = 0x402;
    let mut enhance_data = vec![0; 152];
    put(&mut enhance_data, 8, ENCHANTMENT);
    put(&mut enhance_data, 16, u32::MAX);
    put(&mut enhance_data, 64, 39);
    put(&mut enhance_data, 80, 1);
    put(&mut enhance_data, 84, 3);
    let enhance = Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), enhance_data);
    let mut strike_data = vec![0; 152];
    put(&mut strike_data, 0, 1 << 2);
    put(&mut strike_data, 16, u32::MAX);
    put(&mut strike_data, 64, 0);
    put(&mut strike_data, 68, 25);
    let strike = Record::new(T(b"MGEF"), STRIKE).sub(T(b"DATA"), strike_data);
    let mut efit = 5.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&0_u32.to_le_bytes());
    let enchantment = Record::new(T(b"ENCH"), ENCHANTMENT).sub(T(b"ENIT"), vec![0; 8])
        .sub(T(b"EFID"), STRIKE.to_le_bytes()).sub(T(b"EFIT"), efit);
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);
    let mut spell_efit = 1.0_f32.to_le_bytes().to_vec();
    spell_efit.extend_from_slice(&0_u32.to_le_bytes());
    spell_efit.extend_from_slice(&10_u32.to_le_bytes());
    let spell = Record::new(T(b"SPEL"), SPELL).sub(T(b"SPIT"), spell_data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes()).sub(T(b"EFIT"), spell_efit);
    let mut weapon_data = vec![0; 10];
    put(&mut weapon_data, 4, 8_f32.to_bits());
    weapon_data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = 1;
    put(&mut weapon_dnam, 4, 1_f32.to_bits());
    put(&mut weapon_dnam, 8, 1.1_f32.to_bits());
    put(&mut weapon_dnam, 76, 6);
    put(&mut weapon_dnam, 88, u32::MAX);
    let weapon = Record::new(T(b"WEAP"), WEAPON).sub(T(b"DATA"), weapon_data).sub(T(b"DNAM"), weapon_dnam);
    let mut plugin = Plugin::new(FileHeader::default());
    let mut base_data = vec![0; 24];
    put(&mut base_data, 8, 10);
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), base_data));
    for actor in [ACTOR, OTHER, SOURCE] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x500_u32.to_le_bytes()).sub(T(b"DATA"), vec![0; 24]));
    }
    for record in [enhance, strike, enchantment, spell, weapon] { plugin.push_top(record); }
    let mut order = LoadOrder::new();
    order.add("EnhanceWeapon.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [ACTOR, OTHER, SOURCE] {
        game.world_mut().set_actor_value(soe_esm::GlobalFormId { file: 0, object: actor }, "health", 100.0).unwrap();
        game.world_mut().set_actor_value(soe_esm::GlobalFormId { file: 0, object: actor }, "stamina", 100.0).unwrap();
    }
    game.world_mut().add_item(soe_esm::GlobalFormId { file: 0, object: ACTOR }, soe_esm::GlobalFormId { file: 0, object: WEAPON }, 1);
    game.world_mut().equip_item(soe_esm::GlobalFormId { file: 0, object: ACTOR }, soe_esm::GlobalFormId { file: 0, object: WEAPON }).unwrap();
    let shared = game.shared_world();
    let applied = shared.borrow_mut().cast_spell(
        soe_esm::GlobalFormId { file: 0, object: SPELL }, soe_esm::GlobalFormId { file: 0, object: ACTOR },
        Some(soe_esm::GlobalFormId { file: 0, object: ACTOR }), Some(soe_esm::GlobalFormId { file: 0, object: ACTOR }),
        &mut |name, class| game.vm.create_object_of(name, class));
    applied.unwrap();
    let result = game.melee_hit(MeleeHit {
        attacker: soe_esm::GlobalFormId { file: 0, object: ACTOR },
        target: soe_esm::GlobalFormId { file: 0, object: OTHER },
        weapon: soe_esm::GlobalFormId { file: 0, object: WEAPON }, is_bash: false, is_power_attack: false,
    });
    let hit = result.expect("a valid associated ENCH effect should remain attached to the melee weapon");
    assert!((game.world_mut().actor_value(soe_esm::GlobalFormId { file: 0, object: OTHER }, "stamina") - 95.0).abs() < 0.001);
    assert!((hit.weapon_damage - 10.0).abs() < 0.001);
}
