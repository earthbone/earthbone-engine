// SPDX-License-Identifier: GPL-3.0-or-later

//! List an archive, optionally filtered: `cargo run -p soe-bsa --example ls -- X.bsa [substring]`.

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: ls <archive> [substring]");
        std::process::exit(2);
    };
    let filter = args.next().unwrap_or_default().to_lowercase();
    let index = match soe_bsa::index_archive(std::path::Path::new(&path)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("{}: {} files", index.format, index.files.len());
    for f in &index.files {
        if filter.is_empty() || f.to_lowercase().contains(&filter) {
            println!("{f}");
        }
    }
}
