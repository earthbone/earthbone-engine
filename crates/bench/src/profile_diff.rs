// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-22 differences of PRF-13 Chrome counter summaries. Self time follows
//! the local Godot profiler manual. serde_json reads the existing envelope.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use soe_trace::Operation;

type Identity = (String, String);
type Metrics = BTreeMap<String, u64>;
type Summary = BTreeMap<Identity, Metrics>;

pub fn run(args: &[String]) -> Result<String, String> {
    let [before, after] = args else {
        return Err(
            "usage: soe-bench profile-diff <before-summary.json> <after-summary.json>".into(),
        );
    };
    diff(&read(Path::new(before))?, &read(Path::new(after))?)
}

fn read(path: &Path) -> Result<Summary, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    parse(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn metric_names() -> Vec<&'static str> {
    let mut names = vec!["self_ns", "total_ns", "calls", "wait_ns", "waits"];
    names.extend(Operation::ALL.map(Operation::name));
    names
}

fn parse(bytes: &[u8]) -> Result<Summary, String> {
    let document: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let events = document
        .get("traceEvents")
        .and_then(Value::as_array)
        .ok_or_else(|| "expected a PRF-13 traceEvents array".to_owned())?;
    let mut summary = Summary::new();
    let metrics = metric_names();
    for event in events {
        if event.get("ph").and_then(Value::as_str) != Some("C") {
            return Err("expected PRF-13 counter events, not scope traces".into());
        }
        let kind = event
            .get("cat")
            .and_then(Value::as_str)
            .ok_or("counter has no category")?;
        let name = event
            .get("name")
            .and_then(Value::as_str)
            .ok_or("counter has no name")?;
        let args = event
            .get("args")
            .and_then(Value::as_object)
            .ok_or("counter has no metrics")?;
        let mut values = Metrics::new();
        for metric in &metrics {
            if let Some(value) = args.get(*metric) {
                values.insert(
                    (*metric).to_owned(),
                    value
                        .as_u64()
                        .ok_or_else(|| format!("{kind} {name}: invalid {metric}"))?,
                );
            }
        }
        if !values.contains_key("self_ns") || !values.contains_key("calls") {
            return Err(format!("{kind} {name}: missing self_ns or calls"));
        }
        if !["actor", "script", "quest", "plugin", "system"].contains(&kind) {
            continue;
        }
        let entity = match args.get("entity") {
            Some(value) => value.as_str().ok_or("invalid entity metadata")?,
            None if kind == "plugin" => name,
            None => name.rsplit_once('.').map_or(name, |(id, _)| id),
        };
        let id = match kind {
            "script" => entity
                .rsplit_once('.')
                .map_or(entity, |(script, _)| script)
                .to_owned(),
            "system" => format!(
                "{}.{}",
                args.get("phase").and_then(Value::as_str).unwrap_or("game"),
                entity
            ),
            _ => entity.to_owned(),
        };
        let row = summary.entry((kind.to_owned(), id)).or_default();
        for (metric, value) in values {
            let count = row.entry(metric).or_default();
            *count = count.checked_add(value).ok_or("counter total overflow")?;
        }
    }
    Ok(summary)
}

fn percent(before: u64, after: u64) -> String {
    if before == 0 {
        if after == 0 {
            "0.00%".into()
        } else {
            "new (baseline 0)".into()
        }
    } else {
        format!(
            "{:+.2}%",
            (after as f64 - before as f64) * 100.0 / before as f64
        )
    }
}

fn diff(before: &Summary, after: &Summary) -> Result<String, String> {
    let identities: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let mut out = String::from("PRF-22 profile diff, after minus before\nTimes are nanoseconds. Each metric ranks absolute changes within its kind.\n");
    for kind in ["actor", "script", "quest", "plugin", "system"] {
        let _ = writeln!(out, "\n{kind}:");
        let mut changed = false;
        for metric in metric_names() {
            let mut rows = Vec::new();
            for identity in &identities {
                if identity.0 != kind {
                    continue;
                }
                let old = before
                    .get(*identity)
                    .and_then(|row| row.get(metric))
                    .copied()
                    .unwrap_or(0);
                let new = after
                    .get(*identity)
                    .and_then(|row| row.get(metric))
                    .copied()
                    .unwrap_or(0);
                if old != new {
                    rows.push((&identity.1, old, new));
                }
            }
            rows.sort_by(|a, b| {
                b.1.abs_diff(b.2)
                    .cmp(&a.1.abs_diff(a.2))
                    .then_with(|| a.0.cmp(b.0))
            });
            if rows.is_empty() {
                continue;
            }
            changed = true;
            let _ = writeln!(out, "  {metric}: entity before after delta change");
            for (id, old, new) in rows.into_iter().take(10) {
                let delta = i128::from(new) - i128::from(old);
                let _ = writeln!(
                    out,
                    "    {} {old} {new} {delta:+} {}",
                    serde_json::to_string(id).map_err(|error| error.to_string())?,
                    percent(old, new)
                );
            }
        }
        if !changed {
            out.push_str("  no changes\n");
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prf_22_diff_groups_script_functions_and_ranks_each_count_with_percentages() {
        let summary = |slow, count| {
            serde_json::to_vec(&json!({"traceEvents": [
            {"ph":"C", "cat":"script", "name":"SlowScript.One.execute", "args":{"self_ns":slow,"calls":count}},
            {"ph":"C", "cat":"script", "name":"SlowScript.Two.execute", "args":{"self_ns":slow,"calls":count}},
            {"ph":"C", "cat":"script", "name":"Other.Run.execute", "args":{"self_ns":100,"calls":count * 10}}
        ]})).unwrap()
        };
        let text = diff(
            &parse(&summary(100, 1)).unwrap(),
            &parse(&summary(1000, 2)).unwrap(),
        )
        .unwrap();
        assert!(text.contains("\"SlowScript\" 200 2000 +1800 +900.00%"));
        assert!(text
            .contains("calls: entity before after delta change\n    \"Other\" 10 20 +10 +100.00%"));
    }

    #[test]
    fn prf_22_diff_rejects_invalid_summaries_and_counter_overflow() {
        for value in [
            json!({}),
            json!({"traceEvents":[{"ph":"X"}]}),
            json!({"traceEvents":[{"ph":"C","cat":"actor","name":"a","args":{"self_ns":-1,"calls":1}}]}),
        ] {
            assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let event =
            json!({"ph":"C","cat":"actor","name":"a.tick","args":{"self_ns":u64::MAX,"calls":1}});
        assert!(
            parse(&serde_json::to_vec(&json!({"traceEvents":[event.clone(),event]})).unwrap())
                .is_err()
        );
    }

    #[test]
    fn prf_22_diff_zero_baselines_removals_and_ties_are_stable() {
        assert_eq!(percent(0, 1), "new (baseline 0)");
        assert_eq!(percent(10, 0), "-100.00%");
        let before = Summary::from([(
            ("actor".into(), "z".into()),
            Metrics::from([("calls".into(), 10)]),
        )]);
        let after = Summary::from([(
            ("actor".into(), "a".into()),
            Metrics::from([("calls".into(), 10)]),
        )]);
        assert!(diff(&before, &after)
            .unwrap()
            .contains("\"a\" 0 10 +10 new (baseline 0)\n    \"z\" 10 0 -10 -100.00%"));
    }
}
