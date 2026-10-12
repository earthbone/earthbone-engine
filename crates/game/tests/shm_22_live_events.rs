// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::rc::Rc;

use soe_commonlib_shim::events::{
    EventSink, NotifyControl, TesGrabReleaseEvent, TesHarvestedEvent, TesHitEvent,
    TesObjectLoadedEvent, TesQuestStageEvent,
};
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, quest::entry_flags, Game};

const CELL: u32 = 0x810;
const REFERENCE: u32 = 0x811;

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
    let quest = Record::new(Tag(*b"QUST"), 0x820)
        .sub(Tag(*b"DNAM"), [0; 12])
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"INDX"), [0, 0, 0, 0])
        .sub(Tag(*b"INDX"), [1, 0, 0, 0])
        .sub(Tag(*b"QSDT"), [entry_flags::FAIL_QUEST]);
    plugin.push_top(quest);
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
    let reference_record = plugin.push_record(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), 0xA10_u32.to_le_bytes())
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

trait WriteLiveEvent {
    fn write_live(&self, writer: &mut ChildStdin);
}

struct CppConsumer<T>(Rc<RefCell<ChildStdin>>, std::marker::PhantomData<T>);

impl<T: WriteLiveEvent> EventSink<T> for CppConsumer<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        event.write_live(&mut self.0.borrow_mut());
        NotifyControl::Continue
    }
}

macro_rules! write_event {
    ($event:ty, $tag:expr, $this:ident; $($field:expr),+ $(,)?) => {
        impl WriteLiveEvent for $event {
            fn write_live(&self, writer: &mut ChildStdin) {
                let $this = self;
                writer.write_all(&[$tag]).expect("send event tag to C++ fixture");
                $(writer.write_all(&$field.to_le_bytes()).expect("send live event field to C++ fixture");)+
            }
        }
    };
}

write_event!(
    TesGrabReleaseEvent,
    1,
    event;
    event.reference.0,
    u8::from(event.grabbed)
);
write_event!(TesHarvestedEvent, 2, event; event.produce_item.0, event.harvester.0);
write_event!(
    TesHitEvent,
    3,
    event;
    event.target.0,
    event.cause.0,
    event.source.0,
    event.projectile.0,
    event.flags.0,
);
write_event!(
    TesObjectLoadedEvent,
    4,
    event;
    event.form_id.0,
    u8::from(event.loaded)
);
write_event!(
    TesQuestStageEvent,
    5,
    event;
    event.quest.0,
    event.stage,
    event.item_index,
);

