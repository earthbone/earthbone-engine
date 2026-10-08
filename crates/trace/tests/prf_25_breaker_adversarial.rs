// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{
    measure_physics_query, physics_query_stats, reset_physics_queries, PhysicsQueryKind, System,
    SystemScope,
};

#[test]
fn prf_25_queries_remain_charged_to_the_calling_system() {
    reset_physics_queries();
    {
        let _scope = SystemScope::new(System::AiMovement);
        for kind in [
            PhysicsQueryKind::Raycast,
            PhysicsQueryKind::ShapeCast,
            PhysicsQueryKind::Overlap,
        ] {
            measure_physics_query(kind, std::thread::yield_now);
        }
    }
    let stats = physics_query_stats();
    for kind in [
        PhysicsQueryKind::Raycast,
        PhysicsQueryKind::ShapeCast,
        PhysicsQueryKind::Overlap,
    ] {
        assert_eq!(stats.count(kind, System::AiMovement), 1, "{kind:?}");
    }
}
