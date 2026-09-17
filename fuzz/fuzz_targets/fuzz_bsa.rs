#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let _ = soe_bsa::index_archive_in(data, Path::new("fuzz.bsa"));
});
