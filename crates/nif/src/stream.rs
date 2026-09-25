// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounds-checked little-endian reads over a byte slice. Every read that
//! could run off the end returns `Error::Truncated`, and every count that
//! sizes an allocation is checked against the bytes left, so a hostile
//! length field cannot make the reader allocate more than the input holds.

use crate::error::{Error, Result};
use crate::types::{BlockRef, Bound, Mat33, Quat, Transform, Vec3};

pub struct Stream<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Stream<'a> {
    pub fn new(data: &'a [u8]) -> Stream<'a> {
        Stream { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::Truncated {
                offset: self.pos,
                needed: pos - self.data.len(),
            });
        }
        self.pos = pos;
        Ok(())
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(Error::Truncated {
                offset: self.pos,
                needed: n - self.remaining(),
            });
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    /// Validate a count read from the file against the bytes left before it
    /// is used to size anything. `element_size` is the smallest number of
    /// bytes each element must occupy.
    pub fn count(&self, count: u32, element_size: usize) -> Result<usize> {
        let need = (count as u64).saturating_mul(element_size as u64);
        if need > self.remaining() as u64 {
            return Err(Error::CountTooLarge {
                offset: self.pos,
                count: count as u64,
                element_size,
            });
        }
        Ok(count as usize)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn half(&mut self) -> Result<f32> {
        Ok(half_to_f32(self.u16()?))
    }

    pub fn vec3(&mut self) -> Result<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }

    pub fn vec2(&mut self) -> Result<[f32; 2]> {
        Ok([self.f32()?, self.f32()?])
    }

    pub fn color3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub fn color4(&mut self) -> Result<[f32; 4]> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    /// Matrix33 is stored one column at a time.
    pub fn mat33(&mut self) -> Result<Mat33> {
        let mut rows = [[0.0f32; 3]; 3];
        for c in 0..3 {
            for row in rows.iter_mut() {
                row[c] = self.f32()?;
            }
        }
        Ok(Mat33 { rows })
    }

    /// NiTransform: rotation, translation, scale.
    pub fn transform(&mut self) -> Result<Transform> {
        Ok(Transform {
            rotation: self.mat33()?,
            translation: self.vec3()?,
            scale: self.f32()?,
        })
    }

    pub fn bound(&mut self) -> Result<Bound> {
        Ok(Bound {
            center: self.vec3()?,
            radius: self.f32()?,
        })
    }

    pub fn block_ref(&mut self) -> Result<BlockRef> {
        Ok(BlockRef(self.i32()?))
    }

    /// Quaternion: w, x, y, z, in that file order.
    pub fn quat(&mut self) -> Result<Quat> {
        Ok(Quat {
            w: self.f32()?,
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }

    /// Read a u32 count and validate it, see `count`.
    pub fn counted(&mut self, element_size: usize) -> Result<usize> {
        let n = self.u32()?;
        self.count(n, element_size)
    }

    /// Read a u16 count and validate it, see `count`.
    pub fn counted16(&mut self, element_size: usize) -> Result<usize> {
        let n = self.u16()?;
        self.count(n as u32, element_size)
    }

    pub fn block_refs(&mut self) -> Result<Vec<BlockRef>> {
        let n = self.counted(4)?;
        (0..n).map(|_| self.block_ref()).collect()
    }

    /// SizedString: u32 length then bytes. Bethesda paths are Windows-1252,
    /// matching the decoding the bsa crate uses for archive file paths.
    pub fn sized_string(&mut self) -> Result<String> {
        let n = self.u32()?;
        let n = self.count(n, 1)?;
        Ok(windows1252(self.take(n)?))
    }

    /// ExportString: u8 length including the terminating NUL.
    pub fn export_string(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        let bytes = self.take(n)?;
        let trimmed = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        Ok(windows1252(trimmed))
    }

    /// The header line, up to and including the newline.
    pub fn line(&mut self, max: usize) -> Result<String> {
        let start = self.pos;
        let limit = (self.pos + max).min(self.data.len());
        while self.pos < limit {
            let b = self.data[self.pos];
            self.pos += 1;
            if b == b'\n' {
                return Ok(windows1252(&self.data[start..self.pos - 1]));
            }
        }
        Err(Error::BadMagic)
    }

    /// A `string` field: index into the header string table, or none.
    pub fn string_index(&mut self, table: &[String]) -> Result<String> {
        let offset = self.pos;
        let idx = self.u32()?;
        if idx == u32::MAX {
            return Ok(String::new());
        }
        table
            .get(idx as usize)
            .cloned()
            .ok_or(Error::BadStringIndex { offset, index: idx })
    }
}

pub fn windows1252(bytes: &[u8]) -> String {
    let (s, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
    s.into_owned()
}

/// IEEE half precision to f32, including subnormals, infinities and NaN.
pub fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            let mut e: i32 = 127 - 15 + 1;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((m & 0x3FF) << 13)
        }
    } else if exp == 31 {
        (sign << 31) | 0x7F80_0000 | (mant << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// f32 to half, round to nearest even. Used by the synthetic writers in the
/// tests and exposed for tools that want to write vertex data back.
pub fn f32_to_half(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x7F_FFFF;
    if exp == 0xFF {
        // Infinity or NaN.
        let m = if mant == 0 { 0 } else { 0x200 };
        return sign | 0x7C00 | m;
    }
    let e = exp - 127 + 15;
    if e >= 31 {
        return sign | 0x7C00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mant | 0x80_0000;
        let shift = (14 - e) as u32;
        let half_mant = m >> shift;
        let rem = m & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let rounded = if rem > halfway || (rem == halfway && half_mant & 1 == 1) {
            half_mant + 1
        } else {
            half_mant
        };
        return sign | rounded as u16;
    }
    let half_mant = mant >> 13;
    let rem = mant & 0x1FFF;
    let mut out = ((e as u32) << 10) | half_mant;
    if rem > 0x1000 || (rem == 0x1000 && out & 1 == 1) {
        out += 1;
    }
    sign | out as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_round_trips_exact_values() {
        for v in [
            0.0f32,
            1.0,
            -1.0,
            0.5,
            0.25,
            2.0,
            65504.0,
            -0.001953125,
            1.0 / 3.0,
        ] {
            let h = f32_to_half(v);
            let back = half_to_f32(h);
            assert!(
                (back - v).abs() <= v.abs() * 0.001 + 1e-6,
                "{v} -> {h:#x} -> {back}"
            );
        }
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0xC000), -2.0);
        assert!(half_to_f32(0x7C00).is_infinite());
        assert!(half_to_f32(0x7E00).is_nan());
        // Smallest subnormal.
        assert_eq!(half_to_f32(0x0001), 2f32.powi(-24));
    }

    #[test]
    fn reads_are_bounds_checked() {
        let data = [1u8, 2, 3];
        let mut s = Stream::new(&data);
        assert_eq!(s.u16().unwrap(), 0x0201);
        assert_eq!(
            s.u32(),
            Err(Error::Truncated {
                offset: 2,
                needed: 3
            })
        );
        assert!(s.count(2, 1).is_err());
        assert_eq!(s.count(1, 1).unwrap(), 1);
    }

    #[test]
    fn matrix_is_read_column_major() {
        let mut bytes = Vec::new();
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let m = Stream::new(&bytes).mat33().unwrap();
        assert_eq!(m.rows[0], [1.0, 4.0, 7.0]);
        assert_eq!(m.rows[1], [2.0, 5.0, 8.0]);
        assert_eq!(m.rows[2], [3.0, 6.0, 9.0]);
    }
}
