// SPDX-License-Identifier: MIT

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let files = soe_commonlib_shim::headers::write_headers(&root)?;
    for file in files {
        println!("{}", root.join(file).display());
    }
    Ok(())
}
