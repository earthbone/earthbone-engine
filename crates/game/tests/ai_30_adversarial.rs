// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-30 Guard area eligibility.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::package::{self, Location, Schedule};
use soe_game::World;

const GUARD_BASE: u32 = 0x801;
const INTRUDER_BASE: u32 = 0x802;
const POST: u32 = 0x803;
const TEMPLATE: u32 = 0x850;
const PACKAGE: u32 = 0x851;
const GUARD: u32 = 0x900;
const INTRUDER: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn actor_base(object: u32, aggression: u8) -> Record {
    let mut data = vec![aggression, 4, 70, 1, 3, 2, 0, 0];
    for radius in [500_u32, 300, 100] {
        data.extend_from_slice(&radius.to_le_bytes());
    }
    Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"AIDT"), data)
}

fn actor_ref(object: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::with_capacity(24);
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    Record::new(Tag(*b"ACHR"), object)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(Tag(*b"DATA"), data)
}

fn guard_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(actor_base(GUARD_BASE, 1).sub(Tag(*b"PKID"), PACKAGE.to_le_bytes()));
    plugin.push_top(actor_base(INTRUDER_BASE, 0));
    plugin.push_top(actor_ref(GUARD, GUARD_BASE, [20.0, 20.0, 0.0]));
    plugin.push_top(actor_ref(INTRUDER, INTRUDER_BASE, [40.0, 20.0, 0.0]));
    plugin.push_top(actor_ref(POST, INTRUDER_BASE, [20.0, 20.0, 0.0]));

    let template = package::build_template(TEMPLATE, "GuardTemplate");
    let template = package::close_header(template, None);
    let template = package::with_location_input(template, 0, Location::NearReference(POST), 10);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Guard", &[0], true);
    let template = package::with_input_name(template, 0, "Guard Area", true);
    plugin.push_top(package::with_actions(package::finish(template)));
    let record = package::build_record(PACKAGE, "GuardPackage", 0, Schedule::default());
    let record = package::close_header(record, Some(TEMPLATE));
    let record = package::with_location_input(record, 0, Location::NearReference(POST), 10);
    plugin.push_top(package::with_actions(package::finish(record)));

    let mut order = LoadOrder::new();
    order.add("Ai30Adversarial.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.actor(id(GUARD)).unwrap().level = 5;
    world.ai_tick(0.0, &mut |_, _| soe_papyrus::value::ObjectId(0));
    assert_eq!(
        world.actor(id(GUARD)).unwrap().ai.procedure,
        Procedure::Guard
    );
    world
}

#[test]
fn ai_30_guard_drops_a_combat_target_after_it_leaves_the_assigned_radius() {
    let mut world = guard_world();
    {
        let actor = world.actor(id(GUARD)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(INTRUDER));
    }
    world.reference(id(INTRUDER)).unwrap().position = [200.0, 20.0, 0.0];

    world.ai_tick(0.1, &mut |_, _| soe_papyrus::value::ObjectId(0));

    let (in_combat, combat_target) = {
        let actor = world.actor(id(GUARD)).unwrap();
        (actor.in_combat, actor.combat_target)
    };
    let position = world.reference(id(GUARD)).unwrap().position;
    assert!(
        !in_combat && combat_target.is_none(),
        "outside-radius intrusion remains an active guard target"
    );
    assert!(
        (position[0] - 20.0).abs() <= 10.0 && (position[1] - 20.0).abs() <= 10.0,
        "guard left its assigned area while pursuing {position:?}"
    );
}
