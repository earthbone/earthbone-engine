// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn timer_graph(alarm_time: f32, bind_enable: bool) -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let wrapper = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, wrapper);
    let list = b.object("hkbModifierList", 96);
    b.ptr(wrapper, 72, list);
    b.set_bool(list, 72, true);
    let timer = b.object("hkbTimerModifier", 112);
    b.string(timer, 56, "elapsed timer");
    b.array_ptrs(list, 80, &[timer, timer]);
    b.set_bool(timer, 72, true);
    b.set_f32(timer, 80, alarm_time);
    b.set_i32(timer, 88, 7);
    // secondsElapsed is SERIALIZE_IGNORED and must not seed runtime progress.
    b.set_f32(timer, 104, 99.0);

    if bind_enable {
        let data = b.object("hkbBehaviorGraphData", 128);
        let strings = b.object("hkbBehaviorGraphStringData", 80);
        let values = b.object("hkbVariableValueSet", 64);
        b.ptr(graph, 136, data);
        b.ptr(data, 120, strings);
        b.ptr(data, 112, values);
        b.array_strings(strings, 48, &["timerEnabled"]);
        let infos = b.array_structs(data, 32, 6, 1);
        b.set_i8(infos, 4, 0);
        b.array_i32(values, 16, &[1]);
        let bindings = b.object("hkbVariableBindingSet", 40);
        b.ptr(timer, 16, bindings);
        let binding = b.array_structs(bindings, 16, 40, 1);
        b.string(binding, 0, "enable");
        b.set_i32(binding, 28, 0);
        b.set_i32(bindings, 32, -1);
    }

    BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap()
}

fn events(vm: &mut Interpreter<'_>) -> Vec<i32> {
    vm.take_events().into_iter().map(|event| event.id).collect()
}

#[test]
fn anm_16_timer_modifier_decodes_authored_alarm_fields() {
    let graph = timer_graph(0.5, false);
    let timer = graph.find_by_name("elapsed timer").unwrap();
    assert!(matches!(
        timer.kind,
        NodeKind::TimerModifier {
            enable: true,
            alarm_time_seconds,
            alarm_event,
        } if alarm_time_seconds == 0.5 && alarm_event.id == 7
    ));
}

#[test]
fn anm_16_timer_modifier_emits_once_when_authored_elapsed_time_is_reached() {
    let graph = timer_graph(0.5, false);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.update(0.25);
    assert!(events(&mut vm).is_empty());
    vm.update(0.25);
    assert_eq!(events(&mut vm), [7]);
    vm.update(1.0);
    assert!(events(&mut vm).is_empty());
}

#[test]
fn anm_16_timer_modifier_counts_each_update_once_when_list_repeats_node() {
    let graph = timer_graph(0.5, false);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.update(0.25);
    assert!(events(&mut vm).is_empty());
}

#[test]
fn anm_16_timer_modifier_resets_elapsed_time_and_alarm_on_activation() {
    let graph = timer_graph(0.5, true);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.update(0.25);
    assert!(events(&mut vm).is_empty());
    vm.variables.set_bool(0, false);
    vm.update(2.0);
    assert!(events(&mut vm).is_empty());
    vm.variables.set_bool(0, true);
    vm.update(0.25);
    assert!(events(&mut vm).is_empty());
    vm.update(0.25);
    assert_eq!(events(&mut vm), [7]);

    vm.variables.set_bool(0, false);
    vm.update(0.0);
    vm.variables.set_bool(0, true);
    vm.update(0.5);
    assert_eq!(events(&mut vm), [7]);
}
