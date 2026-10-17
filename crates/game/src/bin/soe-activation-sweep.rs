// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed activation survey. Expectations follow docs/validation.md.

use serde_json::json;
use soe_game::activation_sweep::{
    counts, installed_order, installed_vfs, run_cell, selected_cells,
};
use std::path::Path;
use std::process::ExitCode;

fn run() -> Result<bool, String> {
    let data = std::env::var_os("SKYRIM_DATA").ok_or("SKYRIM_DATA is required")?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = args
        .first()
        .ok_or("usage: soe-activation-sweep <report.json> [--all-cells | cell-editor-id]")?;
    let order = installed_order(Path::new(&data))?;
    let index = soe_esm::CellIndex::build(&order);
    if args.iter().any(|arg| arg == "--catalog") {
        let cells: Vec<_> = selected_cells(&order, &index, false)
            .into_iter()
            .map(|id| {
                json!({
            "id":id.to_string(), "editor_id":order.winner(id).and_then(soe_esm::Record::editor_id),
            "references":index.references(id).len()})
            })
            .collect();
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&cells).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(true);
    }
    let cells = if let Some(name) = args.get(1).filter(|name| !name.starts_with("--")) {
        vec![index
            .cell_by_editor_id(name)
            .ok_or_else(|| format!("unknown cell {name}"))?]
    } else {
        selected_cells(&order, &index, args.iter().any(|a| a == "--all-cells"))
    };
    let vfs = installed_vfs(Path::new(&data), &order)?;
    if cells.is_empty() {
        return Err("no cells selected".into());
    }
    let requested_cells: Vec<_> = cells.iter().map(ToString::to_string).collect();
    let total = cells.len();
    let mut results = Vec::new();
    for (index, cell) in cells.into_iter().enumerate() {
        eprintln!(
            "activation sweep: {} {}",
            cell,
            order
                .winner(cell)
                .and_then(soe_esm::Record::editor_id)
                .unwrap_or_default()
        );
        results.push(run_cell(&order, &vfs, cell)?);
        let report = json!({"schema":1,"oracle":"CK contracts and installed records, no captured vanilla runtime oracle",
            "complete":index + 1 == total,"requested_cells":requested_cells,
            "settle_seconds":3,"isolation":"fresh cell, sequential activations within cell",
            "counts":counts(&results),"cells":results});
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(results
        .iter()
        .filter_map(|c| c["references"].as_array())
        .flatten()
        .all(|r| r["status"] == "pass"))
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("activation sweep: {error}");
            ExitCode::from(2)
        }
    }
}
