// SPDX-License-Identifier: GPL-3.0-or-later

//! The uncompressed block at the front of every save: the magic, the fields
//! the load menu shows, and the screenshot.
//!
//! `headerSize` covers the fields from `version` through `compressionType`
//! only. The magic and the size field itself come before it, the screenshot
//! comes after it, and the compression lengths come after the screenshot. Read
//! it as covering the screenshot and the body starts in the wrong place, which
//! surfaces much later as a garbage location table. ReSaver guards the same
//! span with a size check, reproduced here as [`Error::HeaderSize`].

use crate::cursor::{Cursor, WString};
use crate::error::{Error, Result};

/// The lowest `version` a Special Edition save carries. Legendary Edition
/// stops at 9, so the two are told apart by this field alone.
pub const SE_MIN_VERSION: u32 = 12;

/// Bytes per screenshot pixel. Special Edition writes RGBA, Legendary RGB.
const SE_BYTES_PER_PIXEL: usize = 4;

pub const MAGIC: &[u8; 13] = b"TESV_SAVEGAME";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Compression {
    None,
    Zlib,
    Lz4,
}

impl Compression {
    fn from_code(code: u16) -> Result<Self> {
        match code {
            0 => Ok(Compression::None),
            1 => Ok(Compression::Zlib),
            2 => Ok(Compression::Lz4),
            other => Err(Error::UnknownCompression { found: other }),
        }
    }

    pub fn code(self) -> u16 {
        match self {
            Compression::None => 0,
            Compression::Zlib => 1,
            Compression::Lz4 => 2,
        }
    }

    pub fn is_compressed(self) -> bool {
        self != Compression::None
    }
}

#[derive(Clone, Debug)]
pub struct Header {
    pub version: u32,
    pub save_number: u32,
    pub player_name: WString,
    pub player_level: u32,
    pub player_location: WString,
    pub game_date: WString,
    pub player_race: WString,
    /// 0 is male, 1 is female.
    pub player_sex: u16,
    pub current_experience: f32,
    pub needed_experience: f32,
    /// Windows FILETIME, 100 nanosecond ticks since 1601.
    pub filetime: u64,
    pub screenshot_width: u32,
    pub screenshot_height: u32,
    pub compression: Compression,
    pub screenshot: Vec<u8>,
}

impl Header {
    /// Reads the header block. On success the cursor sits at the first byte
    /// after the screenshot, which is where the body begins.
    pub fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        let magic = cursor.take(MAGIC.len())?;
        if magic != MAGIC {
            let (found, _, _) = encoding_rs::WINDOWS_1252.decode(magic);
            return Err(Error::NotASave {
                found: found.into_owned(),
            });
        }

        let declared = cursor.u32()?;
        let fields_start = cursor.pos();

        let version = cursor.u32()?;
        if version < SE_MIN_VERSION {
            return Err(Error::NotSpecialEdition { version });
        }

        let save_number = cursor.u32()?;
        let player_name = cursor.wstring()?;
        let player_level = cursor.u32()?;
        let player_location = cursor.wstring()?;
        let game_date = cursor.wstring()?;
        let player_race = cursor.wstring()?;
        let player_sex = cursor.u16()?;
        let current_experience = cursor.f32()?;
        let needed_experience = cursor.f32()?;
        let filetime = cursor.u64()?;
        let screenshot_width = cursor.u32()?;
        let screenshot_height = cursor.u32()?;
        let compression = Compression::from_code(cursor.u16()?)?;

        let actual = cursor.pos() - fields_start;
        if actual != declared as usize {
            return Err(Error::HeaderSize { declared, actual });
        }

        let pixels = screenshot_width as usize * screenshot_height as usize;
        let screenshot = cursor.take(pixels * SE_BYTES_PER_PIXEL)?.to_vec();

        Ok(Header {
            version,
            save_number,
            player_name,
            player_level,
            player_location,
            game_date,
            player_race,
            player_sex,
            current_experience,
            needed_experience,
            filetime,
            screenshot_width,
            screenshot_height,
            compression,
            screenshot,
        })
    }

    /// The byte length of the fields `headerSize` covers.
    fn fields_len(&self) -> usize {
        4 // version
            + 4 // save number
            + 2 + self.player_name.bytes().len()
            + 4 // level
            + 2 + self.player_location.bytes().len()
            + 2 + self.game_date.bytes().len()
            + 2 + self.player_race.bytes().len()
            + 2 // sex
            + 4 // current experience
            + 4 // needed experience
            + 8 // filetime
            + 4 // screenshot width
            + 4 // screenshot height
            + 2 // compression
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(self.fields_len() as u32).to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.save_number.to_le_bytes());
        self.player_name.write(out);
        out.extend_from_slice(&self.player_level.to_le_bytes());
        self.player_location.write(out);
        self.game_date.write(out);
        self.player_race.write(out);
        out.extend_from_slice(&self.player_sex.to_le_bytes());
        out.extend_from_slice(&self.current_experience.to_le_bytes());
        out.extend_from_slice(&self.needed_experience.to_le_bytes());
        out.extend_from_slice(&self.filetime.to_le_bytes());
        out.extend_from_slice(&self.screenshot_width.to_le_bytes());
        out.extend_from_slice(&self.screenshot_height.to_le_bytes());
        out.extend_from_slice(&self.compression.code().to_le_bytes());
        out.extend_from_slice(&self.screenshot);
    }
}
