// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::ExitCode;

use soe_game::validation_runner::{format_report, run_profile};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        eprintln!("usage: soe-validate <game-dir> <mo2-root> <profile-name> <scenario-dir>");
        return ExitCode::from(2);
    }

    let results = match run_profile(
        Path::new(&args[0]),
        Path::new(&args[1]),
        &args[2],
        Path::new(&args[3]),
    ) {
        Ok(results) => results,
        Err(error) => {
            eprintln!("validation runner: {error}");
            return ExitCode::FAILURE;
        }
    };
    print!("{}", format_report(&results));
    if results.iter().all(|result| result.report.passed()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
