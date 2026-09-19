// SPDX-License-Identifier: GPL-3.0-or-later

//! Print what the reader sees in one or more NIF files: stream version,
//! block types, root types, and one line per mesh. Pass `--quiet` to print
//! only files that yield no meshes.
//!
//! ```text
//! cargo run -p soe-nif --example nif_dump -- meshes/clutter/bucket01.nif
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quiet = args.iter().any(|a| a == "--quiet");
    for path in args.iter().filter(|a| *a != "--quiet") {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let file = match soe_nif::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let meshes = match file.meshes() {
            Ok(m) => m,
            Err(e) => {
                println!("{path}: mesh walk: {e}");
                continue;
            }
        };
        if quiet && !meshes.is_empty() {
            continue;
        }
        let roots: Vec<&str> = file
            .roots
            .iter()
            .filter_map(|r| file.get(*r))
            .map(|b| b.type_name())
            .collect();
        println!(
            "{path}: BS {} blocks {} roots {:?} meshes {}",
            file.header.bs_version,
            file.blocks.len(),
            roots,
            meshes.len()
        );
        let mut types: Vec<&str> = file.block_types();
        types.sort_unstable();
        types.dedup();
        println!("  blocks: {}", types.join(", "));
        for m in &meshes {
            println!(
                "  {} verts {} tris {} {:?} diffuse {:?} skinned {}",
                m.name,
                m.vertex_count(),
                m.triangle_count(),
                m.material.kind,
                m.material.diffuse().unwrap_or(""),
                m.skin.is_some()
            );
        }
    }
}
