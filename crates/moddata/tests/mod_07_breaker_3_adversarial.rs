// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_moddata::{discover, ConfigKind};

#[test]
fn mod_07_discovers_distributor_files_below_data_root() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("mods/Example");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("Example_DISTR.ini"), "Spell = 0x123|NONE\n").unwrap();
    std::fs::write(nested.join("Example_KID.ini"), "Keyword = 0x123|Armor|Iron\n").unwrap();
    std::fs::write(nested.join("Example_SWAP.ini"), "[Forms]\nRock|Tree\n").unwrap();

    let found = discover(root.path()).unwrap();
    assert_eq!(
        found.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        [ConfigKind::Distr, ConfigKind::Kid, ConfigKind::Swap]
    );
}
