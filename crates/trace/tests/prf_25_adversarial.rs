use soe_trace::{
    measure_physics_query, physics_query_stats, reset_physics_queries, PhysicsQueryKind, System,
    SystemScope,
};

#[test]
fn prf_25_each_wrapped_kind_counts_once_under_its_requesting_system() {
    reset_physics_queries();
    {
        let _scope = SystemScope::new(System::Combat);
        for kind in [
            PhysicsQueryKind::Raycast,
            PhysicsQueryKind::ShapeCast,
            PhysicsQueryKind::Overlap,
        ] {
            assert_eq!(measure_physics_query(kind, || 17), 17);
        }
    }

    let stats = physics_query_stats();
    for kind in [
        PhysicsQueryKind::Raycast,
        PhysicsQueryKind::ShapeCast,
        PhysicsQueryKind::Overlap,
    ] {
        assert_eq!(stats.count(kind, System::Combat), 1);
        assert!(stats.elapsed_ns(kind, System::Combat) > 0);
        assert_eq!(stats.count(kind, System::Other), 0);
    }
}

#[test]
fn prf_25_unscoped_queries_are_charged_to_other() {
    reset_physics_queries();
    measure_physics_query(PhysicsQueryKind::Raycast, || ());

    let stats = physics_query_stats();
    assert_eq!(stats.count(PhysicsQueryKind::Raycast, System::Other), 1);
    assert_eq!(
        stats.counts[PhysicsQueryKind::Raycast as usize]
            .iter()
            .sum::<u64>(),
        1
    );
}

#[test]
fn prf_25_reset_clears_counts_and_elapsed_time_for_all_kinds() {
    reset_physics_queries();
    for kind in [
        PhysicsQueryKind::Raycast,
        PhysicsQueryKind::ShapeCast,
        PhysicsQueryKind::Overlap,
    ] {
        measure_physics_query(kind, || ());
    }
    reset_physics_queries();

    let stats = physics_query_stats();
    for kind in [
        PhysicsQueryKind::Raycast,
        PhysicsQueryKind::ShapeCast,
        PhysicsQueryKind::Overlap,
    ] {
        for system in System::ALL {
            assert_eq!(stats.count(kind, system), 0);
            assert_eq!(stats.elapsed_ns(kind, system), 0);
        }
    }
}

#[test]
fn prf_25_panicking_query_does_not_record_an_uncompleted_call() {
    reset_physics_queries();
    let result = std::panic::catch_unwind(|| {
        measure_physics_query(PhysicsQueryKind::Overlap, || panic!("query failed"));
    });
    assert!(result.is_err());
    let stats = physics_query_stats();
    for system in System::ALL {
        assert_eq!(stats.count(PhysicsQueryKind::Overlap, system), 0);
        assert_eq!(stats.elapsed_ns(PhysicsQueryKind::Overlap, system), 0);
    }
}
