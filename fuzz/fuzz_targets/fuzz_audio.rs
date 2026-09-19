#![no_main]

//! Audio container framing and accessors, without external decoders.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(fuz) = soe_audio::Fuz::split(data) {
        assert_eq!(fuz.to_bytes(), data);
        let _ = fuz.audio_kind();
        if let Ok(lip) = soe_audio::Lip::parse(fuz.lip) {
            let _ = lip.targets();
        }
        let _ = soe_audio::Xwma::parse(fuz.audio);
    }
    if let Ok(lip) = soe_audio::Lip::parse(data) {
        let _ = lip.targets();
    }
    let _ = soe_audio::riff::Riff::parse(data);
    if let Ok(xwma) = soe_audio::Xwma::parse(data) {
        let _ = xwma.duration_seconds();
        let written = xwma.to_bytes();
        assert!(soe_audio::Xwma::parse(&written).is_ok());
    }
});
