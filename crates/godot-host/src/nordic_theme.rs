// SPDX-License-Identifier: GPL-3.0-or-later

//! NORDIC 2.4.1 art provenance follows mods/NORDIC-UI-2.4.1/README.lead.
//! Ruffle's tag length reader and UI-19 converter supply SWF parsing and art.
//! Menu layouts, scripts, fonts, sounds and SkyHUD/third-party patches stay out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sha2::{Digest, Sha256};
use soe_bsa::Archive;
use soe_swf_convert::{Movie, Paint};

/// Extraction errors leave the source install untouched.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid SWF art or converter failure.
    #[error(transparent)]
    Art(#[from] soe_swf_convert::Error),
    /// Reference archive could not be read.
    #[error(transparent)]
    Archive(#[from] soe_bsa::Error),
    /// Source or destination file could not be read or written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Theme manifest could not be encoded.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug)]
struct Definition {
    id: u16,
    code: u16,
    payload: Vec<u8>,
}

struct ArtDefinitions {
    version: u8,
    definitions: Vec<Definition>,
    jpeg_tables: Option<Vec<u8>>,
}

impl Definition {
    fn key(&self) -> Vec<u8> {
        // CONTEXT: Character IDs are local to each movie, not part of the artwork.
        [self.code.to_le_bytes().as_slice(), &self.payload[2..]].concat()
    }

    fn fingerprint(&self) -> String {
        format!("{:x}", Sha256::digest(self.key()))
    }

    fn filename(&self) -> String {
        format!(
            "{}-{}.tres",
            if self.is_shape() { "shape" } else { "bitmap" },
            self.id
        )
    }

    fn is_shape(&self) -> bool {
        matches!(self.code, 2 | 22 | 32 | 83)
    }
}

fn invalid(message: &str) -> Error {
    soe_swf_convert::Error::Invalid(message.into()).into()
}

fn definitions(
    mut bytes: &[u8],
    version: u8,
    depth: usize,
    output: &mut Vec<Definition>,
    jpeg_tables: &mut Option<Vec<u8>>,
) -> Result<(), Error> {
    if depth > 64 {
        return Err(invalid("theme sprite nesting exceeds 64"));
    }
    while !bytes.is_empty() {
        let mut reader = swf::read::Reader::new(bytes, version);
        let (code, length) = reader
            .read_tag_code_and_length()
            .map_err(|e| soe_swf_convert::Error::Swf(e.to_string()))?;
        let payload = reader
            .get_ref()
            .get(..length)
            .ok_or_else(|| invalid("truncated theme tag"))?;
        match code {
            0 => break,
            39 => {
                let inner = payload
                    .get(4..)
                    .ok_or_else(|| invalid("truncated theme sprite"))?;
                definitions(inner, version, depth + 1, output, jpeg_tables)?;
            }
            8 => *jpeg_tables = Some(payload.to_vec()),
            2 | 22 | 32 | 83 | 6 | 20 | 21 | 35 | 36 | 90 => {
                let id = payload
                    .get(..2)
                    .and_then(|b| <[u8; 2]>::try_from(b).ok())
                    .map(u16::from_le_bytes)
                    .ok_or_else(|| invalid("theme art lacks a character ID"))?;
                output.push(Definition {
                    id,
                    code,
                    payload: payload.to_vec(),
                });
            }
            _ => {}
        }
        bytes = &reader.get_ref()[length..];
    }
    Ok(())
}

fn read_definitions(bytes: &[u8]) -> Result<ArtDefinitions, Error> {
    let length = bytes
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes);
    if length.is_none_or(|n| n > 128 * 1024 * 1024) || bytes.len() > 128 * 1024 * 1024 {
        return Err(invalid("theme SWF exceeds 128 MiB or lacks a header"));
    }
    let buffer =
        swf::decompress_swf(bytes).map_err(|e| soe_swf_convert::Error::Swf(e.to_string()))?;
    let mut result = Vec::new();
    let mut jpeg_tables = None;
    definitions(
        &buffer.data,
        buffer.header.version(),
        0,
        &mut result,
        &mut jpeg_tables,
    )?;
    let mut ids = BTreeSet::new();
    if result.iter().any(|definition| !ids.insert(definition.id)) {
        return Err(invalid("theme art redefines a character ID"));
    }
    Ok(ArtDefinitions {
        version: buffer.header.version(),
        definitions: result,
        jpeg_tables,
    })
}

fn raw_tag(code: u16, payload: &[u8], bytes: &mut Vec<u8>) {
    bytes.extend_from_slice(&((code << 6) | 63).to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn art_movie(bytes: &[u8]) -> Result<(Movie, Vec<Definition>), Error> {
    let ArtDefinitions {
        version,
        definitions,
        jpeg_tables,
    } = read_definitions(bytes)?;
    let mut raw = Vec::new();
    if let Some(tables) = jpeg_tables {
        raw_tag(8, &tables, &mut raw);
    }
    for definition in &definitions {
        raw_tag(definition.code, &definition.payload, &mut raw);
    }
    raw.extend_from_slice(&[0, 0]);
    let mut header = swf::Header::default_with_swf_version(version);
    header.num_frames = 0;
    let mut stripped = Vec::new();
    swf::write::write_swf_raw_tags(&header, &raw, &mut stripped)
        .map_err(|e| soe_swf_convert::Error::Swf(e.to_string()))?;
    Ok((soe_swf_convert::convert(&stripped)?, definitions))
}

/// Definitions belonging to SkyUI or vanilla, across all reference movies.
#[derive(Debug, Default)]
pub struct Exclusions {
    keys: BTreeSet<Vec<u8>>,
}

impl Exclusions {
    /// Add every nested shape and bitmap in a reference SWF.
    ///
    /// # Errors
    /// Returns a typed error on malformed tag envelopes.
    pub fn add_movie(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let definitions = read_definitions(bytes)?.definitions;
        self.keys.extend(definitions.iter().map(Definition::key));
        Ok(())
    }

    /// Read all SWF and GFX art from a reference archive.
    ///
    /// # Errors
    /// Returns archive or SWF errors instead of silently omitting a reference.
    pub fn add_archive(&mut self, archive: &Archive) -> Result<(), Error> {
        for path in archive.files() {
            if Path::new(path).extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("swf") || extension.eq_ignore_ascii_case("gfx")
            }) {
                self.add_movie(&archive.read(path)?)?;
            }
        }
        Ok(())
    }
}

/// Filter one movie before writing any assets. A shape needs all its bitmaps.
///
/// # Errors
/// Returns a converter or malformed source error.
pub fn own_art(bytes: &[u8], exclusions: &Exclusions) -> Result<Movie, Error> {
    let (mut movie, definitions) = art_movie(bytes)?;
    let approved: BTreeSet<_> = definitions
        .iter()
        .filter(|d| !exclusions.keys.contains(&d.key()))
        .map(|d| d.id)
        .collect();
    movie.bitmaps.retain(|id, _| approved.contains(id));
    movie.meshes.retain(|id, mesh| {
        approved.contains(id)
            && mesh.surfaces.iter().all(|surface| match surface.paint {
                Paint::Bitmap { id, .. } => movie.bitmaps.contains_key(&id),
                _ => true,
            })
    });
    movie.timelines.clear();
    Ok(movie)
}

/// Extract approved NORDIC art from the user's install with UI-19 conversion.
/// Both reference archives must be supplied. Output contains generated art and
/// provenance only, with no original mod files or SWF scene descriptions.
///
/// # Errors
/// Returns I/O, archive, conversion or manifest errors. A nonempty destination
/// is rejected to prevent stale assets surviving a changed exclusion set.
pub fn extract(
    nordic: &Path,
    skyui: &Archive,
    vanilla: &Archive,
    destination: &Path,
) -> Result<serde_json::Value, Error> {
    if destination.exists() && destination.read_dir()?.next().is_some() {
        return Err(invalid("theme extraction destination must be empty"));
    }
    let mut exclusions = Exclusions::default();
    exclusions.add_archive(skyui)?;
    exclusions.add_archive(vanilla)?;
    let mut elements = BTreeMap::new();
    for path in [
        "interface/skyui/icons_category_psychosteve.swf",
        "interface/skyui/inventorylists.swf",
        "interface/skyui/itemcard.swf",
        "interface/skyui/mapmarkerart.swf",
    ] {
        let bytes = std::fs::read(nordic.join(path))?;
        let movie = own_art(&bytes, &exclusions)?;
        let directory = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("theme source lacks a filename"))?;
        soe_swf_convert::write_art(&movie, &destination.join(directory))?;
        let definitions = read_definitions(&bytes)?.definitions;
        for definition in definitions {
            let bounds = if definition.is_shape() {
                let Some(mesh) = movie.meshes.get(&definition.id) else {
                    continue;
                };
                mesh.bounds
            } else {
                let Some(bitmap) = movie.bitmaps.get(&definition.id) else {
                    continue;
                };
                [0.0, 0.0, f64::from(bitmap.width), f64::from(bitmap.height)]
            };
            elements.insert(
                format!("{directory}/{}", definition.filename()),
                serde_json::json!({"source":path, "character":definition.id,
                    "sha256":definition.fingerprint(), "bounds":bounds}),
            );
        }
    }
    let manifest = serde_json::json!({"version":"2.4.1", "elements":elements});
    std::fs::write(
        destination.join("provenance.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest)
}
