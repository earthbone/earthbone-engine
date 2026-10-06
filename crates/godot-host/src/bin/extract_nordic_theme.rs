// SPDX-License-Identifier: GPL-3.0-or-later

//! UI-24 art extraction reads the user installs named by the backlog contract.

use std::path::PathBuf;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let nordic = PathBuf::from(std::env::var("SOE_TEST_NORDIC")?);
    let skyui = PathBuf::from(std::env::var("SOE_TEST_SKYUI")?);
    let data = PathBuf::from(std::env::var("SKYRIM_DATA")?);
    let destination = std::env::args()
        .nth(1)
        .ok_or("supply an empty extraction destination")?;
    let manifest = soe_godot_host::nordic_theme::extract(
        &nordic,
        &soe_bsa::Archive::open(&skyui.join("SkyUI_SE.bsa"))?,
        &soe_bsa::Archive::open(&data.join("Skyrim - Interface.bsa"))?,
        &PathBuf::from(destination),
    )?;
    println!("{manifest}");
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("NORDIC theme extraction: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
