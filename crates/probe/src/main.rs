// SPDX-License-Identifier: GPL-3.0-or-later

//! File-oriented vanilla log bridge. Command contracts are in docs/probe.md.

use std::{io::Write, path::Path, process::ExitCode};

use soe_probe::{build, compare, parse_log, run_installed, Error, Observation, Probe, EXAMPLE};

fn output(value: &impl serde::Serialize) -> Result<(), Error> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

fn execute() -> Result<u8, Error> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        Some("build") if args.len() == 2 || args.len() == 3 => {
            let source = if let Some(path) = args.get(2) { std::fs::read_to_string(path)? } else { EXAMPLE.into() };
            build(&Probe::parse(&source)?, Path::new(&args[1]))?;
            Ok(0)
        }
        Some("parse") if args.len() == 2 => {
            output(&parse_log(&std::fs::read_to_string(&args[1])?)?)?;
            Ok(0)
        }
        Some("replay") if args.len() == 2 => {
            let probe = Probe::parse(&std::fs::read_to_string(&args[1])?)?;
            let data = std::env::var_os("SKYRIM_DATA").ok_or_else(|| Error::Invalid("replay requires SKYRIM_DATA".into()))?;
            output(&run_installed(&probe, Path::new(&data))?)?;
            Ok(0)
        }
        Some("compare") if args.len() == 3 || args.len() == 4 => {
            let probe = Probe::parse(&std::fs::read_to_string(&args[1])?)?;
            let source = std::fs::read_to_string(&args[2])?;
            let observations: Vec<Observation> = match serde_json::from_str(&source) {
                Ok(observations) => observations,
                Err(_) => parse_log(&source)?,
            };
            let engine = if let Some(path) = args.get(3) {
                serde_json::from_str(&std::fs::read_to_string(path)?)?
            } else {
                let data = std::env::var_os("SKYRIM_DATA").ok_or_else(|| Error::Invalid("compare requires SKYRIM_DATA".into()))?;
                run_installed(&probe, Path::new(&data))?
            };
            let report = compare(&probe, &engine, &observations)?;
            let status = if report.matched { 0 } else { 2 };
            output(&report)?;
            Ok(status)
        }
        _ => Err(Error::Invalid("usage: soe-probe build <out_dir> [probe.toml] | parse <Papyrus.0.log> | replay <probe.toml> | compare <probe.toml> <observations.json or Papyrus.0.log> [expected.json]".into())),
    }
}

fn main() -> ExitCode {
    match execute() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
