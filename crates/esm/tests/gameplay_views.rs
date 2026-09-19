// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Scoped record contracts checked against xEdit field sizes and UESP offsets.

use soe_esm::records::gameplay::*;
use soe_esm::records::Text;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn condition(flags: u8, comparison: u32, function: u16, param: u32) -> Vec<u8> {
    let mut bytes = vec![flags, 0xaa, 0xbb, 0xcc];
    bytes.extend(comparison.to_le_bytes());
    bytes.extend(function.to_le_bytes());
    bytes.extend([0xdd, 0xee]);
    bytes.extend(words(&[param, 7, 2, 0x800, u32::MAX]));
    bytes
}

fn spell() -> Record {
    Record::new(b"SPEL", 0x100).sub(
        b"SPIT",
        words(&[
            20,
            1,
            0,
            0.5f32.to_bits(),
            1,
            2,
            1.5f32.to_bits(),
            50f32.to_bits(),
            0x900,
        ]),
    )
}

fn recipe() -> Record {
    Record::new(b"COBJ", 0x200)
        .sub(b"CNAM", 0x300u32.to_le_bytes())
        .sub(b"NAM1", 2u16.to_le_bytes())
}

fn response(number: u8, emotion: u32, sound: u32) -> Vec<u8> {
    let mut bytes = words(&[emotion, 80, 0xfeed_beef]);
    bytes.extend([number, 1, 2, 3]);
    bytes.extend(sound.to_le_bytes());
    bytes.extend([1, 4, 5, 6]);
    bytes
}

#[test]
fn spell_conditions_stay_with_their_effect_and_parameter_strings() {
    let rec = spell()
        .sub(b"FULL", 42u32.to_le_bytes())
        .sub(b"EFID", 0x10u32.to_le_bytes())
        .sub(b"EFIT", words(&[12.5f32.to_bits(), 20, 30]))
        .sub(b"CTDA", condition(0x85, 0x123, 14, 24))
        .zsub(b"CIS1", "Health")
        .sub(b"EFID", 0x11u32.to_le_bytes())
        .sub(b"EFIT", words(&[3f32.to_bits(), 0, 60]))
        .sub(b"CTDA", condition(0x20, 0.5f32.to_bits(), 77, 5))
        .zsub(b"CIS2", "second");
    let view = Spell::from_record(&rec, true).unwrap();
    assert_eq!(view.name, Some(Text::Localized(42)));
    assert_eq!(view.data.base_cost, 20);
    assert_eq!(view.data.charge_time, 0.5);
    assert_eq!(view.data.cast_duration, 1.5);
    assert_eq!(view.data.range, 50.0);
    assert_eq!(view.data.half_cost_perk, 0x900);
    assert_eq!(view.effects.len(), 2);
    assert_eq!(view.effects[0].base_effect, 0x10);
    assert_eq!(
        view.effects[0].data,
        EffectData {
            magnitude: 12.5,
            area: 20,
            duration: 30
        }
    );
    let c = &view.effects[0].conditions[0];
    assert_eq!(c.comparison, Comparison::Global(0x123));
    assert_eq!(c.operator(), 4);
    assert!(c.or_with_next());
    assert_eq!(c.function, 14);
    assert_eq!(c.parameters, [24, 7]);
    assert_eq!(c.run_on, 2);
    assert_eq!(c.reference, 0x800);
    assert_eq!(c.parameter_3, -1);
    assert_eq!(c.strings, [Some("Health".into()), None]);
    assert_eq!(view.effects[1].data.duration, 60);
    assert_eq!(
        view.effects[1].conditions[0].comparison,
        Comparison::Number(0.5)
    );
    assert_eq!(
        view.effects[1].conditions[0].strings,
        [None, Some("second".into())]
    );
}

#[test]
fn dialogue_subtype_follows_subrecord_order() {
    let rec = Record::new(b"DIAL", 1)
        .sub(b"DATA", [1, 2, 0xfe, 0x01])
        .sub(b"SNAM", *b"SCEN")
        .sub(b"QNAM", 0x123u32.to_le_bytes())
        .sub(b"PNAM", 55f32.to_le_bytes());
    let topic = DialogueTopic::from_record(&rec, false).unwrap();
    assert_eq!(topic.category, Some(2));
    assert_eq!(topic.do_all_before_repeating, Some(1));
    assert_eq!(topic.quest, Some(0x123));
    assert_eq!(topic.priority, Some(55.0));
    assert_eq!(topic.subtype, Some(TopicSubtype::Signature(*b"SCEN")));
    let rec = rec.sub(b"DATA", [0, 3, 0x12, 0x34]);
    let topic = DialogueTopic::from_record(&rec, false).unwrap();
    assert_eq!(topic.subtype, Some(TopicSubtype::Numeric(0x3412)));
    assert_eq!(topic.category, Some(3));
}

