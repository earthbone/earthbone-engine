// SPDX-License-Identifier: GPL-3.0-or-later

//! SWF dictionary and timeline conversion using Ruffle's typed tags.
//! PlaceObject2/3 modification inherits omitted fields at the same depth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use swf::extensions::ReadSwfExt;
use swf::{PlaceObjectAction, Tag};

use crate::bitmap::{self, Bitmap};
use crate::geometry::{self, Mesh, Surface};
use crate::Error;

/// All converted definitions and timeline frames of one winning SWF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Movie {
    /// SWF version, which controls string encoding.
    pub version: u8,
    /// Stage rectangle in pixels.
    pub stage: [f64; 4],
    /// Frames per second.
    pub frame_rate: f64,
    /// Exported and imported linkage names, sorted by name.
    pub symbols: BTreeMap<String, u16>,
    /// Imports, source URL and linkage names.
    pub imports: BTreeMap<String, BTreeMap<String, u16>>,
    /// Mesh definitions keyed by character id.
    pub meshes: BTreeMap<u16, Mesh>,
    /// Fonts keyed by character id.
    pub fonts: BTreeMap<u16, Font>,
    /// Bitmap definitions keyed by character id.
    pub bitmaps: BTreeMap<u16, Bitmap>,
    /// Editable text fields keyed by character id.
    pub text_fields: BTreeMap<u16, TextField>,
    /// Static text records, including glyph indices and advances.
    pub static_text: BTreeMap<u16, serde_json::Value>,
    /// Root timeline at id 0, sprites at their dictionary ids.
    pub timelines: BTreeMap<u16, Timeline>,
    /// Button state display lists.
    pub buttons: BTreeMap<u16, serde_json::Value>,
    /// Number of action blocks, including buttons and clip events.
    pub action_blocks: usize,
    /// Unsupported art tag codes, retained as diagnostics.
    pub unsupported_tags: Vec<u16>,
}

/// A fixed-size Godot font atlas built from the embedded outlines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Font {
    /// Character id.
    pub id: u16,
    /// Font family name.
    pub name: String,
    /// DefineFont2 or DefineFont3.
    pub version: u8,
    /// Bold style.
    pub bold: bool,
    /// Italic style.
    pub italic: bool,
    /// Atlas pixel size, scaled by Godot for requested text sizes.
    pub size: u32,
    /// Ascent, descent and leading at the atlas size.
    pub metrics: [f64; 3],
    /// Glyphs in original SWF index order.
    pub glyphs: Vec<Glyph>,
    /// Kerning by character code.
    pub kerning: Vec<(u16, u16, f64)>,
    /// Straight RGBA atlas pixels.
    pub atlas: Bitmap,
}

/// One glyph's outline triangles, code and atlas placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Glyph {
    /// Character code from the SWF code table.
    pub code: u16,
    /// Horizontal advance at the atlas size.
    pub advance: f64,
    /// Atlas rectangle, x, y, width, height.
    pub atlas_rect: [u32; 4],
    /// Baseline-relative pixel origin.
    pub offset: [f32; 2],
    /// Original outline triangulated at the atlas size.
    pub surfaces: Vec<Surface>,
}

/// Editable text definition, without any AS2 execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextField {
    /// Character id.
    pub id: u16,
    /// Pixel rectangle.
    pub bounds: [f64; 4],
    /// Embedded font character id, when present.
    pub font_id: Option<u16>,
    /// Font class name, when present.
    pub font_class: Option<String>,
    /// Font height in pixels.
    pub height: Option<f64>,
    /// Optional text color.
    pub color: Option<[u8; 4]>,
    /// Maximum input length.
    pub max_length: Option<u16>,
    /// AS2 variable binding name.
    pub variable: String,
    /// Authored initial text.
    pub text: Option<String>,
    /// All authored EditText flags, including multiline, HTML and read-only.
    pub flags: u16,
    /// Alignment, left margin, right margin, indent, leading in pixels.
    pub layout: Option<[f64; 5]>,
}

