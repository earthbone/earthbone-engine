#![no_main]

//! SOEV state parsing followed by bounded execution. Successful parsing must
//! leave a VM whose frame and instance references can be used safely.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut vm = soe_fuzz::state_probe();
    if vm.load_state(data).is_ok() {
        vm.advance(1.0);
        vm.run(128);
        let _ = vm.save_state();
    }
});
