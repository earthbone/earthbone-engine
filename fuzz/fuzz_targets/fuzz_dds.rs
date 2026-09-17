#![no_main]

use libfuzzer_sys::fuzz_target;
use soe_dds::Dds;

fuzz_target!(|data: &[u8]| {
    let _ = Dds::parse(data);
});