/// A placed instance at a sorted display depth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    /// SWF character id.
    pub character: u16,
    /// Display depth.
    pub depth: u16,
    /// Instance name.
    pub name: Option<String>,
    /// Transform2D order: a, b, c, d, tx, ty, with translation in pixels.
    pub transform: [f64; 6],
    /// RGBA multipliers and offsets normalized to color channels.
    pub color_transform: [f32; 8],
    /// Last masked depth, when this instance is a mask.
    pub clip_depth: Option<u16>,
    /// Morph ratio.
    pub ratio: Option<u16>,
    /// Authored blend mode.
    pub blend_mode: u8,
    /// Visible state.
    pub visible: bool,
    /// Authored filters, retained as typed-tag diagnostic text.
    pub filters: Vec<String>,
}

/// A complete display list at ShowFrame, including retained instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Zero-based frame number.
    pub index: usize,
    /// Authored labels on this frame.
    pub labels: Vec<String>,
    /// Placed instances ordered by depth.
    pub display_list: Vec<Instance>,
}

/// Sprite or root display timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    /// Character id, 0 for the root movie.
    pub id: u16,
    /// Declared frame count.
    pub declared_frames: u16,
    /// Display lists after each ShowFrame.
    pub frames: Vec<Frame>,
}

fn string(value: &swf::SwfStr, version: u8) -> String {
    value.to_string_lossy(swf::SwfStr::encoding_for_version(version))
}

/// Parse and convert one SWF's art and authored display lists.
///
/// # Errors
/// Returns a typed parser, bitmap, geometry or invalid timeline error.
pub fn convert(bytes: &[u8]) -> Result<Movie, Error> {
    let length = bytes
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes);
    if bytes.len() > 128 * 1024 * 1024 || length.is_none_or(|n| n > 128 * 1024 * 1024) {
        return Err(Error::Invalid(
            "SWF exceeds the 128 MiB input limit or lacks a header".into(),
        ));
    }
    let buffer = swf::decompress_swf(bytes).map_err(|e| Error::Swf(e.to_string()))?;
    let tags = read_tags(&buffer.data, buffer.header.version(), 0)?;
    let mut movie = Movie {
        version: buffer.header.version(),
        stage: geometry::bounds(*buffer.header.stage_size()),
        frame_rate: buffer.header.frame_rate().to_f64(),
        symbols: BTreeMap::new(),
        imports: BTreeMap::new(),
        meshes: BTreeMap::new(),
        fonts: BTreeMap::new(),
        bitmaps: BTreeMap::new(),
        text_fields: BTreeMap::new(),
        static_text: BTreeMap::new(),
        timelines: BTreeMap::new(),
        buttons: BTreeMap::new(),
        action_blocks: 0,
        unsupported_tags: vec![],
    };
    timeline(&tags, 0, buffer.header.num_frames(), &mut movie, 0)?;
    movie.unsupported_tags.sort_unstable();
    movie.unsupported_tags.dedup();
    Ok(movie)
}

fn read_tags(mut bytes: &[u8], version: u8, nesting: usize) -> Result<Vec<Tag<'_>>, Error> {
    if nesting > 64 {
        return Err(Error::Invalid("sprite nesting exceeds 64".into()));
    }
    let mut tags = Vec::new();
    while !bytes.is_empty() {
        let mut reader = swf::read::Reader::new(bytes, version);
        let (code, length) = reader
            .read_tag_code_and_length()
            .map_err(|e| Error::Swf(e.to_string()))?;
        let payload = reader
            .get_ref()
            .get(..length)
            .ok_or_else(|| Error::Swf("truncated tag payload".into()))?;
        if code == 0 {
            break;
        }
        let tag = if code == 39 {
            // CONTEXT: Installed SkyUI sprites omit End. Ruffle bounds sprite streams.
            let mut sprite = swf::read::Reader::new(payload, version);
            let id = sprite.read_u16().map_err(|e| Error::Swf(e.to_string()))?;
            let num_frames = sprite.read_u16().map_err(|e| Error::Swf(e.to_string()))?;
            Tag::DefineSprite(swf::Sprite {
                id,
                num_frames,
                tags: read_tags(sprite.get_ref(), version, nesting + 1)?,
            })
        } else if matches!(code, 26 | 70) {
            let mut place_reader = swf::read::Reader::new(payload, version);
            let place = place_reader
                .read_place_object_2_or_3(if code == 26 { 2 } else { 3 })
                .map_err(|e| Error::Swf(e.to_string()))?;
            if !place_reader.get_ref().is_empty() {
                return Err(Error::Invalid(format!(
                    "PlaceObject tag {code} has trailing payload bytes"
                )));
            }
            Tag::PlaceObject(Box::new(place))
        } else {
            swf::read::Reader::new(bytes, version)
                .read_tag()
                .map_err(|e| Error::Swf(e.to_string()))?
        };
        tags.push(tag);
        bytes = &reader.get_ref()[length..];
    }
    Ok(tags)
}

