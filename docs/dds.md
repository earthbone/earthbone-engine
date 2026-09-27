# soe-dds: DDS textures

`crates/dds` reads the DDS container and decodes every block format Skyrim SE ships to RGBA8,
level by level, with no dependency beyond `thiserror`. The `encode` feature adds re-encoding
through `image_dds` for the mod manager's `TransformedTexture` step, so the manager and the
engine share one texture crate.

## Reading

`Dds::parse(&bytes)` reads the 124-byte header and the DX10 extension, decides the
[`Format`] once, lays out every level (layer by layer, mip by mip, volumes slice by slice) and
refuses a file that does not hold all the levels it declares with `Error::Truncated`. Levels
are addressed as `(layer, mip)`, where a layer is one cube face or array element. `decode`
returns an [`Image`] of RGBA8, `decode_chain` the whole mip chain of a layer, `level_bytes`
the still-encoded data for a renderer that uploads compressed textures directly.

Formats: BC1, BC2, BC3, BC4 and BC5 (signed and unsigned), BC7, and any uncompressed pixel
of 8, 16, 24 or 32 bits described by channel masks, which covers RGBA8, BGRA8, BGRX8, BGR8,
R8, RG8, luminance and the packed 16-bit formats with one decoder. Legacy FourCCs (DXT1 to
DXT5, ATI1, ATI2, BC4U, BC4S, BC5U, BC5S) and DX10 DXGI formats both resolve. sRGB is
reported as a [`ColorSpace`] and never converted. BC6H, unknown FourCCs, unknown DXGI values
and mask layouts outside the generic decoder's reach are `Error::Unsupported` with the
offending value, so a caller can log the exact format it met.

BC4 decodes into red and BC5 into red and green, other channels zero and alpha opaque. Signed
blocks are decoded on the -127 to 127 range the format defines and mapped to 0 to 255, so a
normal map reads the same whichever way it was stored.

## Arithmetic

The BC1 colour expansion and interpolation follow bcdec's integer formulas, the BC7 partition
and anchor tables were generated from bcdec's source by a script, and the rest follows the
Direct3D 11 specification. The real-file test compares this decoder with `image_dds` (which
decodes through bcndecode) on vanilla textures, and the BC7 round trip compares it on blocks
the Intel encoder produces from noise and gradients (modes 1 and 3 to 7), and on random
blocks with the mode bits forced, which covers all eight modes including the three-subset
modes 0 and 2 that encoders rarely choose: 256 random blocks per mode, all identical to the
reference.

## What vanilla Skyrim SE actually uses

Measured on 2026-09-06 across a sample of 8,571 textures from all nine `Skyrim - Textures`
archives:

| Format | Textures | Worst channel difference against image_dds |
|---|---|---|
| BC3 | 6,321 | 1 |
| BC1 | 2,250 | 0 |
| BGRX8 | 1,959 | 0 |
| BGRA8 | 165 | 0 |
| BGR8 | 63 | 0 |
| RGBA8 | 10 | 0 |
| L8 | 1 | 0 |

No vanilla texture is BC7, BC4 or BC5. BC7 arrives with mods and with the mod manager's
own re-encoding, which is why the round trip test exists. The difference of one on BC3 is
bcndecode rounding the alpha ramp differently from the specification's integer division. No
vanilla texture was unsupported.

## Encoding (`encode` feature)

`encode::encode_rgba8(width, height, rgba, mip_levels, dxgi_format)` produces a DDS file in
memory, mip count zero meaning a full chain. `encode::recompress(src, Target)` decodes the base
level with this crate, resizes with Lanczos3 when the size differs, and encodes, which is the
`TransformedTexture` directive as Wabbajack performs it. The DXGI set is the one Wabbajack can
write, BC6H included on the encode side only. Two block compressors never agree byte for
byte, so a re-encoded texture is checked by header, not by hash, as Wabbajack does.

`encode::export_png` decodes a PNG and writes a slot-profiled DDS. `SlotFormat` selects BC1 sRGB
for opaque diffuse color, BC3 sRGB for diffuse with alpha, linear BC3 for Skyrim tangent-space
normals, or linear BC4 for a single-channel mask. The tangent-normal profile keeps alpha because
PyNifly's Skyrim shader notes identify it as specular input. `MipChain` selects the base image,
the complete chain, or an exact count including the base. Explicit counts larger than the full
chain are rejected. The output keeps the source dimensions and channels before compression.

The editor import plan calls for PNG-to-DDS export with material and slot mappings in
`docs/editor.md`, milestone 4. `nifly/include/Shaders.hpp` defines the texture set as ordered
texture paths, and `nifly/src/Shaders.cpp` sizes that list by NIF version. PyNifly's
[shader reference](https://github.com/BadDogSkyrim/PyNifly/wiki/Shaders) explains Skyrim texture
channel roles and its preference for PNG source images. The export uses the existing
`image_dds` encoder, and keeps slot selection explicit because the NIF slot identifies texture
meaning while compression is an export choice.

## Tests

Unit tests build headers and blocks by hand: the plan's synthetic BC1 block decodes to the
expected four colours, the three-colour mode's transparent slot, BC2 nibble alpha, BC3 alpha
ramps, BC5 channel placement, signed BC4 range, BC7 mode 6 and mode 5 with rotation, the
reserved mode, mask-described pixels in every layout, cube map layout, truncation and
every unsupported case as a typed error. `tests/real_data.rs` runs when `SKYRIM_DATA` names a
Skyrim SE `Data/` folder and reads the textures straight out of the archives through
`soe-bsa`, `SOE_DDS_SAMPLE` sets how many per archive (default 150). `tests/bc7_roundtrip.rs`
needs the `encode` feature. `tests/edt_16_png_export.rs` checks PNG conversion, slot format and
color-space declarations, mip dimensions, tolerance after DDS reload, and invalid input rejection.

## Live run (2026-09-14)

The gated test compared 1134 textures pulled from the installed archives against the
image_dds reference decoder. Formats seen were BC1 (272 textures), BC3 (862), BGR8
(5), BGRA8 (15) and BGRX8 (196). The worst per channel difference was 0 for BC1 and 1
for BC3, both within tolerance, and every format decoded. No BC7 texture turned up in
the sample, that format stays covered by the synthetic round trip in
tests/bc7_roundtrip.rs. The test finished in 20.93 seconds with no failures and no
code change was needed.

## Untested

BC4 and BC5 against textures in the wild (only synthetic blocks), BC7 against textures in the
wild (random and encoder-produced blocks match the reference decoder exactly),
texture arrays with more than one element (the layout code is shared with cube maps, which
are tested), volume textures beyond the layout arithmetic, and BC6H is not decoded at all.
