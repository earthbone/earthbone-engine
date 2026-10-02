// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Removing the loading screen without changing what a script sees.
//!
//! The claim this file exists to defend is narrow and testable: prefetching a
//! destination cell is invisible. A move that streamed early and a move that
//! did not must queue the same events, to the same objects, in the same
//! order. The only permitted difference is that the prefetched one has
//! nothing left to build, which is the whole point.
//!
//! The fixture is the one `transitions.rs` builds, reused rather than copied.
//! It carries no `VMAD`, so nothing in it is scripted, and the event filter
//! keys on whether a form already has a VM object. Calling `object_of_form`
//! gives a reference an object without inventing a script, which is exactly
//! the condition the filter tests.

#[path = "transitions.rs"]
mod transitions;

use soe_game::world::Threshold;
use soe_game::{Game, Pending};
use soe_papyrus::ObjectId;
use transitions::{DOOR_OUT, START_CELL, id};

/// Cell B, the destination. `transitions.rs` exports the door ids but not the
/// far cell, and the far cell is what a prefetch takes.
const CELL_B: u32 = 0x811;
const ROCK_B: u32 = 0x840;
const DOOR_B: u32 = 0x821;

fn events(g: &Game) -> Vec<(ObjectId, String)> {
    g.world()
        .pending
        .iter()
        .filter_map(|p| match p {
            Pending::Event { object, name, .. } => Some((*object, name.clone())),
            Pending::Call { .. }
            | Pending::CallScript { .. }
            | Pending::BindEffect { .. }
            | Pending::BindReference { .. }
            | Pending::ReferenceEvent { .. }
            | Pending::DeferredDeath { .. }
            | Pending::ActorDeath { .. } => None,
        })
        .collect()
}

fn names(g: &Game) -> Vec<String> {
    events(g).into_iter().map(|(_, n)| n).collect()
}

#[test]
fn exterior_focus_changes_retain_neighbors_until_their_resource_unload() {
    let mut g = staged();
    let old = g.object_of_form(id(DOOR_OUT));
    let new = g.object_of_form(id(ROCK_B));
    g.world_mut().pending.clear();
    g.world_mut()
        .update_streaming_cells(id(START_CELL), &[id(CELL_B)], &[])
        .unwrap();
    assert!(events(&g).contains(&(new, "OnCellAttach".into())));
    assert!(!events(&g).iter().any(|(object, _)| *object == old));
    g.world_mut().pending.clear();

    g.world_mut()
        .update_streaming_cells(id(CELL_B), &[], &[])
        .unwrap();
    assert!(events(&g).is_empty(), "retained resources must not reload");
    assert!(g.world().cell_is_resident(id(START_CELL)));
    assert_eq!(g.world().pending_unload(), None);
    assert_eq!(g.world_mut().settle_unload(), (0, 0));
    let player = g.player();
    assert_eq!(
        g.world().reference_ref(player).unwrap().cell,
        Some(id(CELL_B))
    );

    g.world_mut()
        .update_streaming_cells(id(CELL_B), &[], &[id(START_CELL)])
        .unwrap();
    assert_eq!(
        events(&g),
        vec![(old, "OnCellDetach".into()), (old, "OnUnload".into())]
    );
    assert!(!g.world().cell_is_resident(id(START_CELL)));
    assert!(g.world().cell_is_resident(id(CELL_B)));
}

#[test]
fn invalid_streaming_delta_is_atomic() {
    let g = staged();
    let before = g.world().loaded_reference_count();
    assert!(
        g.world_mut()
            .update_streaming_cells(id(CELL_B), &[id(CELL_B), id(0xDEAD)], &[id(START_CELL)])
            .is_err()
    );
    assert_eq!(g.world().loaded_reference_count(), before);
    assert_eq!(g.world().loaded_cell(), Some(id(START_CELL)));
    assert!(events(&g).is_empty());
    assert!(g.world().cell_is_resident(id(START_CELL)));
}

#[test]
fn neighboring_cells_keep_the_focus_and_validate_before_detaching() {
    let g = staged();
    g.world_mut().attach_streaming_cells(&[id(CELL_B)]).unwrap();
    assert_eq!(g.world().loaded_cell(), Some(id(START_CELL)));
    assert!(g.world().cell_is_resident(id(CELL_B)));
    g.world_mut().pending.clear();
    assert!(
        g.world_mut()
            .detach_streaming_cells(&[id(CELL_B), id(START_CELL)])
            .is_err()
    );
    assert!(g.world().cell_is_resident(id(CELL_B)));
    assert!(events(&g).is_empty());
    g.world_mut().detach_streaming_cells(&[id(CELL_B)]).unwrap();
    assert!(!g.world().cell_is_resident(id(CELL_B)));
    assert_eq!(g.world().loaded_cell(), Some(id(START_CELL)));
}