#[test]
fn info_responses_keep_their_own_text_sound_and_idle_links() {
    let rec = Record::new(b"INFO", 2)
        .sub(b"ENAM", [2, 4, 0xff, 0xff])
        .sub(b"TRDT", response(1, 5, 0x100))
        .sub(b"NAM1", 11u32.to_le_bytes())
        .sub(b"SNAM", 0x200u32.to_le_bytes())
        .sub(b"TRDT", response(3, 2, 0x101))
        .sub(b"NAM1", 12u32.to_le_bytes())
        .sub(b"LNAM", 0x201u32.to_le_bytes())
        .sub(b"CTDA", condition(0, 1f32.to_bits(), 1, 0))
        .sub(b"TPIC", 0x300u32.to_le_bytes())
        .sub(b"DNAM", 0x301u32.to_le_bytes())
        .sub(b"TCLT", 0x400u32.to_le_bytes())
        .sub(b"TCLT", 0x401u32.to_le_bytes())
        .sub(b"VMAD", [1, 2, 3]);
    let info = DialogueInfo::from_record(&rec, true).unwrap();
    assert_eq!(info.data.as_ref().unwrap().flags(), 0x402);
    assert_eq!(info.data.unwrap().reset_hours(), 24.0);
    assert_eq!(info.previous_topic, Some(0x300));
    assert_eq!(info.shared_info, Some(0x301));
    assert_eq!(info.topic_links, [0x400, 0x401]);
    assert_eq!(info.vmad, Some(vec![1, 2, 3]));
    assert_eq!(info.responses.len(), 2);
    let a = &info.responses[0];
    assert_eq!(
        (a.number, a.emotion, a.emotion_value, a.sound),
        (1, 5, 80, 0x100)
    );
    assert_eq!(a.text, Some(Text::Localized(11)));
    assert_eq!(a.speaker_idle, Some(0x200));
    assert_eq!(a.listener_idle, None);
    let b = &info.responses[1];
    assert_eq!((b.number, b.sound), (3, 0x101));
    assert_eq!(b.text, Some(Text::Localized(12)));
    assert_eq!(b.speaker_idle, None);
    assert_eq!(b.listener_idle, Some(0x201));
    assert_eq!(info.conditions.len(), 1);
    let old = Record::new(b"INFO", 2)
        .sub(b"DATA", words(&[0x12340002, 2.5f32.to_bits()]))
        .sub(b"TRDT", response(1, 0, 0))
        .zsub(b"NAM1", "plain text");
    let old = DialogueInfo::from_record(&old, false).unwrap();
    assert_eq!(old.data.as_ref().unwrap().flags(), 0x1234);
    assert_eq!(old.data.unwrap().reset_hours(), 60.0);
    assert_eq!(
        old.responses[0].text,
        Some(Text::Plain("plain text".into()))
    );
}

