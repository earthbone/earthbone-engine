// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::rc::Rc;

use soe_commonlib_shim::events::{EventSink, NotifyControl, QuestStartStopEvent};
use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest::entry_flags, world::World};
use soe_papyrus::ObjectId;

const QUEST: u32 = 0x800;
const QUEST_ID: soe_esm::GlobalFormId = soe_esm::GlobalFormId {
    file: 0,
    object: QUEST,
};
const T: fn(&[u8; 4]) -> Tag = Tag::new;

struct Capture(Rc<RefCell<Vec<QuestStartStopEvent>>>);

impl EventSink<QuestStartStopEvent> for Capture {
    fn process(&mut self, event: &QuestStartStopEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

struct CppConsumer(Rc<RefCell<ChildStdin>>);

impl EventSink<QuestStartStopEvent> for CppConsumer {
    fn process(&mut self, event: &QuestStartStopEvent) -> NotifyControl {
        let mut input = self.0.borrow_mut();
        input
            .write_all(&event.form_id.0.to_le_bytes())
            .expect("send live quest form to C++ fixture");
        input
            .write_all(&[u8::from(event.started), u8::from(event.failed)])
            .expect("send live quest flags to C++ fixture");
        input
            .write_all(&[0, 0])
            .expect("send live quest padding to C++ fixture");
        NotifyControl::Continue
    }
}

fn cpp_consumer() -> (std::process::Child, Rc<RefCell<ChildStdin>>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-23-live-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("create C++ fixture directory");
    let source = scratch.join("live_quest_events.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cstdio>
#include <cstring>
#include <memory>

struct QuestSink final : RE::EventSink<RE::TESQuestStartStopEvent> {
    unsigned count{};
    bool valid{true};
    RE::NotifyControl process(const RE::TESQuestStartStopEvent& event) override {
        if (count == 0) {
            valid = event.formID == 0x800 && event.started && !event.failed && event.pad06 == 0;
        } else if (count == 1) {
            valid = valid && event.formID == 0x800 && !event.started && event.failed && event.pad06 == 0;
        } else {
            valid = false;
        }
        ++count;
        return RE::NotifyControl::Continue;
    }
};

int main() {
    RE::EventSource<RE::TESQuestStartStopEvent> source;
    auto sink = std::make_unique<QuestSink>();
    auto* observed = sink.get();
    source.add_sink(std::move(sink));
    RE::TESQuestStartStopEvent event{};
    while (std::fread(&event, sizeof(event), 1, stdin) == 1) {
        if (source.send(event) != 1) return 1;
    }
    if (std::ferror(stdin) || observed->count != 2 || !observed->valid) return 2;
    return std::strcmp(SKSE::TRAMPOLINE.what,
        "SKSE::Trampoline patches machine code and has no target in this engine") == 0 ? 0 : 3;
}
"#,
    )
    .expect("write C++ fixture");
    let executable = scratch.join("live_quest_events");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("../commonlib-shim/include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by the SHM-23 done check");
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
fn shm_23_live_quest_transitions_reach_the_mit_event_sink() {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), [0; 12])
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0])
            .sub(T(b"INDX"), [1, 0, 0, 0])
            .sub(T(b"QSDT"), [entry_flags::FAIL_QUEST]),
    );
    let mut order = LoadOrder::new();
    order.add("Quest.esm", plugin).unwrap();
    let mut world = World::new(order);
    let events = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(events.clone())));
    let (mut cpp, cpp_input) = cpp_consumer();
    world.add_quest_start_stop_sink(Box::new(CppConsumer(cpp_input.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(world.start_quest(QUEST_ID, &mut allocate));
    assert!(world.quest(QUEST_ID).unwrap().running);
    assert!(world.set_stage(QUEST_ID, 1, &mut allocate));
    let state = world.quest(QUEST_ID).unwrap();
    assert!(!state.running);
    assert!(state.failed);

    assert_eq!(
        *events.borrow(),
        vec![
            QuestStartStopEvent {
                form_id: soe_commonlib_shim::form::FormId(0x0000_0800),
                started: true,
                failed: false,
            },
            QuestStartStopEvent {
                form_id: soe_commonlib_shim::form::FormId(0x0000_0800),
                started: false,
                failed: true,
            },
        ]
    );
    drop(cpp_input);
    drop(world);
    assert!(cpp.wait().expect("wait for C++ event consumer").success());
}
