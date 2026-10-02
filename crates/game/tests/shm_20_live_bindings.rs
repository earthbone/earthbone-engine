// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SHM-20 event bindings from successful runtime operations.

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::{
    events::{
        EventSink, NotifyControl, ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned,
        TESCellFullyLoadedEvent,
    },
    form::FormId,
    refr::{ActorHandle, ActorValue},
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x800;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Clone> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

fn soul_trap_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for base in [0x100_u32, 0x101] {
        plugin.push_top(Record::new(T(b"NPC_"), base).sub(T(b"ACBS"), vec![0; 24]));
    }
    for (reference, base) in [(0x110_u32, 0x100_u32), (0x111, 0x101)] {
        plugin.push_top(
            Record::new(T(b"ACHR"), reference)
                .sub(T(b"NAME"), base.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    let mut effect_data = vec![0; 152];
    effect_data[64..68].copy_from_slice(&23u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), effect_data));
    let mut spell_data = vec![0; 36];
    spell_data[20] = 3;
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x300)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), 0x200u32.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    1.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    30u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    for (empty, filled, capacity) in [(0x400_u32, 0x401_u32, 2_u8), (0x402, 0x403, 3)] {
        plugin.push_top(
            Record::new(T(b"SLGM"), empty)
                .sub(T(b"SOUL"), [0])
                .sub(T(b"SLCP"), [capacity])
                .sub(T(b"NAM0"), filled.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(T(b"SLGM"), filled)
                .sub(T(b"SOUL"), [capacity])
                .sub(T(b"SLCP"), [capacity]),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Shm20.esp", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x110, 0x111] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game.world_mut().actor(id(0x111)).unwrap().level = 18;
    game.world_mut()
        .reference(id(0x110))
        .unwrap()
        .inventory
        .add(id(0x402), 1)
        .unwrap();
    game
}

#[test]
fn shm_20_shout_cast_emits_the_equipped_live_shout() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        ACTOR,
        "Shouter",
        "Shouter",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(Record::new(T(b"MGEF"), 0x801).sub(T(b"DATA"), {
        let mut data = vec![0; 152];
        data[0..4].copy_from_slice(&6u32.to_le_bytes());
        data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        data[68..72].copy_from_slice(&24u32.to_le_bytes());
        data
    }));
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x802)
            .sub(T(b"SPIT"), vec![0; 36])
            .sub(T(b"EFID"), 0x801u32.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    10.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    0u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    let mut shout = Record::new(T(b"SHOU"), 0x803);
    for index in 0..3 {
        let mut word = vec![0; 12];
        if index == 2 {
            word[4..8].copy_from_slice(&0x802u32.to_le_bytes());
            word[8..12].copy_from_slice(&4.0f32.to_le_bytes());
        }
        shout = shout.sub(T(b"SNAM"), word);
    }
    plugin.push_top(shout);
    let mut order = LoadOrder::new();
    order.add("Shm20Shout.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.player();
    {
        let mut world = game.world_mut();
        let state = world.actor(actor).unwrap();
        state.shouts.insert(id(0x803));
        state.equipped_shout = Some(id(0x803));
    }
    let seen = Rc::new(RefCell::new(Vec::<ShoutAttack>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .shout_attack()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut()
        .cast_shout(actor, None, &mut |_, _| soe_papyrus::ObjectId(0))
        .unwrap();
    assert_eq!(
        seen.borrow().as_slice(),
        &[ShoutAttack {
            shout: FormId(game.world().order().runtime_id(id(0x803)))
        }]
    );
}

#[test]
fn shm_20_player_skill_rank_gain_emits_live_player_and_actor_value() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        ACTOR,
        "Learner",
        "Learner",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"AVIF"), 0x801)
            .sub(T(b"EDID"), b"AVOneHanded\0".to_vec())
            .sub(
                T(b"AVSK"),
                [2.0f32, 1.0, 0.2, 2.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x802)
            .sub(T(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(T(b"DATA"), 2.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Shm20Skill.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let seen = Rc::new(RefCell::new(Vec::<SkillIncrease>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .skill_increase()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut()
        .advance_skill(player, "onehanded", 10.0)
        .unwrap();
    assert_eq!(
        seen.borrow().as_slice(),
        &[SkillIncrease {
            player: ActorHandle(game.world().order().runtime_id(player)),
            actor_value: ActorValue(6),
        }]
    );
}

#[test]
fn shm_20_successful_soul_capture_emits_live_trapper_and_target() {
    let mut game = soul_trap_game();
    let seen = Rc::new(RefCell::new(Vec::<SoulsTrapped>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .souls_trapped()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut()
        .cast_spell(
            id(0x300),
            id(0x110),
            Some(id(0x110)),
            Some(id(0x111)),
            &mut |_, _| soe_papyrus::ObjectId(0),
        )
        .unwrap();
    assert!(game.world_mut().kill_actor(id(0x111), Some(id(0x110))));
    game.pump();
    game.pump();
    assert_eq!(
        seen.borrow().as_slice(),
        &[SoulsTrapped {
            trapper: ActorHandle(game.world().order().runtime_id(id(0x110))),
            target: ActorHandle(game.world().order().runtime_id(id(0x111))),
        }]
    );
}

#[test]
fn shm_20_new_player_spell_emits_live_spell_form() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        ACTOR,
        "Learner",
        "Learner",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(Record::new(T(b"SPEL"), 0x801));
    let mut order = LoadOrder::new();
    order.add("Shm20Spell.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let seen = Rc::new(RefCell::new(Vec::<SpellsLearned>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .spells_learned()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(game.world_mut().add_actor_spell(player, id(0x801)));
    assert!(!game.world_mut().add_actor_spell(player, id(0x801)));
    assert_eq!(
        seen.borrow().as_slice(),
        &[SpellsLearned {
            spell: FormId(game.world().order().runtime_id(id(0x801)))
        }]
    );
}

#[test]
fn shm_20_streamed_cell_arrival_emits_live_cell_form() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 0x900));
    let mut order = LoadOrder::new();
    order.add("Shm20Cell.esp", plugin).unwrap();
    let game = Game::new(order);
    let seen = Rc::new(RefCell::new(Vec::<TESCellFullyLoadedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .cell_fully_loaded()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut()
        .attach_streaming_cells(&[id(0x900)])
        .unwrap();
    assert_eq!(
        seen.borrow().as_slice(),
        &[TESCellFullyLoadedEvent {
            cell: FormId(game.world().order().runtime_id(id(0x900)))
        }]
    );
}

#[test]
fn shm_20_binary_hook_rejection_remains_explicit() {
    let game = Game::new(LoadOrder::new());
    assert_eq!(
        game.world()
            .reject_commonlib_hook("write_vfunc")
            .unwrap_err()
            .name,
        "write_vfunc"
    );
}
