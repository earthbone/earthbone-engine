// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass for CMB-25 (weapon poison consumption on a successful hit).
//!
//! Contract under test, from `combat_stack_poison`, `check_combat_weapon` and
//! `check_no_poison_or_enchant` in crates/game/src/world/combat.rs:
//! - The equipped set is form-only (CMB-03), so a hit cannot name which
//!   physical stack the attacker swung. When more than one stack of the
//!   equipped weapon carries a kind-62 poison extra, the selected stack
//!   cannot be determined, so the contact rejects with
//!   `Error::Unsupported("ambiguous poisoned item instances")`, even when
//!   the two stacks agree on the same poison form and count: consumption
//!   needs one physical index, not just an agreeing value, unlike quality
//!   (CMB-10).
//! - A kind-62 extra whose fields are not exactly `[Field::Form(Some(_)),
//!   Field::Word(count)]`, or whose count is 0, is "malformed weapon poison
//!   extra".
//! - Poison is only ever read on an ordinary weapon swing. A bash still
//!   rejects a poisoned equipped item unconditionally through
//!   `check_no_poison_or_enchant`, the same as an enchanted one, "poisoned
//!   or enchanted item instances", CMB-03's original armor and bash rule.
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    combat::{Error, MeleeHit},
    inventory::{Extra, Field, Stack},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const WEAPON: u32 = 0xA00;
const SHIELD: u32 = 0xB00;
const POISON: u32 = 0xC00;
const ATTACKER: u32 = 0x900;
const TARGET: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon(object: u32, damage: u16, skill: i32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
    Record::new(T(b"WEAP"), object)
        .sub(T(b"DATA"), data)
        .sub(T(b"DNAM"), dnam)
}

/// A BOD2 shield-slot (bit 9) ARMO, the bash source `combat_armor_record`
/// requires. Kind 2 (shield-type) contributes nothing to armor rating or
/// category count, matching cmb_10_quality_adversarial.rs's own `armor`.
fn shield(object: u32, rating_hundredths: u32) -> Record {
    let mut body = (1_u32 << 9).to_le_bytes().to_vec();
    body.extend_from_slice(&2_u32.to_le_bytes());
    Record::new(T(b"ARMO"), object)
        .sub(
            T(b"DATA"),
            [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
        )
        .sub(T(b"DNAM"), rating_hundredths.to_le_bytes())
        .sub(T(b"BOD2"), body)
}

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(T(b"GMST"), object)
        .zsub(T(b"EDID"), name)
        .sub(T(b"DATA"), value.to_le_bytes())
}

fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0_f32),
        ("fDamagePCSkillMin", 1.0),
        ("fDamagePCSkillMax", 1.0),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 1.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.0),
        ("fArmorRatingMax", 1.0),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.0),
        ("fMaxArmorRating", 80.0),
        ("fBashStaminaCost", 5.0),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(gmst(0x1000 + index as u32, name, value));
    }
}

/// Two plain NPC-driven actors (0x900 attacking 0x901), so `melee_hit`'s
/// difficulty term is always 1.0. No item is equipped here, each test
/// equips exactly the stacks it means to exercise.
fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [ATTACKER, TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    plugin.push_top(weapon(WEAPON, 10, 6));
    plugin.push_top(shield(SHIELD, 100));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Poison.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ATTACKER, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "stamina", 100.)
            .unwrap();
    }
    game
}

fn equip_stacks(game: &Game, actor: u32, item: u32, total: i32, stacks: Vec<Stack>) {
    game.world_mut()
        .reference(id(actor))
        .unwrap()
        .inventory
        .restore(id(item), total, stacks)
        .unwrap();
    game.world_mut()
        .actor(id(actor))
        .unwrap()
        .equipped
        .insert(id(item));
}

fn stack(count: i32, extras: Vec<Extra>) -> Stack {
    Stack::new(count, extras).unwrap()
}

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(ATTACKER),
        target: id(TARGET),
        weapon: id(WEAPON),
        is_bash: false,
        is_power_attack: false,
    }
}

