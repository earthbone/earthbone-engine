//! Sanity check the generated parser seeds. Any rejected seed fails the command.

use std::path::Path;

fn main() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let read =
        |target: &str, name: &str| std::fs::read(base.join(target).join(name)).expect("read seed");
    soe_esm::Plugin::parse(&read("fuzz-esm", "seed_default.bin")).expect("esm seed");
    soe_bsa::index_archive_in(&read("fuzz-bsa", "seed_empty.bin"), Path::new("seed.bsa"))
        .expect("bsa seed");
    soe_nif::parse(&read("fuzz-nif", "seed_empty.bin")).expect("nif seed");
    soe_hkx::Packfile::parse(&read("fuzz-hkx", "seed_empty.bin")).expect("hkx seed");
    soe_dds::Dds::parse(&read("fuzz-dds", "seed_bc1.bin")).expect("dds seed");
    soe_papyrus::pex::parse(&read("fuzz-pex", "seed_default.bin")).expect("pex seed");
    soe_audio::Fuz::split(&read("fuzz-audio", "seed_empty.bin")).expect("audio seed");
    soe_fuzz::state_probe()
        .load_state(&read("fuzz-vm-state", "seed_default.bin"))
        .expect("state seed");
    for target in ["fuzz-profile", "fuzz-moddata"] {
        let text = String::from_utf8(read(target, "seed_default.bin")).expect("text seed");
        assert!(!text.is_empty(), "{target} seed is empty");
    }
    println!("all seeds passed");
}
