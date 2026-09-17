#![no_main]

use libfuzzer_sys::fuzz_target;
use soe_esm::Plugin;

fuzz_target!(|data: &[u8]| {
    let _ = Plugin::parse(data);
});