#[test]
fn a_door_departure_can_become_a_neighbor_without_a_later_unload() {
    let g = staged();
    g.world_mut().attach_cell(Some(id(CELL_B)), true);
    assert_eq!(g.world().pending_unload(), Some(id(START_CELL)));
    g.world_mut()
        .attach_streaming_cells(&[id(START_CELL)])
        .unwrap();
    assert_eq!(g.world().pending_unload(), None);
    assert_eq!(g.world_mut().settle_unload(), (0, 0));
    assert!(g.world().cell_is_resident(id(START_CELL)));
    assert!(g.world().cell_is_resident(id(CELL_B)));
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_B)));
}

#[test]
fn blocked_activation_reports_failure_but_default_only_bypasses_it() {
    let mut g = staged();
    let player = g.player();
    g.world_mut()
        .reference(id(DOOR_OUT))
        .unwrap()
        .activation_blocked = true;
    assert!(!g.activate_no_settle(id(DOOR_OUT), player));
    assert_eq!(g.world().loaded_cell(), Some(id(START_CELL)));
    assert_eq!(names(&g), vec!["OnActivate"]);
    g.world_mut().pending.clear();
    let shared = g.shared_world();
    assert!(
        shared
            .borrow_mut()
            .activate(id(DOOR_OUT), player, false, &mut |name, class| g
                .vm
                .create_object_of(name, class))
    );
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_B)));
    assert!(!names(&g).iter().any(|name| name == "OnActivate"));
}

#[test]
fn deferred_activation_matches_events_and_retains_source_until_settled() {
    let mut immediate = staged();
    let mut deferred = staged();
    let player = immediate.player();
    let player_deferred = deferred.player();
    assert!(immediate.activate_no_settle(id(DOOR_OUT), player));
    let shared = deferred.shared_world();
    assert!(shared.borrow_mut().activate_with_unload(
        id(DOOR_OUT),
        player_deferred,
        true,
        true,
        &mut |name, class| deferred.vm.create_object_of(name, class)
    ));
    assert_eq!(events(&immediate), events(&deferred));
    assert_eq!(
        names(&deferred).last().map(String::as_str),
        Some("OnActivate")
    );
    assert_eq!(deferred.world().pending_unload(), Some(id(START_CELL)));
    assert_eq!(
        deferred.world().reference_ref(player_deferred),
        immediate.world().reference_ref(player)
    );
    assert!(deferred.world().is_reference_loaded(id(0x830)));
    assert!(!immediate.world().is_reference_loaded(id(0x830)));
    deferred.world_mut().settle_unload();
    assert_eq!(deferred.world().pending_unload(), None);
    assert!(!deferred.world().is_reference_loaded(id(0x830)));
}

/// A game sitting in cell A, with the destination's references given VM
/// objects so the cell events have somewhere to go. Allocation order is fixed
/// so two games built this way agree on object ids.
fn staged() -> Game {
    let mut g = transitions::game();
    g.world_mut().load_cell(id(START_CELL));
    g.object_of_form(id(ROCK_B));
    g.object_of_form(id(DOOR_B));
    g.world_mut().pending.clear();
    g
}

#[test]
fn a_prefetch_builds_the_cell_and_queues_nothing() {
    let g = staged();
    assert!(
        !g.world_mut().cell_is_resident(id(CELL_B)),
        "the destination starts cold"
    );

    let built = g.world_mut().prefetch_cell(id(CELL_B));

    assert!(built > 0, "prefetch built {built} references");
    assert!(
        g.world_mut().cell_is_resident(id(CELL_B)),
        "the destination is resident after a prefetch"
    );
    assert!(
        g.world().reference_ref(id(ROCK_B)).is_some(),
        "a reference in the destination was built"
    );
    // The two things a script could otherwise notice.
    assert_eq!(
        names(&g),
        Vec::<String>::new(),
        "a prefetch queued an event"
    );
    assert_eq!(
        g.world().loaded_cell(),
        Some(id(START_CELL)),
        "a prefetch moved the attached cell"
    );
}

#[test]
fn after_a_prefetch_the_move_has_nothing_left_to_build() {
    let cold = staged();
    let built_cold = cold.world_mut().change_cell(Some(id(CELL_B))).loaded;

    let warm = staged();
    let prefetched = warm.world_mut().prefetch_cell(id(CELL_B));
    let built_warm = warm
        .world_mut()
        .change_cell_with(Some(id(CELL_B)), true)
        .loaded;

    assert!(built_cold > 0, "the cold move built {built_cold}");
    assert_eq!(prefetched, built_cold, "the prefetch did the same work");
    assert_eq!(
        built_warm, 0,
        "the prefetched move still built {built_warm} references"
    );
}