fn font(source: &swf::Font<'_>, version: u8) -> Result<Font, Error> {
    const SIZE: u32 = 64;
    const WIDTH: u32 = 1024;
    let units = if source.version == 3 {
        20.0 * 1024.0
    } else {
        1024.0
    };
    let scale = SIZE as f32 / units;
    let mut glyphs = vec![];
    let mut rasters = vec![];
    let (mut x, mut y, mut row_height) = (1, 1, 0);
    for glyph in &source.glyphs {
        let (surfaces, raster) = geometry::glyph(&glyph.shape_records, scale)?;
        let (offset, width, height) = raster.as_ref().map_or(([0.0, 0.0], 0, 0), |p| {
            let bounds = p.bounds();
            let origin = [bounds.left().floor(), bounds.top().floor()];
            (
                origin,
                (bounds.right().ceil() - origin[0]).max(0.0) as u32,
                (bounds.bottom().ceil() - origin[1]).max(0.0) as u32,
            )
        });
        if width + 2 > WIDTH || height > 1024 {
            return Err(Error::Invalid("font glyph exceeds atlas limits".into()));
        }
        if x + width + 1 > WIDTH {
            x = 1;
            y += row_height + 2;
            row_height = 0;
        }
        let atlas_rect = [x, y, width, height];
        glyphs.push(Glyph {
            code: glyph.code,
            advance: glyph.advance as f64 * scale as f64,
            atlas_rect,
            offset,
            surfaces,
        });
        rasters.push(raster);
        row_height = row_height.max(height);
        x += width + 2;
    }
    let height = (y + row_height + 1).max(1);
    if height > 8192 {
        return Err(Error::Invalid("font atlas exceeds 8192 rows".into()));
    }
    let mut atlas = tiny_skia::Pixmap::new(WIDTH, height)
        .ok_or_else(|| Error::Invalid("cannot allocate font atlas".into()))?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    for (glyph, path) in glyphs.iter().zip(rasters) {
        if let Some(path) = path {
            atlas.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::EvenOdd,
                tiny_skia::Transform::from_translate(
                    glyph.atlas_rect[0] as f32 - glyph.offset[0],
                    glyph.atlas_rect[1] as f32 - glyph.offset[1],
                ),
                None,
            );
        }
    }
    let mut pixels = atlas.take();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[0] = 255;
        pixel[1] = 255;
        pixel[2] = 255;
    }
    let metrics = source
        .layout
        .as_ref()
        .map_or([SIZE as f64 * 0.8, SIZE as f64 * 0.2, 0.0], |l| {
            [
                l.ascent as f64 * scale as f64,
                l.descent as f64 * scale as f64,
                l.leading as f64 * scale as f64,
            ]
        });
    let kerning = source.layout.as_ref().map_or_else(Vec::new, |l| {
        l.kerning
            .iter()
            .map(|k| {
                (
                    k.left_code,
                    k.right_code,
                    k.adjustment.get() as f64 * scale as f64,
                )
            })
            .collect()
    });
    Ok(Font {
        id: source.id,
        name: string(source.name, version),
        version: source.version,
        bold: source.flags.contains(swf::FontFlag::IS_BOLD),
        italic: source.flags.contains(swf::FontFlag::IS_ITALIC),
        size: SIZE,
        metrics,
        glyphs,
        kerning,
        atlas: Bitmap {
            id: source.id,
            width: WIDTH,
            height,
            rgba: pixels,
        },
    })
}

