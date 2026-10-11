// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{EventSink, NotifyControl, TesObjectLoadedEvent};
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const CELL: u32 = 0x810;
const REFERENCE: u32 = 0x811;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
    let cell_record =
        plugin.push_record(Record::new(Tag(*b"CELL"), CELL).sub(Tag(*b"DATA"), [1_u8, 0]));
    let reference_record = plugin.push_record(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0_u8; 24]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(reference_record));
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
    order.add("Shm22Residency.esp", plugin).unwrap();
    Game::new(order)
}

struct Capture(Rc<RefCell<Vec<TesObjectLoadedEvent>>>);

impl EventSink<TesObjectLoadedEvent> for Capture {
    fn process(&mut self, event: &TesObjectLoadedEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn shm_22_object_loaded_tracks_enabled_three_d_residency_once() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .object_loaded()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut().reference(id(REFERENCE)).unwrap().enabled = false;

    game.world_mut()
        .attach_streaming_cells(&[id(CELL)])
        .unwrap();
    assert!(seen.borrow().is_empty());

    game.world_mut().set_reference_enabled(id(REFERENCE), true);
    game.world_mut().set_reference_enabled(id(REFERENCE), true);
    game.world_mut().set_reference_enabled(id(REFERENCE), false);
    game.world_mut().set_reference_enabled(id(REFERENCE), false);

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
