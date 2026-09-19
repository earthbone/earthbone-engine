// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit upper bounds for measured durations. Missing metrics fail the
//! run so a misspelled name cannot silently disable a performance gate.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct Gates {
    limits: BTreeMap<String, f64>,
    seen: BTreeSet<String>,
}

impl Gates {
    pub(crate) fn add(&mut self, value: &str) -> Result<(), String> {
        let (name, seconds) = value.split_once('=').ok_or("expected metric=seconds")?;
        let seconds: f64 = seconds.parse().map_err(|_| "invalid seconds")?;
        if name.is_empty() || !seconds.is_finite() || seconds <= 0.0 {
            return Err("metric must be named and seconds finite and positive".into());
        }
        if self.limits.contains_key(name) {
            return Err(format!("duplicate performance limit: {name}"));
        }
        self.limits.insert(name.to_string(), seconds);
        Ok(())
    }

    pub(crate) fn observe(&mut self, name: &str, seconds: f64) -> Result<(), String> {
        if let Some(limit) = self.limits.get(name) {
            self.seen.insert(name.to_string());
            if !seconds.is_finite() || seconds > *limit {
                return Err(format!(
                    "performance gate failed: {name} {seconds:.6}s exceeds {limit:.6}s"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<(), String> {
        let missing: Vec<_> = self
            .limits
            .keys()
            .filter(|name| !self.seen.contains(*name))
            .cloned()
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "performance metrics were not measured: {}",
                missing.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Gates;

    #[test]
    fn limits_fail_on_regressions_missing_names_and_invalid_values() {
        let mut gates = Gates::default();
        for bad in ["", "x", "x=NaN", "x=inf", "x=0", "x=-1", "=1"] {
            assert!(gates.add(bad).is_err(), "accepted {bad}");
        }
        gates.add("startup_median=1").unwrap();
        assert!(gates.add("startup_median=2").is_err());
        assert!(gates.finish().is_err(), "unobserved metric must fail");
        gates.observe("startup_median", 1.0).unwrap();
        gates.finish().unwrap();
        assert!(gates.observe("startup_median", 1.01).is_err());
        assert!(gates.observe("startup_median", f64::NAN).is_err());
    }
}
