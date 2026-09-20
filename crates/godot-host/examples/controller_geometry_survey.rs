// SPDX-License-Identifier: GPL-3.0-or-later

//! Inspect controller collision carried by installed actor skeletons.

use std::collections::BTreeMap;
use std::path::PathBuf;

use soe_bsa::Archive;
use soe_physics::bhk::Layer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = PathBuf::from(std::env::var("SKYRIM_DATA")?);
    let mut total = 0;
    let mut authored = 0;
    let mut skipped = BTreeMap::new();
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = Archive::open(&data.join(name))?;
        for path in archive.files().iter().filter(|path| {
            path.contains("actors/")
                && path.contains("skeleton")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
        }) {
            let file = soe_nif::parse(&archive.read(path)?)?;
            let collected = soe_physics::shape::collect(&file)?;
            let controllers: Vec<_> = collected
                .phantoms
                .iter()
                .filter(|body| body.layer == Layer::CHAR_CONTROLLER)
                .collect();
            total += 1;
            authored += usize::from(!controllers.is_empty());
            println!(
                "{path}: controllers={} other={} skipped={}",
                controllers.len(),
                collected.colliders.len(),
                collected.skipped.len()
            );
            for body in controllers {
                println!(
                    "  {}: transform={:?} shape={:?}",
                    body.node_name, body.transform, body.shape
                );
            }
            for (_, reason) in collected.skipped {
                *skipped.entry(reason).or_insert(0) += 1;
            }
        }
    }
    println!("Skeleton files={total}, carrying controller bodies={authored}, skipped={skipped:?}");
    Ok(())
}
