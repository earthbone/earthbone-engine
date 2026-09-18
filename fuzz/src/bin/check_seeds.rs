//! Sanity check: every seed in `fuzz/corpus/<target>/` parses as `Ok` through
//! its target's entry point. Run once after `gen_seeds`, not part of the
//! fuzz harness itself.

use std::path::Path;

fn main() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");

    let esm = std::fs::read(base.join("fuzz-esm/seed_default.bin")).unwrap();
    println!("esm: {:?}", soe_esm::Plugin::parse(&esm).map(|_| ()));

    let bsa = std::fs::read(base.join("fuzz-bsa/seed_empty.bin")).unwrap();
    println!(
        "bsa: {:?}",
        soe_bsa::index_archive_in(&bsa, Path::new("seed.bsa")).map(|_| ())
    );

    let nif = std::fs::read(base.join("fuzz-nif/seed_empty.bin")).unwrap();
    println!("nif: {:?}", soe_nif::parse(&nif).map(|_| ()));

    let havok = std::fs::read(base.join("fuzz-havok/seed_empty.bin")).unwrap();
    println!(
        "havok: {:?}",
        soe_havok::Packfile::parse(&havok).map(|_| ())
    );

    let dds = std::fs::read(base.join("fuzz-dds/seed_bc1.bin")).unwrap();
    println!("dds: {:?}", soe_dds::Dds::parse(&dds).map(|_| ()));

    let pex = std::fs::read(base.join("fuzz-pex/seed_default.bin")).unwrap();
    println!("pex: {:?}", soe_papyrus::pex::parse(&pex).map(|_| ()));
}
