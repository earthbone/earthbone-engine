// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless gameplay sweeps. Expectations follow docs/validation.md.

use soe_game::activation_sweep::installed_order;
use soe_game::gameplay_sweeps::{self, consumables, Options};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str =
    "usage: soe-gameplay-sweep <consumables> <report.json> [--only <editor id prefix>] [--limit <n>]";

fn run() -> Result<bool, String> {
    let data = std::env::var_os("SKYRIM_DATA").ok_or("SKYRIM_DATA is required")?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let kind = args.first().ok_or(USAGE)?;
    let output = args.get(1).ok_or(USAGE)?;
    let options = Options::from_args(&args);
    let order = installed_order(Path::new(&data))?;
    let report = match kind.as_str() {
        "consumables" => consumables::run(&order, &options),
        _ => return Err(USAGE.into()),
    };
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        Path::new(output).with_extension("md"),
        gameplay_sweeps::markdown(&report),
    )
    .map_err(|e| e.to_string())?;
    Ok(gameplay_sweeps::all_passed(&report))
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("gameplay sweep: {error}");
            ExitCode::from(2)
        }
    }
}