#[test]
fn prefetching_does_not_change_one_event() {
    let cold = staged();
    cold.world_mut().change_cell(Some(id(CELL_B)));
    let from_cold = events(&cold);

    let warm = staged();
    // Capture coldness before the prefetch, which is the whole discipline.
    let was_cold = !warm.world_mut().cell_is_resident(id(CELL_B));
    warm.world_mut().prefetch_cell(id(CELL_B));
    warm.world_mut()
        .change_cell_with(Some(id(CELL_B)), was_cold);
    let from_warm = events(&warm);

    assert!(
        !from_cold.is_empty(),
        "the cold move queued nothing to compare"
    );
    assert_eq!(
        from_cold, from_warm,
        "prefetching changed the events a script receives"
    );
}

#[test]
fn reading_residency_after_the_prefetch_is_the_trap_that_loses_on_cell_load() {
    let g = staged();
    g.world_mut().prefetch_cell(id(CELL_B));

    // What a careless implementation would do: ask now.
    let asked_late = !g.world_mut().cell_is_resident(id(CELL_B));
    assert!(
        !asked_late,
        "residency after a prefetch always says resident"
    );

    g.world_mut().change_cell_with(Some(id(CELL_B)), asked_late);
    assert!(
        !names(&g).contains(&"OnCellLoad".to_string()),
        "asking late is supposed to lose the event, which is why it is banned"
    );
}

#[test]
fn on_cell_load_follows_whether_the_cell_is_still_streamed() {
    let g = staged();
    g.world_mut().change_cell(Some(id(CELL_B)));
    assert!(
        names(&g).contains(&"OnCellLoad".to_string()),
        "a cold cell owes OnCellLoad"
    );

    // A cell that is still streamed owes nothing. Prefetching is what puts it
    // there, and `change_cell` reading residency itself is what makes this the
    // suppressed case.
    g.world_mut().change_cell(Some(id(START_CELL)));
    g.world_mut().prefetch_cell(id(CELL_B));
    g.world_mut().pending.clear();
    g.world_mut().change_cell(Some(id(CELL_B)));
    assert!(
        !names(&g).contains(&"OnCellLoad".to_string()),
        "a cell still streamed owed no OnCellLoad"
    );

    // Leaving drops the 3D, so the next entry owes it again. Our engine
    // really does unload, where vanilla keeps a cache, and the wiki calls the
    // event unpredictable for that reason.
    g.world_mut().change_cell(Some(id(START_CELL)));
    g.world_mut().pending.clear();
    g.world_mut().change_cell(Some(id(CELL_B)));
    assert!(
        names(&g).contains(&"OnCellLoad".to_string()),
        "a cell that unloaded owes OnCellLoad on the way back"
    );
}

#[test]
fn attach_reaches_a_disabled_reference_and_load_does_not() {
    let mut g = staged();
    g.world_mut().prefetch_cell(id(CELL_B));
    if let Some(r) = g.world_mut().reference(id(ROCK_B)) {
        r.enabled = false;
    }
    let rock = g.object_of_form(id(ROCK_B));
    g.world_mut().pending.clear();

    g.world_mut().change_cell_with(Some(id(CELL_B)), true);

    let got = events(&g);
    assert!(
        got.contains(&(rock, "OnCellAttach".into())),
        "the wiki says attach fires for disabled references"
    );
    assert!(
        !got.contains(&(rock, "OnLoad".into())),
        "the wiki says OnLoad does not fire for disabled references"
    );
}

#[test]
fn the_order_is_detach_then_unload_then_attach_then_load_then_cell_load() {
    let mut g = staged();
    // Give the cell being left an object too, so both halves are visible.
    g.object_of_form(id(DOOR_OUT));
    g.world_mut().pending.clear();

    g.world_mut().change_cell(Some(id(CELL_B)));

    let seen = names(&g);
    let first = |n: &str| seen.iter().position(|s| s == n);
    let detach = first("OnCellDetach").expect("no OnCellDetach");
    let attach = first("OnCellAttach").expect("no OnCellAttach");
    let load = first("OnLoad").expect("no OnLoad");
    let cell_load = first("OnCellLoad").expect("no OnCellLoad");
    assert!(
        detach < attach,
        "the old cell detached after the new attached"
    );
    assert!(attach < load, "OnCellAttach must precede OnLoad");
    assert!(
        attach < cell_load,
        "the wiki states OnCellAttach reliably precedes OnCellLoad"
    );
    assert!(
        load < cell_load,
        "every 3D loads before the cell reports ready"
    );
}

