#![no_main]

//! Grammar driven DDS fuzzing, aimed at the decoder rather than the parser.
//!
//! `fuzz-dds` feeds raw bytes to `Dds::parse` and stops there. Random bytes
//! almost never form a header the parser accepts, so the decoder behind it has
//! never been fuzzed at all, and the decoder is where the arithmetic lives:
//! block counts, mip chains, surface sizes, slice offsets. That arithmetic on
//! attacker chosen dimensions is the interesting target.
//!
//! This builds a header the parser will accept, with dimensions, a format and
//! a mip count drawn from the fuzzer, then a payload that is deliberately
//! whatever length the fuzzer felt like. Then it decodes every mip of every
//! layer. The assertion is only that nothing panics and that a decoded image
//! is self consistent, because a truncated payload is a legitimate error and
//! not a defect.

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soe_dds::Dds;

const HEADER_SIZE: u32 = 124;
const PF_FOURCC: u32 = 0x4;
const PF_RGB: u32 = 0x40;
const PF_ALPHA: u32 = 0x1;

#[derive(Arbitrary, Debug)]
enum Format {
    Dxt1,
    Dxt3,
    Dxt5,
    Ati1,
    Ati2,
    Bc5S,
    Bgra8,
    Bgr8,
    Dx10(u32),
}

#[derive(Arbitrary, Debug)]
struct Plan {
    format: Format,
    /// Kept small so a case is cheap, and shifted so zero and one are reached.
    width_log: u8,
    height_log: u8,
    mips: u8,
    cubemap: bool,
    depth: u16,
    array_size: u8,
    payload: Vec<u8>,
    trailing: u8,
}

fn four_cc(f: &Format) -> Option<[u8; 4]> {
    match f {
        Format::Dxt1 => Some(*b"DXT1"),
        Format::Dxt3 => Some(*b"DXT3"),
        Format::Dxt5 => Some(*b"DXT5"),
        Format::Ati1 => Some(*b"ATI1"),
        Format::Ati2 => Some(*b"ATI2"),
        Format::Bc5S => Some(*b"BC5S"),
        Format::Dx10(_) => Some(*b"DX10"),
        Format::Bgra8 | Format::Bgr8 => None,
    }
}

fuzz_target!(|plan: Plan| {
    let mut out: Vec<u8> = Vec::with_capacity(256 + plan.payload.len());
    out.extend_from_slice(b"DDS ");

    let mut h = [0u8; HEADER_SIZE as usize];
    let put = |h: &mut [u8], at: usize, v: u32| {
        h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    };
    put(&mut h, 0, HEADER_SIZE);
    // Caps, height, width, pixel format and mip count all declared present.
    put(&mut h, 4, 0x1 | 0x2 | 0x4 | 0x1000 | 0x2_0000);
    put(&mut h, 8, 1u32 << (plan.height_log % 9));
    put(&mut h, 12, 1u32 << (plan.width_log % 9));
    put(&mut h, 16, 0);
    put(&mut h, 20, u32::from(plan.depth % 4));
    put(&mut h, 24, u32::from(plan.mips % 12));

    let pfo = 72;
    put(&mut h, pfo, 32);
    match four_cc(&plan.format) {
        Some(cc) => {
            put(&mut h, pfo + 4, PF_FOURCC);
            h[pfo + 8..pfo + 12].copy_from_slice(&cc);
        }
        None => {
            let alpha = matches!(plan.format, Format::Bgra8);
            put(&mut h, pfo + 4, PF_RGB | if alpha { PF_ALPHA } else { 0 });
            put(&mut h, pfo + 12, if alpha { 32 } else { 24 });
            put(&mut h, pfo + 16, 0x00ff_0000);
            put(&mut h, pfo + 20, 0x0000_ff00);
            put(&mut h, pfo + 24, 0x0000_00ff);
            put(&mut h, pfo + 28, if alpha { 0xff00_0000 } else { 0 });
        }
    }
    put(&mut h, 104, 0x1000);
    put(&mut h, 108, if plan.cubemap { 0x200 | 0xfc00 } else { 0 });
    out.extend_from_slice(&h);

    if let Format::Dx10(dxgi) = plan.format {
        let mut d = [0u8; 20];
        put(&mut d, 0, dxgi);
        put(&mut d, 4, if plan.depth > 0 { 4 } else { 3 });
        put(&mut d, 8, if plan.cubemap { 0x4 } else { 0 });
        put(&mut d, 12, u32::from(plan.array_size).max(1));
        out.extend_from_slice(&d);
    }

    out.extend_from_slice(&plan.payload);
    out.extend(std::iter::repeat_n(
        plan.trailing,
        usize::from(plan.trailing) % 32,
    ));

    let Ok(dds) = Dds::parse(&out) else {
        return;
    };

    // Reaching here means the header was accepted, which is the whole point:
    // the decoder now runs on dimensions and a payload length the fuzzer chose.
    let layers = dds.layer_count().min(8);
    let mips = dds.mip_count().min(12);
    for layer in 0..layers {
        for mip in 0..mips {
            if let Ok(image) = dds.decode(layer, mip) {
                let pixels = image.rgba.len() / 4;
                assert_eq!(
                    image.rgba.len() % 4,
                    0,
                    "a decoded image must be whole RGBA pixels"
                );
                assert_eq!(
                    pixels,
                    image.width as usize * image.height as usize * image.depth.max(1) as usize,
                    "decoded pixel count must match the dimensions reported with it"
                );
            }
        }
        let _ = dds.decode_chain(layer);
    }
});
