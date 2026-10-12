// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod play_path {
    include!("tst_19_play_path.rs");

    mod adversarial {
        use super::{validate, CHECKS, EXTERIOR_CHECKS, STOP_CHECKS};
        use serde_json::{json, Value};
        use std::fmt::Write as _;

        const STOPS: &[&str] = &[
            "hulda",
            "container",
            "loose",
            "hearth",
            "exit",
            "market",
            "gate",
            "return",
        ];

        fn report(rows: &[Value]) -> String {
            let mut output = String::new();
            for row in rows {
                writeln!(output, "PLAY_PATH {row}").unwrap();
            }
            output.push_str(&format!(
                "PLAY_PATH_RESULT {}",
                json!({"elapsed_s": 180.0, "loops": 1, "required_failures": 0})
            ));
            output
        }

        fn complete_rows() -> Vec<Value> {
            let route_stops = [
                ("route.hulda", "hulda"),
                ("route.container", "container"),
                ("route.loose", "loose"),
                ("route.hearth", "hearth"),
                ("route.exit", "exit"),
                ("route.market", "market"),
                ("route.gate", "gate"),
                ("route.return", "return"),
                ("route.repeat", "return"),
                ("route.walk", "market"),
            ];
            let mut rows: Vec<_> = CHECKS
                .iter()
                .enumerate()
                .map(|(index, check)| {
                    let stop = route_stops
                        .iter()
                        .find(|(route_check, _)| route_check == check)
                        .map_or(STOPS[index % STOPS.len()], |(_, stop)| *stop);
                    let value = if *check == "route.walk" {
                        json!({"distance": 25.0, "path_points": 3, "seconds": 1.0})
                    } else if check.starts_with("menu.") {
                        json!({"applicable": true, "reference": "reference.png", "reference_similarity": 0.95})
                    } else {
                        json!({"sample": 1})
                    };
                    json!({"check": check, "stop": stop, "status": "PASS", "pass": true,
                        "owner": "TST-19", "value": value})
                })
                .collect();
            for stop in STOPS {
                for check in
                    STOP_CHECKS
                        .iter()
                        .chain(if matches!(*stop, "market" | "gate" | "return") {
                            EXTERIOR_CHECKS
                        } else {
                            &[]
                        })
                {
                    rows.push(
                        json!({"check": check, "stop": stop, "status": "PASS", "pass": true,
                        "owner": "TST-19", "value": {"sample": 1}}),
                    );
                }
            }
            rows
        }

        #[test]
        fn tst_19_rejects_route_checks_reported_out_of_walk_order() {
            let mut rows = complete_rows();
            let gate_index = rows
                .iter()
                .position(|row| row["check"] == "route.gate")
                .unwrap();
            let gate = rows.remove(gate_index);
            let hulda_index = rows
                .iter()
                .position(|row| row["check"] == "route.hulda")
                .unwrap();
            rows.insert(hulda_index, gate);
            let result = validate(&report(&rows), &json!({}), 180.0);
            assert!(
                result.is_err(),
                "route.gate passed before the exterior route"
            );
        }

        #[test]
        fn tst_19_rejects_route_walk_success_without_displacement() {
            let mut rows = complete_rows();
            let walk = rows
                .iter_mut()
                .find(|row| row["check"] == "route.walk")
                .unwrap();
            walk["value"] = json!({"distance": 0.0, "path_points": 1, "seconds": 0.0});
            let result = validate(&report(&rows), &json!({}), 180.0);
            assert!(
                result.is_err(),
                "zero-distance navigation was accepted as a route"
            );
        }

        #[test]
        fn tst_19_rejects_menu_theme_pass_based_only_on_loaded_sources() {
            let mut rows = complete_rows();
            let theme = rows
                .iter_mut()
                .find(|row| row["check"] == "menu.inventory_theme")
                .unwrap();
            theme["value"] = json!({"applicable": true, "theme": "skyui",
                "sources": ["interface/skyui/inventorylists.swf"]});
            let result = validate(&report(&rows), &json!({}), 180.0);
            assert!(
                result.is_err(),
                "theme source presence was treated as visual match"
            );
        }
    }
}