fn cpp_event_consumer() -> (std::process::Child, Rc<RefCell<ChildStdin>>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-22-live-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("create C++ fixture directory");
    let source = scratch.join("live_events.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <array>
#include <cstdio>
#include <cstring>
#include <memory>

template <class T> bool read(T& value) { return std::fread(&value, sizeof(value), 1, stdin) == 1; }

struct State { unsigned events{}; bool valid{true}; };
template <class Event> struct Sink final : RE::EventSink<Event> {
    State& state;
    explicit Sink(State& value) : state(value) {}
    RE::NotifyControl process(const Event& event) override;
};

template <> RE::NotifyControl Sink<RE::TESGrabReleaseEvent>::process(const RE::TESGrabReleaseEvent& e) {
    state.valid = state.valid && e.reference.value == 0x811 && e.grabbed;
    ++state.events; return RE::NotifyControl::Continue;
}
template <> RE::NotifyControl Sink<RE::TESHarvestedEvent>::process(const RE::TESHarvestedEvent& e) {
    state.valid = state.valid && e.produceItem.value == 0xA10 && e.harvester.value == 0x900;
    ++state.events; return RE::NotifyControl::Continue;
}
template <> RE::NotifyControl Sink<RE::TESHitEvent>::process(const RE::TESHitEvent& e) {
    state.valid = state.valid && e.target.value == 0x901 && e.cause.value == 0x900 &&
        e.source.value == 0xA00 && e.projectile.value == 0 && e.flags.value == 0;
    ++state.events; return RE::NotifyControl::Continue;
}
template <> RE::NotifyControl Sink<RE::TESObjectLoadedEvent>::process(const RE::TESObjectLoadedEvent& e) {
    state.valid = state.valid && e.formID.value == 0x811 && e.loaded;
    ++state.events; return RE::NotifyControl::Continue;
}
template <> RE::NotifyControl Sink<RE::TESQuestStageEvent>::process(const RE::TESQuestStageEvent& e) {
    state.valid = state.valid && e.formID.value == 0x820 && e.stage == 1 && e.itemIndex == 0;
    ++state.events; return RE::NotifyControl::Continue;
}

int main() {
    RE::ScriptEventSourceHolder sources;
    State state;
    sources.grab_release().add_sink(std::make_unique<Sink<RE::TESGrabReleaseEvent>>(state));
    sources.harvested().add_sink(std::make_unique<Sink<RE::TESHarvestedEvent>>(state));
    sources.hit().add_sink(std::make_unique<Sink<RE::TESHitEvent>>(state));
    sources.object_loaded().add_sink(std::make_unique<Sink<RE::TESObjectLoadedEvent>>(state));
    sources.quest_stage().add_sink(std::make_unique<Sink<RE::TESQuestStageEvent>>(state));
    std::uint8_t kind{};
    while (read(kind)) {
        switch (kind) {
        case 1: { std::uint32_t ref{}; std::uint8_t grabbed{}; if (!read(ref) || !read(grabbed)) return 1;
            sources.grab_release().send(RE::TESGrabReleaseEvent{{ref}, grabbed != 0}); break; }
        case 2: { std::uint32_t item{}, actor{}; if (!read(item) || !read(actor)) return 2;
            sources.harvested().send(RE::TESHarvestedEvent{{item}, {actor}}); break; }
        case 3: { std::uint32_t target{}, cause{}, source{}, projectile{}; std::uint8_t flags{};
            if (!read(target) || !read(cause) || !read(source) || !read(projectile) || !read(flags)) return 3;
            sources.hit().send(RE::TESHitEvent{{target}, {cause}, {source}, {projectile}, {flags}}); break; }
        case 4: { std::uint32_t form{}; std::uint8_t loaded{}; if (!read(form) || !read(loaded)) return 4;
            sources.object_loaded().send(RE::TESObjectLoadedEvent{{form}, loaded != 0}); break; }
        case 5: { std::uint32_t quest{}; std::uint16_t stage{}; std::uint8_t item{};
            if (!read(quest) || !read(stage) || !read(item)) return 5;
            sources.quest_stage().send(RE::TESQuestStageEvent{{quest}, stage, item}); break; }
        default: return 6;
        }
    }
    const auto call = RE::request_hook(RE::HookKind::CallSite);
    const auto branch = RE::request_hook(RE::HookKind::Branch);
    const auto vtable = RE::request_hook(RE::HookKind::Vtable);
    return state.events == 5 && state.valid && call.code == RE::HookErrorCode::Unsupported &&
        branch.code == RE::HookErrorCode::Unsupported && vtable.code == RE::HookErrorCode::Unsupported &&
        std::strcmp(SKSE::TRAMPOLINE.what,
            "SKSE::Trampoline patches machine code and has no target in this engine") == 0 ? 0 : 7;
}
"#,
    )
    .expect("write live C++ fixture");
    let executable = scratch.join("live_events");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("../commonlib-shim/include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by the SHM-22 done check");
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
fn shm_22_world_events_reach_the_cpp_mit_event_sinks() {
    let mut game = fixture();
    let (mut cpp, input) = cpp_event_consumer();
    {
        let mut world = game.world_mut();
        let consumer = CppConsumer::<TesGrabReleaseEvent>(input.clone(), std::marker::PhantomData);
        world
            .commonlib_event_sources()
            .grab_release()
            .add_sink(Box::new(consumer));
        let consumer = CppConsumer::<TesHarvestedEvent>(input.clone(), std::marker::PhantomData);
        world
            .commonlib_event_sources()
            .harvested()
            .add_sink(Box::new(consumer));
        let consumer = CppConsumer::<TesHitEvent>(input.clone(), std::marker::PhantomData);
        world
            .commonlib_event_sources()
            .hit()
            .add_sink(Box::new(consumer));
        let consumer = CppConsumer::<TesObjectLoadedEvent>(input.clone(), std::marker::PhantomData);
        world
            .commonlib_event_sources()
            .object_loaded()
            .add_sink(Box::new(consumer));
        let consumer = CppConsumer::<TesQuestStageEvent>(input.clone(), std::marker::PhantomData);
        world
            .commonlib_event_sources()
            .quest_stage()
            .add_sink(Box::new(consumer));
    }
    game.world_mut().set_grabbed_reference(Some(id(REFERENCE)));
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    assert!(game
        .world_mut()
        .harvest_item(id(0x900), id(0xA10), 1, &mut allocate));
    game.melee_hit(MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    })
    .unwrap();
    game.world_mut()
        .attach_streaming_cells(&[id(CELL)])
        .unwrap();
    assert!(game.set_stage(id(0x820), 1));
    drop(input);
    drop(game);
    assert!(cpp
        .wait()
        .expect("wait for C++ live event consumer")
        .success());
}

