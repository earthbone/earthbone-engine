// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACK: u32 = 0x901;
const BASE: u32 = 0x902;
const MARKER_BASE: u32 = 0x903;
const ACTOR: u32 = 0x904;
const MARKER: u32 = 0x906;
const CELL: u32 = 0x910;
const IDLE: u32 = 0x912;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = x.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 20]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "Idle"), None);
    let template = package::with_location_input(template, 0, Location::NearSelf, 100);
    let template = package::finish(template);
    let template = package::with_procedure(template, "UseIdleMarker", &[0], true);
    plugin.push_top(package::with_actions(template));
    let pack = package::close_header(
        package::build_record(PACK, "IdlePackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(pack)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "IdleActor",
            "Idle actor",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"IDLM"), MARKER_BASE).sub(T(b"IDLA"), IDLE.to_le_bytes()));
    plugin.push_top(
        Record::new(T(b"IDLE"), IDLE)
            .zsub(T(b"EDID"), "IdleEditorId")
            .zsub(T(b"ENAM"), "IdleAnimationEvent"),
    );
    let cell_record = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    for record in [
        placed(*b"ACHR", ACTOR, BASE, 0.0),
        placed(*b"REFR", MARKER, MARKER_BASE, 20.0),
    ] {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell_record));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order
        .add("Idle.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

#[test]
fn ai_23_queues_the_idle_animation_event_from_enam() {
    let mut game = game();
    game.tick(1.0);
    let requests = &game.world().animation_requests;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target, id(ACTOR));
    assert_eq!(requests[0].animation, "IdleAnimationEvent");
}