#[test]
fn arming_a_door_streams_the_far_side_and_tells_nobody() {
    let g = staged();
    let before = g.world_mut().player();
    let start = g.world().reference_ref(before).map(|r| r.position);

    let way = g
        .world_mut()
        .arm_doorway(id(DOOR_OUT))
        .expect("a load door");

    assert_eq!(
        way.to,
        Some(id(CELL_B)),
        "the far cell was read from the far door"
    );
    assert!(way.cold, "the destination was cold at activation");
    assert!(
        way.prefetched > 0,
        "the arm built {} references",
        way.prefetched
    );
    assert_eq!(names(&g), Vec::<String>::new(), "arming queued an event");
    assert_eq!(
        g.world().loaded_cell(),
        Some(id(START_CELL)),
        "arming attached the destination early"
    );
    assert_eq!(
        g.world().reference_ref(before).map(|r| r.position),
        start,
        "arming moved the player"
    );
}

#[test]
fn committing_a_door_moves_the_player_and_fires_the_sequence() {
    let g = staged();
    let way = g
        .world_mut()
        .arm_doorway(id(DOOR_OUT))
        .expect("a load door");

    let moved = g.world_mut().commit_doorway(&way);

    assert_eq!(moved.to, Some(id(CELL_B)));
    assert_eq!(moved.loaded, 0, "the commit had work left to do");
    let player = g.world_mut().player();
    assert_eq!(
        g.world().reference_ref(player).map(|r| r.cell),
        Some(Some(id(CELL_B))),
        "the player is in the destination"
    );
    let seen = names(&g);
    assert!(seen.contains(&"OnCellAttach".to_string()));
    assert!(
        seen.contains(&"OnCellLoad".to_string()),
        "a cold destination still owes OnCellLoad after a prefetch"
    );
}

#[test]
fn a_plain_door_is_not_a_doorway() {
    let g = staged();
    // The static in the destination has no teleport data.
    assert!(g.world_mut().arm_doorway(id(ROCK_B)).is_none());
}

#[test]
fn the_crossing_test_is_the_thing_that_decides_when_events_fire() {
    let g = staged();
    let way = g
        .world_mut()
        .arm_doorway(id(DOOR_OUT))
        .expect("a load door");
    let mut plane: Threshold = way.threshold;

    // Standing short of the plane commits nothing.
    let landing = way.landing.position;
    let near = [landing[0], landing[1] + 100.0, landing[2]];
    assert!(plane.update(near).is_none() || plane.update(near).is_none());

    // Through it, once.
    let far = [landing[0], landing[1] - 100.0, landing[2]];
    let crossed = plane.update(far);
    assert!(crossed.is_some(), "walking through did not register");
    assert!(
        plane.update(far).is_none(),
        "the same position crossed twice"
    );
}

#[test]
fn hst_07_approaching_a_load_door_selects_its_link_without_unrelated_cell_coordinates() {
    let g = staged();
    let before = g.world().loaded_reference_count();
    let selected = g
        .world()
        .nearby_interior_destination(id(START_CELL), [0.0; 3], 10.0)
        .expect("near source door");
    assert_eq!(selected.door, id(DOOR_OUT));
    assert_eq!(selected.cell, id(CELL_B));
    assert_eq!(g.world().loaded_reference_count(), before);
    assert!(!g.world().cell_is_resident(id(CELL_B)));
    assert!(events(&g).is_empty());

    // The unrelated interior's door overlaps the query while the local door is distant.
    g.world_mut().reference(id(DOOR_B)).unwrap().position = [2000.0; 3];
    assert!(
        g.world()
            .nearby_interior_destination(id(START_CELL), [2000.0; 3], 10.0)
            .is_none()
    );
}

#[test]
fn nearby_prefetch_respects_live_door_state_and_rejects_invalid_proximity() {
    let g = staged();
    for invalid in [f32::NAN, f32::INFINITY, -1.0] {
        assert!(
            g.world()
                .nearby_interior_destination(id(START_CELL), [0.0; 3], invalid)
                .is_none()
        );
    }
    assert!(
        g.world()
            .nearby_interior_destination(id(START_CELL), [f32::NAN; 3], 500.0)
            .is_none()
    );
    g.world_mut().reference(id(DOOR_OUT)).unwrap().enabled = false;
    assert!(
        g.world()
            .nearby_interior_destination(id(START_CELL), [0.0; 3], 500.0)
            .is_none()
    );
    g.world_mut().reference(id(DOOR_OUT)).unwrap().enabled = true;
    g.world_mut().reference(id(DOOR_OUT)).unwrap().deleted = true;
    assert!(
        g.world()
            .nearby_interior_destination(id(START_CELL), [0.0; 3], 500.0)
            .is_none()
    );
}
