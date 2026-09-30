// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{graph, Root};
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

#[test]
fn anm_27_reactivation_allows_one_event_on_each_later_deactivation() {
    let bytes = graph(&Root::Deactivate, "idle", "walk");
    let graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    for _ in 0..2 {
        assert!(vm.variables.set_word(2, 0));
        vm.update(0.0);
        assert_eq!(
            vm.take_events()
                .iter()
                .filter(|event| event.id == 2)
                .count(),
            1
        );
        vm.update(0.0);
        assert!(vm.take_events().is_empty());
        assert!(vm.variables.set_word(2, 1));
        vm.update(0.0);
        assert!(vm.take_events().is_empty());
    }
}