#[test]
fn shm_22_live_grab_producer_tracks_reference_release_and_grab() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesGrabReleaseEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .grab_release()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut().set_grabbed_reference(Some(id(REFERENCE)));
    game.world_mut().set_grabbed_reference(None);
    assert_eq!(
        *seen.borrow(),
        [
            TesGrabReleaseEvent {
                reference: soe_commonlib_shim::refr::ObjectRefHandle(0x811),
                grabbed: true,
            },
            TesGrabReleaseEvent {
                reference: soe_commonlib_shim::refr::ObjectRefHandle(0x811),
                grabbed: false,
            }
        ]
    );
}

#[test]
fn shm_22_live_harvest_producer_follows_successful_inventory_addition() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesHarvestedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .harvested()
        .add_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    assert!(game
        .world_mut()
        .harvest_item(id(0x900), id(0xA10), 2, &mut allocate));
    assert_eq!(game.world_mut().item_count(id(0x900), id(0xA10)), 2);
    assert_eq!(
        *seen.borrow(),
        [TesHarvestedEvent {
            produce_item: soe_commonlib_shim::form::FormId(0xA10),
            harvester: soe_commonlib_shim::refr::ActorHandle(0x900),
        }]
    );
}

#[test]
fn shm_22_live_hit_producer_reports_committed_melee_contact() {
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
            is_power_attack: false,
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
    assert_eq!(
        *seen.borrow(),
        [TesHitEvent {
            target: soe_commonlib_shim::refr::ObjectRefHandle(0x901),
            cause: soe_commonlib_shim::refr::ObjectRefHandle(0x900),
            source: soe_commonlib_shim::form::FormId(0xA00),
            projectile: soe_commonlib_shim::form::FormId(0),
            flags: soe_commonlib_shim::events::HitFlags::default(),
        }]
    );
}

#[test]
fn shm_22_live_object_loaded_producer_tracks_stream_residency() {
    let game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesObjectLoadedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .object_loaded()
        .add_sink(Box::new(Capture(seen.clone())));
    game.world_mut()
        .attach_streaming_cells(&[id(CELL)])
        .unwrap();
    game.world_mut()
        .detach_streaming_cells(&[id(CELL)])
        .unwrap();
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
fn shm_22_live_quest_stage_producer_only_sends_defined_stages() {
    let mut game = fixture();
    let seen = Rc::new(RefCell::new(Vec::<TesQuestStageEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .quest_stage()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(!game.set_stage(id(0x820), 99));
    assert!(game.set_stage(id(0x820), 1));
    assert_eq!(
        *seen.borrow(),
        [TesQuestStageEvent {
            quest: soe_commonlib_shim::form::FormId(0x820),
            stage: 1,
            item_index: 0,
        }]
    );
}
