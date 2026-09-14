// SPDX-License-Identifier: MIT

//! The shim scope as data: every header group from `docs/shim.md` with its
//! type count and what the engine offers for it. A test compares this
//! table with the document so the two cannot drift apart.

/// What the engine has for a group of CommonLib types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Support {
    /// An engine crate exists and the shim binds to it now.
    Bound,
    /// The concept is in the plan and the binding waits on that crate.
    Planned,
    /// The engine has the concept in another shape, the shim adapts.
    Different,
    /// Provided by the shim itself from Rust std.
    ShimProvided,
    /// No target exists and none is planned in the first two rounds.
    Missing,
    /// Cannot be supported: it patches the Skyrim binary.
    Never,
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub name: &'static str,
    pub types: usize,
    pub support: Support,
}

/// 349 names across four plugins, surveyed 2026-09-06.
pub const GROUPS: &[Group] = &[
    Group {
        name: "Forms and record enums",
        types: 86,
        support: Support::Bound,
    },
    Group {
        name: "References, actors, inventory and world singletons",
        types: 70,
        support: Support::Planned,
    },
    Group {
        name: "Papyrus VM",
        types: 11,
        support: Support::Bound,
    },
    Group {
        name: "Conditions",
        types: 4,
        support: Support::Planned,
    },
    Group {
        name: "Event sources, sinks and story events",
        types: 26,
        support: Support::Planned,
    },
    Group {
        name: "Containers, strings, locks and memory",
        types: 23,
        support: Support::ShimProvided,
    },
    Group {
        name: "NetImmerse scene graph and materials",
        types: 35,
        support: Support::Different,
    },
    Group {
        name: "Havok animation and physics",
        types: 29,
        support: Support::Missing,
    },
    Group {
        name: "Camera, input and player controls",
        types: 27,
        support: Support::Different,
    },
    Group {
        name: "Scaleform UI and menus",
        types: 19,
        support: Support::Missing,
    },
    Group {
        name: "Vtable hooks",
        types: 19,
        support: Support::Never,
    },
];

pub fn total_types() -> usize {
    GROUPS.iter().map(|g| g.types).sum()
}

/// Types the shim can serve today or after round 2, as a share of all.
pub fn reachable_share() -> f64 {
    let reachable: usize = GROUPS
        .iter()
        .filter(|g| {
            matches!(
                g.support,
                Support::Bound | Support::Planned | Support::ShimProvided | Support::Different
            )
        })
        .map(|g| g.types)
        .sum();
    reachable as f64 / total_types() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_the_survey() {
        assert_eq!(total_types(), 349);
        assert!(reachable_share() > 0.8);
    }

    #[test]
    fn the_document_carries_the_same_counts() {
        let doc =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/shim.md"))
                .expect("docs/shim.md sits two levels up from the crate");
        for g in GROUPS {
            let row = doc
                .lines()
                .find(|l| l.starts_with(&format!("| {} |", g.name)))
                .unwrap_or_else(|| panic!("{} is missing from docs/shim.md", g.name));
            let count: usize = row.split('|').nth(2).unwrap().trim().parse().unwrap();
            assert_eq!(count, g.types, "{}", g.name);
        }
    }
}
