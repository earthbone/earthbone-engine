// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SHM-20 event bindings from successful runtime operations.

use std::{
    cell::RefCell,
    fs,
    io::Write,
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    rc::Rc,
};

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

struct CppConsumer(Rc<RefCell<ChildStdin>>);

impl CppConsumer {
    fn send(&self, kind: u8, first: u32, second: u32) {
        let mut input = self.0.borrow_mut();
        input
            .write_all(&[kind])
            .expect("write event kind to C++ fixture");
        input
            .write_all(&first.to_le_bytes())
            .expect("write first event field to C++ fixture");
        input
            .write_all(&second.to_le_bytes())
            .expect("write second event field to C++ fixture");
    }
}

impl EventSink<ShoutAttack> for CppConsumer {
    fn process(&mut self, event: &ShoutAttack) -> NotifyControl {
        self.send(1, event.shout.0, 0);
        NotifyControl::Continue
    }
}

impl EventSink<SkillIncrease> for CppConsumer {
    fn process(&mut self, event: &SkillIncrease) -> NotifyControl {
        self.send(2, event.player.0, event.actor_value.0);
        NotifyControl::Continue
    }
}

impl EventSink<SoulsTrapped> for CppConsumer {
    fn process(&mut self, event: &SoulsTrapped) -> NotifyControl {
        self.send(3, event.trapper.0, event.target.0);
        NotifyControl::Continue
    }
}

impl EventSink<SpellsLearned> for CppConsumer {
    fn process(&mut self, event: &SpellsLearned) -> NotifyControl {
        self.send(4, event.spell.0, 0);
        NotifyControl::Continue
    }
}

impl EventSink<TESCellFullyLoadedEvent> for CppConsumer {
    fn process(&mut self, event: &TESCellFullyLoadedEvent) -> NotifyControl {
        self.send(5, event.cell.0, 0);
        NotifyControl::Continue
    }
}

fn cpp_consumer() -> (std::process::Child, Rc<RefCell<ChildStdin>>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-20-live-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("create C++ fixture directory");
    let source = scratch.join("live_events.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <memory>

struct LiveState {
    unsigned count{};
    bool valid{true};
    void receive(const RE::ShoutAttack& event) {
        valid = valid && event.shout.value == 0x803;
    }
    void receive(const RE::SkillIncrease& event) {
        valid = valid && event.player.value != 0 && event.actor_value.value == 6;
    }
    void receive(const RE::SoulsTrapped& event) {
        valid = valid && event.trapper.value == 0x110 && event.target.value == 0x111;
    }
    void receive(const RE::SpellsLearned& event) {
        valid = valid && event.spell.value == 0x801;
    }
    void receive(const RE::TESCellFullyLoadedEvent& event) {
        valid = valid && event.cell.value == 0x900;
    }
};

template <class Event>
struct Sink final : RE::EventSink<Event> {
    LiveState& state;
    explicit Sink(LiveState& value) : state(value) {}
    RE::NotifyControl process(const Event& event) override {
        state.receive(event);
        ++state.count;
        return RE::NotifyControl::Continue;
    }
};

int main() {
    RE::ScriptEventSourceHolder sources;
    LiveState state;
    sources.shout_attack().add_sink(std::make_unique<Sink<RE::ShoutAttack>>(state));
    sources.skill_increase().add_sink(std::make_unique<Sink<RE::SkillIncrease>>(state));
    sources.souls_trapped().add_sink(std::make_unique<Sink<RE::SoulsTrapped>>(state));
    sources.spells_learned().add_sink(std::make_unique<Sink<RE::SpellsLearned>>(state));
    sources.cell_fully_loaded().add_sink(
        std::make_unique<Sink<RE::TESCellFullyLoadedEvent>>(state));

    std::uint8_t kind{};
    std::uint32_t first{};
    std::uint32_t second{};
    for (unsigned index = 0; index < 5; ++index) {
        if (std::fread(&kind, sizeof(kind), 1, stdin) != 1) return 1;
        if (std::fread(&first, sizeof(first), 1, stdin) != 1 ||
            std::fread(&second, sizeof(second), 1, stdin) != 1) return 1;
        switch (kind) {
        case 1: sources.shout_attack().send(RE::ShoutAttack{{first}}); break;
        case 2: sources.skill_increase().send(RE::SkillIncrease{{first}, {second}}); break;
        case 3: sources.souls_trapped().send(RE::SoulsTrapped{{first}, {second}}); break;
        case 4: sources.spells_learned().send(RE::SpellsLearned{{first}}); break;
        case 5: sources.cell_fully_loaded().send(RE::TESCellFullyLoadedEvent{{first}}); break;
        default: return 2;
        }
    }
    if (state.count != 5 || !state.valid) return 3;
    const auto hook = RE::request_hook(RE::HookKind::Vtable);
    if (hook.code != RE::HookErrorCode::Unsupported || hook.kind != RE::HookKind::Vtable) return 4;
    return std::strcmp(SKSE::TRAMPOLINE.what,
        "SKSE::Trampoline patches machine code and has no target in this engine") == 0 ? 0 : 5;
}
"#,
    )
    .expect("write C++ live event fixture");
    let executable = scratch.join("live_events");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("../commonlib-shim/include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by the SHM-20 done check");
    assert!(
        compile.status.success(),
        "live C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .spawn()
        .expect("start C++ live event consumer");
    let input = Rc::new(RefCell::new(
        child.stdin.take().expect("piped fixture stdin"),
    ));
    (child, input)
}

#[test]
fn shm_20_live_world_events_reach_the_compiled_mit_event_sinks() {
    let (mut cpp, cpp_input) = cpp_consumer();

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
    order.add("Shm20CppShout.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.player();
    {
        let mut world = game.world_mut();
        let state = world.actor(actor).unwrap();
        state.shouts.insert(id(0x803));
        state.equipped_shout = Some(id(0x803));
    }
    game.world_mut()
        .commonlib_event_sources()
        .shout_attack()
        .add_sink(Box::new(CppConsumer(cpp_input.clone())));
    game.world_mut()
        .cast_shout(actor, None, &mut |_, _| soe_papyrus::ObjectId(0))
        .unwrap();

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
    order.add("Shm20CppSkill.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .commonlib_event_sources()
        .skill_increase()
        .add_sink(Box::new(CppConsumer(cpp_input.clone())));
    game.world_mut()
        .advance_skill(player, "onehanded", 10.0)
        .unwrap();

    let mut game = soul_trap_game();
    game.world_mut()
        .commonlib_event_sources()
        .souls_trapped()
        .add_sink(Box::new(CppConsumer(cpp_input.clone())));
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
    order.add("Shm20CppSpell.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .commonlib_event_sources()
        .spells_learned()
        .add_sink(Box::new(CppConsumer(cpp_input.clone())));
    assert!(game.world_mut().add_actor_spell(player, id(0x801)));

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 0x900));
    let mut order = LoadOrder::new();
    order.add("Shm20CppCell.esp", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .commonlib_event_sources()
        .cell_fully_loaded()
        .add_sink(Box::new(CppConsumer(cpp_input.clone())));
    game.world_mut()
        .attach_streaming_cells(&[id(0x900)])
        .unwrap();

    drop(cpp_input);
    assert!(cpp.wait().expect("wait for C++ event consumer").success());
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
