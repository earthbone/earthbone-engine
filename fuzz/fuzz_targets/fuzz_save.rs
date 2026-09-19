#![no_main]

//! The vanilla save loader, on bytes nobody wrote on purpose.
//!
//! A save is the most trusted file the engine reads and the least trustworthy
//! in practice. It comes from a crash, an older build, a different mod list or
//! a hand edit, and unlike a plugin it is not something an author tested. The
//! reader already round trips the bodies of sixty saves in the corpus, but
//! that only covers files the game itself wrote.
//!
//! Two properties here. Parsing must not panic, whatever the bytes. And when
//! parsing succeeds, writing the result back must preserve its header and body,
//! because a reader that silently drops a field would otherwise look correct
//! right up until someone saves over their game with a shortened file.

use libfuzzer_sys::fuzz_target;
use soe_save::Save;

fuzz_target!(|data: &[u8]| {
    let Ok(save) = Save::parse(data) else {
        return;
    };
    // `write` reassembles from the parsed model. Anything the reader did not
    // keep shows up here as a difference, which is the failure worth catching.
    let (_, header_len, body) = Save::split(data).expect("parsed save must split");
    assert_eq!(save.body_bytes(), body, "save body changed during parsing");
    let again = save.write().expect("parsed save must write");
    let (_, written_header_len, written_body) =
        Save::split(&again).expect("written save must split");
    assert_eq!(&data[..header_len], &again[..written_header_len]);
    assert_eq!(body, written_body, "save body changed during writing");
});
