// SPDX-License-Identifier: GPL-3.0-or-later

//! Extract an archive, or the files under one folder of it:
//! `cargo run -p soe-bsa --example extract -- X.bsa OUT_DIR [folder prefix]`.

use std::path::{Path, PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: extract <archive> <out dir> [folder prefix]");
        std::process::exit(2);
    };
    let prefix = args.next().unwrap_or_default().to_lowercase();
    let archive = match soe_bsa::Archive::open(Path::new(&path)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let out = PathBuf::from(out);
    let mut written = 0usize;
    for name in archive.files() {
        if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix) {
            continue;
        }
        let target = out.join(name.replace('\\', "/"));
        let result = archive.read(name).and_then(|bytes| {
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&target, bytes)?;
            Ok(())
        });
        match result {
            Ok(()) => written += 1,
            Err(e) => eprintln!("{name}: {e}"),
        }
    }
    println!("{written} files written under {}", out.display());
}
