// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{
    EventSink, NotifyControl, TesGrabReleaseEvent, TesHarvestedEvent, TesHitEvent,
    TesObjectLoadedEvent, TesQuestStageEvent, UnsupportedHook,
};
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, quest::entry_flags, Game};

const CELL: u32 = 0x810;
const REFERENCE: u32 = 0x811;
const DISABLED_REFERENCE: u32 = 0x812;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon() -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    Record::new(Tag(*b"WEAP"), 0xA00)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0xA10));
    plugin.push_top(
        Record::new(Tag(*b"QUST"), 0x820)
            .sub(Tag(*b"DNAM"), [0; 12])
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0, 0, 0, 0])
            .sub(Tag(*b"INDX"), [1, 0, 0, 0])
            .sub(Tag(*b"QSDT"), [entry_flags::FAIL_QUEST]),
    );
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let cell_record =
        plugin.push_record(Record::new(Tag(*b"CELL"), CELL).sub(Tag(*b"DATA"), [1_u8, 0]));
    let references = [REFERENCE, DISABLED_REFERENCE].map(|reference| {
        plugin.push_record(
            Record::new(Tag(*b"REFR"), reference)
                .sub(Tag(*b"NAME"), 0xA10_u32.to_le_bytes())
                .sub(Tag(*b"DATA"), [0_u8; 24]),
        )
    });
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.extend(references.map(Entry::Record));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell_record));
    block.entries.push(Entry::Group(children));
    let mut interior = Group::new(GroupKind::InteriorBlock(0));
    interior.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(Tag(*b"CELL")));
    top.entries.push(Entry::Group(interior));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order
        .add("Shm22.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901] {
        let mut world = game.world_mut();
        world.set_actor_value(id(actor), "health", 100.).unwrap();
        world.set_actor_value(id(actor), "onehanded", 100.).unwrap();
        world.add_item(id(actor), id(0xA00), 1);
        world.actor(id(actor)).unwrap().equipped.insert(id(0xA00));
    }
    game
}

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Clone> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

#[test]
fn shm_22_grab_repeats_are_suppressed_and_handoffs_release_first() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesGrabReleaseEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .grab_release()
        .add_sink(Box::new(Capture(seen.clone())));
    let mut world = game.world_mut();
    world.set_grabbed_reference(Some(id(REFERENCE)));
    world.set_grabbed_reference(Some(id(REFERENCE)));
    world.set_grabbed_reference(Some(id(DISABLED_REFERENCE)));
    assert_eq!(
        *seen.borrow(),
        [
            TesGrabReleaseEvent {
                reference: soe_commonlib_shim::refr::ObjectRefHandle(REFERENCE),
                grabbed: true,
            },
            TesGrabReleaseEvent {
                reference: soe_commonlib_shim::refr::ObjectRefHandle(REFERENCE),
                grabbed: false,
            },
            TesGrabReleaseEvent {
                reference: soe_commonlib_shim::refr::ObjectRefHandle(DISABLED_REFERENCE),
                grabbed: true,
            },
        ]
    );
}

#[test]
fn shm_22_rejected_harvests_do_not_emit_or_change_inventory() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesHarvestedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .harvested()
        .add_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    let mut world = game.world_mut();
    assert!(!world.harvest_item(id(0x900), id(0xA10), 0, &mut allocate));
    assert!(!world.harvest_item(id(0x900), id(0xDEAD), 1, &mut allocate));
    assert!(!world.harvest_item(id(0xDEAD), id(0xA10), 1, &mut allocate));
    assert!(seen.borrow().is_empty());
    assert_eq!(world.item_count(id(0x900), id(0xA10)), 0);
    assert!(world.harvest_item(id(0x900), id(0xA10), 2, &mut allocate));
    assert!(world.harvest_item(id(0x900), id(0xA10), 1, &mut allocate));
    assert_eq!(world.item_count(id(0x900), id(0xA10)), 3);
    assert_eq!(seen.borrow().len(), 2);
}

#[test]
fn shm_22_hit_events_skip_rejected_contacts_and_keep_source_identity() {
    let mut game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesHitEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .hit()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(game
        .melee_hit(MeleeHit {
            attacker: id(0x900),
            target: id(0x900),
            weapon: id(0xA00),
            is_bash: false,
            is_power_attack: true,
        })
        .is_err());
    assert!(seen.borrow().is_empty());
    game.melee_hit(MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    })
    .unwrap();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].source.0, 0xA00);
    assert_eq!(
        seen.borrow()[0].flags,
        soe_commonlib_shim::events::HitFlags::default()
    );
}

#[test]
fn shm_22_loaded_events_ignore_disabled_refs_and_deduplicate_residency() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesObjectLoadedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .object_loaded()
        .add_sink(Box::new(Capture(seen.clone())));
    {
        let mut world = game.world_mut();
        world.set_reference_enabled(id(DISABLED_REFERENCE), false);
        world.attach_streaming_cells(&[id(CELL)]).unwrap();
        world.attach_streaming_cells(&[id(CELL)]).unwrap();
        world.detach_streaming_cells(&[id(CELL)]).unwrap();
    }
    assert_eq!(
        *seen.borrow(),
        [
            TesObjectLoadedEvent {
                form_id: soe_commonlib_shim::form::FormId(REFERENCE),
                loaded: true,
            },
            TesObjectLoadedEvent {
                form_id: soe_commonlib_shim::form::FormId(REFERENCE),
                loaded: false,
            },
        ]
    );
}

#[test]
fn shm_22_defined_quest_stages_emit_and_binary_hooks_stay_unsupported() {
    let mut game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesQuestStageEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .quest_stage()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(!game.set_stage(id(0xDEAD), 1));
    assert!(!game.set_stage(id(0x820), 99));
    assert!(seen.borrow().is_empty());
    assert!(game.set_stage(id(0x820), 1));
    assert_eq!(
        *seen.borrow(),
        [TesQuestStageEvent {
            quest: soe_commonlib_shim::form::FormId(0x820),
            stage: 1,
            item_index: 0,
        }]
    );
    let error = game
        .world()
        .reject_commonlib_hook("test call site")
        .unwrap_err();
    assert_eq!(error, UnsupportedHook::new("test call site"));
}
