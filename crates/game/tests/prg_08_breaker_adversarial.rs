// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    perks::{EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const PERK: u32 = 0x800;
const NPC: u32 = 0x801;
const ACTOR: u32 = 0x802;
const ENTRY: u8 = 35;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn record_rank(world: &mut World, invocation: &EntryPointInvocation) {
    world
        .set_actor_value(invocation.actor, "health", f32::from(invocation.rank))
        .unwrap();
}

fn context(actor: GlobalFormId) -> EntryPointContext {
    EntryPointContext {
        tabs: vec![actor],
        target: None,
        quest: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        skill_use: None,
    }
}

#[test]
fn npc_prkr_rank_makes_matching_prke_rank_eligible() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"PERK"), PERK)
            .sub(Tag(*b"DATA"), [0, 0, 2, 1, 0])
            .sub(Tag(*b"PRKE"), [2, 1, 0])
            .sub(Tag(*b"DATA"), [ENTRY, 1, 0])
            .sub(Tag(*b"EPFT"), [1])
            .sub(Tag(*b"EPFD"), 3.0f32.to_le_bytes())
            .sub(Tag(*b"PRKF"), []),
    );
    // TES5Edit defines PRKR as a perk FormID, a rank byte, then three unused bytes.
    plugin.push_top(
        npc::build_record(NPC, "RankedActor", "Ranked Actor", Config::default(), Stats::default())
            .sub(Tag(*b"PRKR"), [0x00, 0x08, 0, 0, 2, 0, 0, 0]),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), NPC.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("PerkRank.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = id(ACTOR);
    game.world_mut()
        .set_actor_value(actor, "health", 100.0)
        .unwrap();

    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, record_rank);
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 2.0);
}