/// Two stacks of the equipped weapon both carry a kind-62 extra, and they
/// agree on the same poison form and count. Unlike CMB-10's quality
/// agreement rule, poison still rejects: consumption needs one physical
/// stack index, and nothing here names which of the two the attacker swung.
#[test]
fn cmb_25_two_agreeing_poisoned_stacks_reject_as_ambiguous_and_leave_state_untouched() {
    let mut game = fixture();
    equip_stacks(
        &game,
        ATTACKER,
        WEAPON,
        2,
        vec![
            stack(
                1,
                vec![Extra {
                    kind: 62,
                    fields: vec![Field::Form(Some(id(POISON))), Field::Word(1)],
                }],
            ),
            stack(
                1,
                vec![Extra {
                    kind: 62,
                    fields: vec![Field::Form(Some(id(POISON))), Field::Word(1)],
                }],
            ),
        ],
    );
    let before = game.save_state().unwrap();
    let result = game.melee_hit(hit());
    assert!(
        matches!(
            result,
            Err(Error::Unsupported("ambiguous poisoned item instances"))
        ),
        "{result:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
}

/// A kind-62 extra with a single `Word` field instead of CommonLib
/// `ExtraPoison`'s `[Form, Word]` pair does not match `combat_stack_poison`'s
/// pattern and must reject rather than panic or silently skip the stack.
#[test]
fn cmb_25_malformed_poison_extra_shape_rejects_without_partial_application() {
    let mut game = fixture();
    equip_stacks(
        &game,
        ATTACKER,
        WEAPON,
        1,
        vec![stack(
            1,
            vec![Extra {
                kind: 62,
                fields: vec![Field::Word(1)],
            }],
        )],
    );
    let before = game.save_state().unwrap();
    let result = game.melee_hit(hit());
    assert!(
        matches!(
            result,
            Err(Error::Unsupported("malformed weapon poison extra"))
        ),
        "{result:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
}

/// A kind-62 extra with the right field shape but a zero count is not a
/// valid dose either, same rejection as a shape mismatch.
#[test]
fn cmb_25_zero_count_poison_extra_rejects_as_malformed() {
    let mut game = fixture();
    equip_stacks(
        &game,
        ATTACKER,
        WEAPON,
        1,
        vec![stack(
            1,
            vec![Extra {
                kind: 62,
                fields: vec![Field::Form(Some(id(POISON))), Field::Word(0)],
            }],
        )],
    );
    let before = game.save_state().unwrap();
    let result = game.melee_hit(hit());
    assert!(
        matches!(
            result,
            Err(Error::Unsupported("malformed weapon poison extra"))
        ),
        "{result:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
}

/// A shield bash still rejects a poisoned equipped stack unconditionally
/// through `check_no_poison_or_enchant`, the same as an enchanted one: CMB-25
/// only lets an ordinary weapon swing (the `else` branch of `melee_hit`)
/// read and consume poison. `Error::Unsupported("poisoned or enchanted item
/// instances")`, not the CMB-25 poison-specific errors, confirms the bash
/// branch never reaches `combat_stack_poison` at all.
#[test]
fn cmb_25_bash_rejects_a_poisoned_shield_and_never_consumes_it() {
    let mut game = fixture();
    equip_stacks(
        &game,
        ATTACKER,
        SHIELD,
        1,
        vec![stack(
            1,
            vec![Extra {
                kind: 62,
                fields: vec![Field::Form(Some(id(POISON))), Field::Word(1)],
            }],
        )],
    );
    // Not a full save_state() before/after comparison here: the shield's
    // poison extra is well-formed and unambiguous, so the early read-only
    // quality/poison pass does not reject, and `melee_hit` reads the
    // target's actor base right after it, before the bash branch's own
    // poison/enchant check runs. That read may initialize the target's
    // actor base lazily, and save_state persists the entry (CMB-36,
    // docs/combat.md). The rejection below is still unconditional and
    // still pre-empts damage and consumption, so the assertions target
    // those two invariants directly instead.
    let target_health = game
        .world_mut()
        .actor(id(TARGET))
        .unwrap()
        .values
        .get("health")
        .current();
    let target_stamina = game
        .world_mut()
        .actor(id(TARGET))
        .unwrap()
        .values
        .get("stamina")
        .current();
    let result = game.melee_hit(MeleeHit {
        attacker: id(ATTACKER),
        target: id(TARGET),
        weapon: id(SHIELD),
        is_bash: true,
        is_power_attack: false,
    });
    assert!(
        matches!(
            result,
            Err(Error::Unsupported("poisoned or enchanted item instances"))
        ),
        "{result:?}"
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("health")
            .current(),
        target_health,
        "a rejected bash must not damage the target"
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        target_stamina,
        "a rejected bash must not apply the shield's poison to the target"
    );
    // The coating must still be on the shield's stack: a rejected hit never
    // reaches the consumption step.
    let stacks = game
        .world_mut()
        .reference(id(ATTACKER))
        .unwrap()
        .inventory
        .stacks(id(SHIELD))
        .to_vec();
    assert!(stacks[0].extra(62).is_some());
}
