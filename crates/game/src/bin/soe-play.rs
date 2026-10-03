// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 && !(args.len() == 4 && args[3] == "--world-checkpoint") {
        eprintln!("usage: soe-play <Data-directory> <external-save.ess> <route-file> [--world-checkpoint]");
        return ExitCode::from(2);
    }
    let result = if args.len() == 4 {
        run_checkpoint(&args)
    } else {
        soe_game::play::run_from_save(
            Path::new(&args[0]),
            Path::new(&args[1]),
            Path::new(&args[2]),
        )
    };
    match result {
        Ok(report) => {
            print!("{}", report.format());
            if report.passed() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("soe-play: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_checkpoint(args: &[String]) -> Result<soe_game::play::Report, soe_game::play::Error> {
    use soe_game::play::{self, Error, Route};
    let text = std::fs::read_to_string(&args[2]).map_err(|e| Error::Startup(e.to_string()))?;
    let route = Route::parse(&text)?;
    let mut game = play::load_world_checkpoint(Path::new(&args[0]), Path::new(&args[1]))?;
    eprintln!("WORLD CHECKPOINT: supported ESS world fields restored, saved VM state omitted, fresh scripts bound");
    Ok(play::run(&mut game, &route))
}
