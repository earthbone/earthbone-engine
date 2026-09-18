// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Does any shipped script define one function twice in a state?
//!
//! `asm_unique_names.rs` argues from the format that it cannot, and Caprica
//! rejects it at compile time. This is the empirical half: every compiled
//! script Bethesda shipped, checked. A single counterexample would mean the
//! assembler was right to append and the reader wrong to match by name, so
//! this is the test that could overturn the fix rather than confirm it.
//!
//! Gated on `SKYRIM_DATA`, reads the script archive.

use soe_bsa::Archive;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn archive() -> Option<Archive> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    for name in ["Skyrim - Misc.bsa", "Skyrim - Scripts.bsa"] {
        let path = dir.join(name);
        if path.exists() {
            return Some(Archive::open(&path).expect("the script archive opens"));
        }
    }
    eprintln!("skipping: no script archive under {}", dir.display());
    None
}

#[test]
fn no_shipped_script_defines_a_function_twice_in_a_state() {
    let Some(archive) = archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };

    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| {
            let lower = n.to_ascii_lowercase();
            std::path::Path::new(&lower)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pex"))
        })
        .cloned()
        .collect();
    if names.is_empty() {
        eprintln!("skipping: the archive lists no compiled scripts");
        return;
    }

    let (mut parsed, mut states, mut functions) = (0usize, 0usize, 0usize);
    let mut offenders: Vec<String> = Vec::new();
    for name in &names {
        let Ok(bytes) = archive.read(name) else {
            continue;
        };
        let Ok(script) = soe_papyrus::pex::parse(&bytes) else {
            continue;
        };
        parsed += 1;
        for object in &script.objects {
            for state in &object.states {
                states += 1;
                let mut seen: BTreeMap<String, usize> = BTreeMap::new();
                for f in &state.functions {
                    functions += 1;
                    *seen.entry(f.name.to_ascii_lowercase()).or_default() += 1;
                }
                for (func, n) in seen.iter().filter(|(_, n)| **n > 1) {
                    offenders.push(format!(
                        "{name}: {}.{} defines {func} {n} times",
                        object.name, state.name
                    ));
                }
            }
        }
    }

    eprintln!("shipped scripts checked for repeated function names:");
    eprintln!("  scripts parsed {parsed} of {}", names.len());
    eprintln!("  states         {states}");
    eprintln!("  functions      {functions}");
    eprintln!("  offenders      {}", offenders.len());
    for line in offenders.iter().take(10) {
        eprintln!("    {line}");
    }

    assert!(parsed > 0, "nothing parsed, so nothing was measured");
    assert!(
        offenders.is_empty(),
        "{} shipped states define a function twice, which would mean the \
         assembler was right to append and the reader wrong to match by name",
        offenders.len()
    );
}
