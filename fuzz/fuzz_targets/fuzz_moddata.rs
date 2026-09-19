#![no_main]

//! The text formats described in docs/moddata.md, parsed through their real
//! INI frontend. Paths are diagnostic labels and no file is read or written.

use libfuzzer_sys::fuzz_target;
use soe_moddata::{bos::SwapFile, ini::Ini, kid::KidFile, spid::DistrFile};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let ini = Ini::parse("fuzz.ini", &text);
    let _ = DistrFile::from_ini(&ini);
    let _ = KidFile::from_ini(&ini);
    let _ = SwapFile::from_ini(&ini);
});