#[test]
fn magic_effect_gameplay_fields_follow_documented_offsets() {
    let mut data = vec![0; 152];
    for (at, value) in [
        (0, 0x10400105u32),
        (4, 3.5f32.to_bits()),
        (8, 0x800),
        (12, u32::MAX),
        (16, 41),
        (20, 0xbeef0002),
        (40, 50),
        (44, 12),
        (48, 0.5f32.to_bits()),
        (52, 0.25f32.to_bits()),
        (56, 2f32.to_bits()),
        (60, 0.75f32.to_bits()),
        (64, 5),
        (68, 24),
        (80, 2),
        (84, 3),
        (88, 25),
        (104, 1.5f32.to_bits()),
        (128, 0x900),
        (136, 0xa00),
    ] {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    let rec = Record::new(b"MGEF", 3)
        .sub(b"DATA", data)
        .sub(b"ESCE", 1u32.to_le_bytes())
        .sub(b"ESCE", 2u32.to_le_bytes())
        .sub(b"KWDA", words(&[0x100, 0x101]));
    let effect = MagicEffect::from_record(&rec, false).unwrap();
    let d = effect.data;
    assert_eq!(d.flags, 0x10400105);
    assert_eq!(d.base_cost, 3.5);
    assert_eq!(d.associated_item, 0x800);
    assert_eq!((d.magic_skill, d.resistance), (-1, 41));
    assert_eq!(d.counter_effect_count, 2);
    assert_eq!((d.minimum_skill, d.area), (50, 12));
    assert_eq!(
        (d.casting_time, d.taper_curve, d.taper_duration),
        (0.5, 0.25, 2.0)
    );
    assert_eq!(d.second_actor_value_weight, 0.75);
    assert_eq!(
        (d.archetype, d.primary_actor_value, d.second_actor_value),
        (5, 24, 25)
    );
    assert_eq!((d.cast_type, d.delivery), (2, 3));
    assert_eq!(d.skill_usage_multiplier, 1.5);
    assert_eq!((d.equip_ability, d.perk), (0x900, 0xa00));
    assert_eq!(effect.counter_effects, [1, 2]);
    assert_eq!(effect.keywords, [0x100, 0x101]);
}

#[test]
fn recipe_counts_and_ownership_remain_attached_to_ingredients() {
    let rec = recipe()
        .sub(b"COCT", u32::MAX.to_le_bytes())
        .sub(b"CNTO", words(&[0x101, 3]))
        .sub(b"COED", words(&[0x500, u32::MAX, 0.5f32.to_bits()]))
        .sub(b"CNTO", words(&[0x102, u32::MAX]))
        .sub(b"BNAM", 0x400u32.to_le_bytes())
        .sub(b"CTDA", condition(0, 1f32.to_bits(), 448, 0x600));
    let view = Recipe::from_record(&rec).unwrap();
    assert_eq!(view.declared_ingredient_count, Some(u32::MAX));
    assert_eq!(view.ingredients.len(), 2);
    assert_eq!(
        (view.ingredients[0].item, view.ingredients[0].count),
        (0x101, 3)
    );
    assert_eq!(
        view.ingredients[0].ownership,
        Some(ItemOwnership {
            owner: 0x500,
            global_or_rank: u32::MAX,
            condition: 0.5
        })
    );
    assert_eq!(view.ingredients[1].count, -1);
    assert_eq!(view.ingredients[1].ownership, None);
    assert_eq!(
        (
            view.created_object,
            view.created_count,
            view.workbench_keyword
        ),
        (Some(0x300), Some(2), Some(0x400))
    );
    assert_eq!(view.conditions[0].function, 448);
    assert_eq!(view.conditions[0].parameters[0], 0x600);
}

#[test]
fn malformed_fields_or_orphaned_groups_return_errors() {
    for size in 0..36 {
        let rec = Record::new(b"SPEL", 1).sub(b"SPIT", vec![0; size]);
        assert!(Spell::from_record(&rec, false).is_err());
    }
    for size in 0..152 {
        let rec = Record::new(b"MGEF", 1).sub(b"DATA", vec![0; size]);
        assert!(MagicEffect::from_record(&rec, false).is_err());
    }
    for size in 0..32 {
        assert!(Recipe::from_record(&recipe().sub(b"CTDA", vec![0; size])).is_err());
    }
    for size in 0..24 {
        let rec = Record::new(b"INFO", 1).sub(b"TRDT", vec![0; size]);
        assert!(DialogueInfo::from_record(&rec, false).is_err());
    }
    assert!(Spell::from_record(&spell().sub(b"EFIT", [0; 12]), false).is_err());
    assert!(Spell::from_record(&spell().sub(b"EFID", [0; 4]), false).is_err());
    assert!(Recipe::from_record(&recipe().sub(b"COED", [0; 12])).is_err());
    assert!(Recipe::from_record(&recipe().zsub(b"CIS1", "orphan")).is_err());
    assert_eq!(
        Recipe::from_record(&Record::new(b"COBJ", 1))
            .unwrap()
            .created_object,
        None
    );
    assert!(
        DialogueInfo::from_record(&Record::new(b"INFO", 1).zsub(b"NAM1", "orphan"), false).is_err()
    );
    assert!(
        DialogueTopic::from_record(&Record::new(b"DIAL", 1).sub(b"FULL", [0; 3]), true).is_err()
    );
    let wrong = Record::new(b"WRLD", 1);
    assert!(DialogueTopic::from_record(&wrong, false).is_err());
    assert!(DialogueInfo::from_record(&wrong, false).is_err());
    assert!(Recipe::from_record(&wrong).is_err());
    assert!(Spell::from_record(&wrong, false).is_err());
    assert!(MagicEffect::from_record(&wrong, false).is_err());
}

#[test]
fn recipe_links_resolve_through_the_winning_files_master_list() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(recipe());
    let mut other = Plugin::new(FileHeader::default());
    other.push_top(Record::new(b"WEAP", 0x800));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Other.esm".into(), "Base.esm".into()],
        ..Default::default()
    });
    patch.push_top(
        Record::new(b"COBJ", 0x01000200)
            .sub(b"CNAM", 0x800u32.to_le_bytes())
            .sub(b"CNTO", words(&[0x999, 4]))
            .sub(b"NAM1", 7u16.to_le_bytes())
            .sub(b"ZZZZ", [1, 2, 3]),
    );
    let mut order = LoadOrder::new();
    for (name, p) in [
        ("Base.esm", base),
        ("Other.esm", other),
        ("Patch.esp", patch),
    ] {
        let bytes = p.to_bytes();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let id = GlobalFormId {
        file: 0,
        object: 0x200,
    };
    let winner = order.winner_ref(id).unwrap();
    assert_eq!(winner.file, 2);
    let rec = order.record(winner);
    let before = rec.clone();
    let view = Recipe::from_record(rec).unwrap();
    assert_eq!(view.created_count, Some(7));
    let created = order.resolve(winner.file, view.created_object.unwrap());
    assert_eq!(
        created,
        GlobalFormId {
            file: 1,
            object: 0x800
        }
    );
    assert_eq!(order.winner(created).unwrap().tag, Tag::new(b"WEAP"));
    let missing = order.resolve(winner.file, view.ingredients[0].item);
    assert!(order.winner(missing).is_none());
    assert_eq!(rec, &before);
    assert_eq!(rec.find(Tag::new(b"ZZZZ")), Some([1, 2, 3].as_slice()));
}