fn timeline(
    tags: &[Tag<'_>],
    id: u16,
    declared_frames: u16,
    movie: &mut Movie,
    nesting: usize,
) -> Result<(), Error> {
    if nesting > 64 {
        return Err(Error::Invalid("sprite nesting exceeds 64".into()));
    }
    let mut display: BTreeMap<u16, Instance> = BTreeMap::new();
    let mut frames = vec![];
    let mut labels = vec![];
    let jpeg_tables = tags.iter().find_map(|tag| match tag {
        Tag::JpegTables(bytes) => Some(*bytes),
        _ => None,
    });
    let version = movie.version;
    for tag in tags {
        match tag {
            Tag::DefineShape(source) => {
                movie.meshes.insert(source.id, geometry::shape(source)?);
            }
            Tag::DefineFont2(source) => {
                movie.fonts.insert(source.id, font(source, version)?);
            }
            Tag::DefineBits { id, jpeg_data } => {
                movie
                    .bitmaps
                    .insert(*id, bitmap::jpeg(*id, jpeg_data, jpeg_tables, None)?);
            }
            Tag::DefineBitsJpeg2 { id, jpeg_data } => {
                movie
                    .bitmaps
                    .insert(*id, bitmap::jpeg(*id, jpeg_data, None, None)?);
            }
            Tag::DefineBitsJpeg3(source) => {
                movie.bitmaps.insert(
                    source.id,
                    bitmap::jpeg(source.id, source.data, None, Some(source.alpha_data))?,
                );
            }
            Tag::DefineBitsLossless(source) => {
                movie.bitmaps.insert(source.id, bitmap::lossless(source)?);
            }
            Tag::DefineEditText(source) => {
                movie.text_fields.insert(
                    source.id(),
                    TextField {
                        id: source.id(),
                        bounds: geometry::bounds(*source.bounds()),
                        font_id: source.font_id(),
                        font_class: source.font_class().map(|s| string(s, version)),
                        height: source.height().map(swf::Twips::to_pixels),
                        color: source.color().map(geometry::rgba),
                        max_length: source.max_length(),
                        variable: string(source.variable_name(), version),
                        text: source.initial_text().map(|s| string(s, version)),
                        flags: source.flags().bits(),
                        layout: source.layout().map(|l| {
                            [
                                l.align as u8 as f64,
                                l.left_margin.to_pixels(),
                                l.right_margin.to_pixels(),
                                l.indent.to_pixels(),
                                l.leading.to_pixels(),
                            ]
                        }),
                    },
                );
            }
            Tag::DefineText(source) | Tag::DefineText2(source) => {
                movie.static_text.insert(source.id, serde_json::json!({"bounds":geometry::bounds(source.bounds), "transform": geometry::matrix(source.matrix),
                    "records":source.records.iter().map(|r| serde_json::json!({"font_id":r.font_id,"color":r.color.map(geometry::rgba),"x":r.x_offset.map(swf::Twips::to_pixels),"y":r.y_offset.map(swf::Twips::to_pixels),"height":r.height.map(swf::Twips::to_pixels),"glyphs":r.glyphs.iter().map(|g| (g.index,g.advance)).collect::<Vec<_>>()})).collect::<Vec<_>>() }));
            }
            Tag::DefineSprite(sprite) => timeline(
                &sprite.tags,
                sprite.id,
                sprite.num_frames,
                movie,
                nesting + 1,
            )?,
            Tag::ExportAssets(exports) => {
                for asset in exports {
                    movie.symbols.insert(string(asset.name, version), asset.id);
                }
            }
            Tag::ImportAssets { url, imports } => {
                let entries: BTreeMap<_, _> = imports
                    .iter()
                    .map(|a| (string(a.name, version), a.id))
                    .collect();
                movie.symbols.extend(entries.clone());
                movie.imports.insert(string(url, version), entries);
            }
            Tag::SymbolClass(symbols) => {
                for symbol in symbols {
                    movie
                        .symbols
                        .insert(string(symbol.class_name, version), symbol.id);
                }
            }
            Tag::PlaceObject(place) => {
                if let Some(actions) = &place.clip_actions {
                    movie.action_blocks += actions.records.len();
                }
                let mut instance = match place.action {
                    PlaceObjectAction::Place(character) => Instance {
                        character,
                        depth: place.depth,
                        name: None,
                        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                        color_transform: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
                        clip_depth: None,
                        ratio: None,
                        blend_mode: 0,
                        visible: true,
                        filters: vec![],
                    },
                    PlaceObjectAction::Modify | PlaceObjectAction::Replace(_) => {
                        let Some(previous) = display.get(&place.depth).cloned() else {
                            // CONTEXT: Ruffle ignores updates to empty depths during frame playback.
                            continue;
                        };
                        previous
                    }
                };
                if let PlaceObjectAction::Replace(character) = place.action {
                    instance.character = character;
                }
                if let Some(name) = place.name {
                    instance.name = Some(string(name, version));
                }
                if let Some(transform) = place.matrix {
                    instance.transform = geometry::matrix(transform);
                    instance.transform[4] /= 20.0;
                    instance.transform[5] /= 20.0;
                }
                if let Some(color) = place.color_transform {
                    instance.color_transform[..4].copy_from_slice(&color.mult_rgba_normalized());
                    instance.color_transform[4..].copy_from_slice(&color.add_rgba_normalized());
                }
                if let Some(depth) = place.clip_depth {
                    instance.clip_depth = Some(depth);
                }
                if let Some(ratio) = place.ratio {
                    instance.ratio = Some(ratio);
                }
                if let Some(blend) = place.blend_mode {
                    instance.blend_mode = blend as u8;
                }
                if let Some(visible) = place.is_visible {
                    instance.visible = visible;
                }
                if let Some(filters) = &place.filters {
                    instance.filters = filters.iter().map(|f| format!("{f:?}")).collect();
                }
                display.insert(place.depth, instance);
            }
            Tag::RemoveObject(remove) => {
                display.remove(&remove.depth);
            }
            Tag::FrameLabel(label) => labels.push(string(label.label, version)),
            Tag::ShowFrame => frames.push(Frame {
                index: frames.len(),
                labels: std::mem::take(&mut labels),
                display_list: display.values().cloned().collect(),
            }),
            Tag::DoAction(_) | Tag::DoInitAction { .. } => movie.action_blocks += 1,
            Tag::DefineButton(button) | Tag::DefineButton2(button) => {
                movie.action_blocks += button.actions.len();
                movie.buttons.insert(button.id, serde_json::json!({"records":button.records.iter().map(|r| serde_json::json!({"character":r.id,"depth":r.depth,"states":r.states.bits(),"transform":geometry::matrix(r.matrix),"blend_mode":r.blend_mode as u8})).collect::<Vec<_>>() }));
            }
            Tag::DefineMorphShape(_) => movie.unsupported_tags.push(46),
            Tag::DefineFont(_) => movie.unsupported_tags.push(10),
            Tag::DefineFont4(_) => movie.unsupported_tags.push(91),
            Tag::Unknown { tag_code, .. } => movie.unsupported_tags.push(*tag_code),
            _ => {}
        }
    }
    if id != 0 && frames.len() + 1 == declared_frames as usize && !display.is_empty() {
        // CONTEXT: SkyUI icon sprites retain their final display without ShowFrame.
        frames.push(Frame {
            index: frames.len(),
            labels,
            display_list: display.values().cloned().collect(),
        });
    }
    if frames.len() != declared_frames as usize {
        return Err(Error::Invalid(format!(
            "timeline {id} declares {declared_frames} frames but contains {}",
            frames.len()
        )));
    }
    movie.timelines.insert(
        id,
        Timeline {
            id,
            declared_frames,
            frames,
        },
    );
    Ok(())
}
