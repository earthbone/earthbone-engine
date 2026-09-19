#![no_main]

use libfuzzer_sys::fuzz_target;
use soe_hkx::Packfile;

fuzz_target!(|data: &[u8]| {
    let _ = Packfile::parse(data);
});