#[test]
fn localized_dialogue_and_magic_views_read_patch_values() {
    let bases = [
        Record::new(b"DIAL", 1).zsub(b"FULL", "old topic"),
        Record::new(b"INFO", 2)
            .sub(b"TRDT", response(1, 0, 0))
            .zsub(b"NAM1", "old response"),
        spell().zsub(b"FULL", "old spell"),
        Record::new(b"MGEF", 3)
            .sub(b"DATA", [0; 152])
            .zsub(b"FULL", "old effect"),
    ];
    let patches = [
        Record::new(b"DIAL", 1).sub(b"FULL", 101u32.to_le_bytes()),
        Record::new(b"INFO", 2)
            .sub(b"TRDT", response(7, 2, 0))
            .sub(b"NAM1", 102u32.to_le_bytes()),
        spell().sub(b"FULL", 103u32.to_le_bytes()),
        Record::new(b"MGEF", 3)
            .sub(b"DATA", [0; 152])
            .sub(b"FULL", 104u32.to_le_bytes()),
    ];
    let mut base = Plugin::new(FileHeader::default());
    let mut patch = Plugin::new(FileHeader {
        flags: soe_esm::plugin::flags::LOCALIZED,
        masters: vec!["Base.esm".into()],
        ..Default::default()
    });
    for rec in bases {
        base.push_top(rec);
    }
    for rec in patches {
        patch.push_top(rec);
    }
    let mut order = LoadOrder::new();
    for (name, plugin) in [("Base.esm", base), ("Patch.esp", patch)] {
        order
            .add(name, Plugin::parse(&plugin.to_bytes()).unwrap())
            .unwrap();
    }
    for (object, signature, string_id) in [
        (1, b"DIAL", 101),
        (2, b"INFO", 102),
        (0x100, b"SPEL", 103),
        (3, b"MGEF", 104),
    ] {
        let winner = order.winner_ref(GlobalFormId { file: 0, object }).unwrap();
        assert_eq!(winner.file, 1);
        let rec = order.record(winner);
        let localized = order.file(winner.file).plugin.header.is_localized();
        let text = match signature {
            b"DIAL" => DialogueTopic::from_record(rec, localized).unwrap().name,
            b"INFO" => {
                let view = DialogueInfo::from_record(rec, localized).unwrap();
                assert_eq!(view.responses[0].number, 7);
                view.responses[0].text.clone()
            }
            b"SPEL" => Spell::from_record(rec, localized).unwrap().name,
            b"MGEF" => MagicEffect::from_record(rec, localized).unwrap().name,
            _ => unreachable!(),
        };
        assert_eq!(text, Some(Text::Localized(string_id)));
    }
}

#[test]
fn repeated_group_fields_fail_but_successive_conditions_keep_separate_strings() {
    let c = condition(0, 1f32.to_bits(), 1, 0);
    let rec = recipe().sub(b"CTDA", c.clone()).zsub(b"CIS1", "first");
    assert!(Recipe::from_record(&rec.clone().zsub(b"CIS1", "duplicate")).is_err());
    let rec = rec.sub(b"CTDA", c).zsub(b"CIS1", "second");
    let view = Recipe::from_record(&rec).unwrap();
    assert_eq!(view.conditions[0].strings[0].as_deref(), Some("first"));
    assert_eq!(view.conditions[1].strings[0].as_deref(), Some("second"));
    let rec = spell()
        .sub(b"EFID", [0; 4])
        .sub(b"EFIT", [0; 12])
        .sub(b"EFIT", [0; 12]);
    assert!(Spell::from_record(&rec, false).is_err());
    let rec = recipe()
        .sub(b"CNTO", [0; 8])
        .sub(b"COED", [0; 12])
        .sub(b"COED", [0; 12]);
    assert!(Recipe::from_record(&rec).is_err());
}
