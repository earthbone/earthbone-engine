#![no_main]

use libfuzzer_sys::fuzz_target;
use soe_papyrus::pex;

fuzz_target!(|data: &[u8]| {
    let _ = pex::parse(data);
});
