// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{ActorKill, CriticalHit, EventFormId, EventSink, NotifyControl};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 1, object }
}

fn game() -> Game {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"NPC_"), 0x800));

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..Default::default()
    });
    for object in [0x900, 0x901, 0x902] {
        patch.push_top(
            Record::new(Tag(*b"ACHR"), 0x0100_0000 | object)
                .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    patch.push_top(Record::new(Tag(*b"WEAP"), 0x0100_0A00));
    patch.push_top(Record::new(Tag(*b"MISC"), 0x0100_0A01));

    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    Game::new(order)
}

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Copy> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn live_event_payloads_keep_the_plugin_file_index_and_full_object_id() {
    let game = game();
    let kills = Rc::new(RefCell::new(Vec::<ActorKill>::new()));
    let hits = Rc::new(RefCell::new(Vec::<CriticalHit>::new()));
    game.world_mut()
        .actor_kill_events()
        .add_sink(Box::new(Capture(kills.clone())));
    game.world_mut()
        .critical_hit_events()
        .add_sink(Box::new(Capture(hits.clone())));

    assert!(game.world_mut().kill_actor(id(0x900), Some(id(0x901))));
    game.world_mut()
        .report_critical_hit(id(0x902), id(0xA00), true)
        .unwrap();

    assert_eq!(
        &*kills.borrow(),
        &[ActorKill {
            killer: EventFormId((1_u64 << 32) | 0x901),
            victim: EventFormId((1_u64 << 32) | 0x900),
        }]
    );
    assert_eq!(
        &*hits.borrow(),
        &[CriticalHit {
            aggressor: EventFormId((1_u64 << 32) | 0x902),
            weapon: EventFormId((1_u64 << 32) | 0xA00),
            sneak_hit: true,
        }]
    );
}

#[test]
fn rejected_critical_hit_inputs_do_not_emit_events() {
    let game = game();
    let hits = Rc::new(RefCell::new(Vec::<CriticalHit>::new()));
    game.world_mut()
        .critical_hit_events()
        .add_sink(Box::new(Capture(hits.clone())));

    assert!(game
        .world_mut()
        .report_critical_hit(id(0xFFFF), id(0xA00), false)
        .is_err());
    assert!(game
        .world_mut()
        .report_critical_hit(id(0x902), id(0xA01), false)
        .is_err());
    assert!(hits.borrow().is_empty());
}

#[test]
fn repeated_valid_critical_reports_preserve_each_caller_confirmed_outcome() {
    let game = game();
    let hits = Rc::new(RefCell::new(Vec::<CriticalHit>::new()));
    game.world_mut()
        .critical_hit_events()
        .add_sink(Box::new(Capture(hits.clone())));

    for sneak_hit in [false, true, false] {
        game.world_mut()
            .report_critical_hit(id(0x902), id(0xA00), sneak_hit)
            .unwrap();
    }

    assert_eq!(
        hits.borrow().as_slice(),
        [false, true, false].map(|sneak_hit| CriticalHit {
            aggressor: EventFormId((1_u64 << 32) | 0x902),
            weapon: EventFormId((1_u64 << 32) | 0xA00),
            sneak_hit,
        })
    );
}
