// SPDX-License-Identifier: GPL-3.0-or-later

//! SWF edge soup to fill and stroke meshes, following Ruffle shape_utils.
//! Lyon handles curve flattening, winding, caps and joins.

use lyon_tessellation::math::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use serde::{Deserialize, Serialize};
use swf::{FillStyle, ShapeRecord};

use crate::Error;

/// SWF paint coordinates and colors, retained for the native material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Paint {
    /// Straight RGBA color channels.
    Solid([u8; 4]),
    /// Gradient matrix, type, spread, interpolation, focal point and stops.
    Gradient {
        /// Local gradient units to SWF twips.
        matrix: [f64; 6],
        /// 0 linear, 1 radial, 2 focal.
        kind: u8,
        /// 0 pad, 1 reflect, 2 repeat.
        spread: u8,
        /// Interpolate in linear RGB when true.
        linear_rgb: bool,
        /// Focal point for focal gradients.
        focal: f64,
        /// Position and RGBA color for each stop.
        stops: Vec<(u8, [u8; 4])>,
    },
    /// A bitmap definition in the same movie dictionary.
    Bitmap {
        /// SWF character id.
        id: u16,
        /// Bitmap pixels to SWF twips.
        matrix: [f64; 6],
        /// Smooth texture sampling.
        smoothed: bool,
        /// Repeat outside the source image.
        repeating: bool,
    },
}

/// One authored fill or stroke, triangulated in pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    /// Pixel coordinates.
    pub vertices: Vec<[f32; 2]>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// Paint used by the triangles.
    pub paint: Paint,
    /// Stroke width in pixels, absent for a fill.
    pub stroke_width: Option<f32>,
    /// Horizontal and vertical stroke scaling flags.
    pub stroke_scaling: [bool; 2],
    /// Pixel hinting flag.
    pub pixel_hinting: bool,
}

/// Ordered fill and stroke surfaces for one shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    /// Character id.
    pub id: u16,
    /// Pixel bounds, x, y, width, height.
    pub bounds: [f64; 4],
    /// Authored layer order, fills before lines within a layer.
    pub surfaces: Vec<Surface>,
}

pub(crate) fn matrix(value: swf::Matrix) -> [f64; 6] {
    [
        value.a.to_f64(),
        value.b.to_f64(),
        value.c.to_f64(),
        value.d.to_f64(),
        value.tx.get() as f64,
        value.ty.get() as f64,
    ]
}

pub(crate) fn rgba(value: swf::Color) -> [u8; 4] {
    [value.r, value.g, value.b, value.a]
}

pub(crate) fn bounds(value: swf::Rectangle<swf::Twips>) -> [f64; 4] {
    [
        value.x_min.to_pixels(),
        value.y_min.to_pixels(),
        (value.x_max.get() as f64 - value.x_min.get() as f64) / 20.0,
        (value.y_max.get() as f64 - value.y_min.get() as f64) / 20.0,
    ]
}

impl From<&FillStyle> for Paint {
    fn from(style: &FillStyle) -> Self {
        match style {
            FillStyle::Color(color) => Self::Solid(rgba(*color)),
            FillStyle::Bitmap {
                id,
                matrix: transform,
                is_smoothed,
                is_repeating,
            } => Self::Bitmap {
                id: *id,
                matrix: matrix(*transform),
                smoothed: *is_smoothed,
                repeating: *is_repeating,
            },
            FillStyle::LinearGradient(g)
            | FillStyle::RadialGradient(g)
            | FillStyle::FocalGradient { gradient: g, .. } => Self::Gradient {
                matrix: matrix(g.matrix),
                kind: match style {
                    FillStyle::LinearGradient(_) => 0,
                    FillStyle::RadialGradient(_) => 1,
                    _ => 2,
                },
                spread: g.spread as u8,
                linear_rgb: g.interpolation == swf::GradientInterpolation::LinearRgb,
                focal: match style {
                    FillStyle::FocalGradient { focal_point, .. } => focal_point.to_f64(),
                    _ => 0.0,
                },
                stops: g
                    .records
                    .iter()
                    .map(|stop| (stop.ratio, rgba(stop.color)))
                    .collect(),
            },
        }
    }
}

type Position = [i64; 2];

#[derive(Clone)]
struct Edge {
    start: Position,
    control: Option<Position>,
    end: Position,
}

impl Edge {
    fn reversed(&self) -> Self {
        Self {
            start: self.end,
            control: self.control,
            end: self.start,
        }
    }
}

fn path(edges: &[Edge], scale: f32, closed: bool) -> Path {
    let mut remaining: Vec<_> = edges.to_vec();
    let mut builder = Path::builder();
    let to_point = |p: Position| point(p[0] as f32 * scale, p[1] as f32 * scale);
    while let Some(first) = remaining.first().cloned() {
        remaining.remove(0);
        let start = first.start;
        let mut cursor = first.end;
        builder.begin(to_point(start));
        let mut edge = first;
        loop {
            if let Some(control) = edge.control {
                builder.quadratic_bezier_to(to_point(control), to_point(edge.end));
            } else {
                builder.line_to(to_point(edge.end));
            }
            if cursor == start {
                break;
            }
            let found = remaining
                .iter()
                .position(|next| next.start == cursor || (!closed && next.end == cursor));
            let Some(index) = found else { break };
            edge = remaining.remove(index);
            if edge.start != cursor {
                edge = edge.reversed();
            }
            cursor = edge.end;
        }
        builder.end(closed || cursor == start);
    }
    builder.build()
}

fn cap(value: swf::LineCapStyle) -> LineCap {
    match value {
        swf::LineCapStyle::Round => LineCap::Round,
        swf::LineCapStyle::Square => LineCap::Square,
        swf::LineCapStyle::None => LineCap::Butt,
    }
}

fn tessellate(
    edges: &[Edge],
    scale: f32,
    fill: FillRule,
    style: Option<&swf::LineStyle>,
    paint: Paint,
) -> Result<Surface, Error> {
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let outline = path(edges, scale, style.is_none());
    if let Some(style) = style {
        let options = StrokeOptions::default()
            .with_line_width((style.width().get() as f32 * scale).max(1.0))
            .with_start_cap(cap(style.start_cap()))
            .with_end_cap(cap(style.end_cap()))
            .with_line_join(match style.join_style() {
                swf::LineJoinStyle::Round => LineJoin::Round,
                swf::LineJoinStyle::Bevel => LineJoin::Bevel,
                swf::LineJoinStyle::Miter(_) => LineJoin::MiterClip,
            })
            .with_miter_limit(match style.join_style() {
                swf::LineJoinStyle::Miter(limit) => (limit.to_f32()).max(1.0),
                _ => 4.0,
            })
            .with_tolerance(0.1);
        StrokeTessellator::new()
            .tessellate_path(
                &outline,
                &options,
                &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
                    vertex.position().to_array()
                }),
            )
            .map_err(|e| Error::Tessellation(e.to_string()))?;
    } else {
        FillTessellator::new()
            .tessellate_path(
                &outline,
                &FillOptions::default()
                    .with_fill_rule(fill)
                    .with_tolerance(0.1),
                &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| {
                    vertex.position().to_array()
                }),
            )
            .map_err(|e| Error::Tessellation(e.to_string()))?;
    }
    Ok(Surface {
        vertices: buffers.vertices,
        indices: buffers.indices,
        paint,
        stroke_width: style.map(|s| s.width().get() as f32 * scale),
        stroke_scaling: style.map_or([true, true], |s| [s.allow_scale_x(), s.allow_scale_y()]),
        pixel_hinting: style.is_some_and(swf::LineStyle::is_pixel_hinted),
    })
}

struct Layer<'a> {
    styles: &'a swf::ShapeStyles,
    fills: Vec<Vec<Edge>>,
    lines: Vec<Vec<Edge>>,
}

impl<'a> Layer<'a> {
    fn new(styles: &'a swf::ShapeStyles) -> Self {
        Self {
            styles,
            fills: vec![vec![]; styles.fill_styles.len()],
            lines: vec![vec![]; styles.line_styles.len()],
        }
    }

    fn finish(&self, surfaces: &mut Vec<Surface>, fill: FillRule, scale: f32) -> Result<(), Error> {
        for (edges, style) in self.fills.iter().zip(&self.styles.fill_styles) {
            if !edges.is_empty() {
                surfaces.push(tessellate(edges, scale, fill, None, Paint::from(style))?);
            }
        }
        for (edges, style) in self.lines.iter().zip(&self.styles.line_styles) {
            if !edges.is_empty() {
                surfaces.push(tessellate(
                    edges,
                    scale,
                    fill,
                    Some(style),
                    Paint::from(style.fill_style()),
                )?);
            }
        }
        Ok(())
    }
}

pub(crate) fn shape(shape: &swf::Shape) -> Result<Mesh, Error> {
    let fill = if shape.flags.contains(swf::ShapeFlag::NON_ZERO_WINDING_RULE) {
        FillRule::NonZero
    } else {
        FillRule::EvenOdd
    };
    let surfaces = records(&shape.shape, &shape.styles, fill, 1.0 / 20.0)?;
    Ok(Mesh {
        id: shape.id,
        bounds: bounds(shape.shape_bounds),
        surfaces,
    })
}

fn records(
    records: &[ShapeRecord],
    styles: &swf::ShapeStyles,
    fill: FillRule,
    scale: f32,
) -> Result<Vec<Surface>, Error> {
    let mut layer = Layer::new(styles);
    let mut surfaces = vec![];
    let mut cursor = [0_i64; 2];
    let (mut fill0, mut fill1, mut line) = (0_usize, 0_usize, 0_usize);
    for record in records {
        let edge = match record {
            ShapeRecord::StyleChange(change) => {
                if let Some(styles) = &change.new_styles {
                    layer.finish(&mut surfaces, fill, scale)?;
                    layer = Layer::new(styles);
                    fill0 = 0;
                    fill1 = 0;
                    line = 0;
                }
                if let Some(p) = change.move_to {
                    cursor = [p.x.get() as i64, p.y.get() as i64];
                }
                if let Some(id) = change.fill_style_0 {
                    fill0 = id as usize;
                }
                if let Some(id) = change.fill_style_1 {
                    fill1 = id as usize;
                }
                if let Some(id) = change.line_style {
                    line = id as usize;
                }
                if fill0 > layer.fills.len()
                    || fill1 > layer.fills.len()
                    || line > layer.lines.len()
                {
                    return Err(Error::Invalid("shape style index exceeds its table".into()));
                }
                continue;
            }
            ShapeRecord::StraightEdge { delta } => Edge {
                start: cursor,
                control: None,
                end: [
                    cursor[0] + delta.dx.get() as i64,
                    cursor[1] + delta.dy.get() as i64,
                ],
            },
            ShapeRecord::CurvedEdge {
                control_delta,
                anchor_delta,
            } => {
                let control = [
                    cursor[0] + control_delta.dx.get() as i64,
                    cursor[1] + control_delta.dy.get() as i64,
                ];
                Edge {
                    start: cursor,
                    control: Some(control),
                    end: [
                        control[0] + anchor_delta.dx.get() as i64,
                        control[1] + anchor_delta.dy.get() as i64,
                    ],
                }
            }
        };
        cursor = edge.end;
        if fill0 > 0 {
            layer.fills[fill0 - 1].push(edge.reversed());
        }
        if fill1 > 0 {
            layer.fills[fill1 - 1].push(edge.clone());
        }
        if line > 0 {
            layer.lines[line - 1].push(edge);
        }
    }
    layer.finish(&mut surfaces, fill, scale)?;
    Ok(surfaces)
}

pub(crate) fn glyph(
    records: &[ShapeRecord],
    scale: f32,
) -> Result<(Vec<Surface>, Option<tiny_skia::Path>), Error> {
    let styles = swf::ShapeStyles {
        fill_styles: vec![FillStyle::Color(swf::Color::WHITE)],
        line_styles: vec![],
    };
    let surfaces = self::records(records, &styles, FillRule::EvenOdd, scale)?;
    let mut raster = tiny_skia::PathBuilder::new();
    // CONTEXT: Lyon's path assembly supplies the same contour directions to the atlas rasterizer.
    let mut cursor = [0_i64; 2];
    let mut edges = vec![];
    for record in records {
        match record {
            ShapeRecord::StyleChange(change) => {
                if let Some(p) = change.move_to {
                    cursor = [p.x.get() as i64, p.y.get() as i64];
                }
            }
            ShapeRecord::StraightEdge { delta } => {
                let end = [
                    cursor[0] + delta.dx.get() as i64,
                    cursor[1] + delta.dy.get() as i64,
                ];
                edges.push(Edge {
                    start: cursor,
                    control: None,
                    end,
                });
                cursor = end;
            }
            ShapeRecord::CurvedEdge {
                control_delta,
                anchor_delta,
            } => {
                let control = [
                    cursor[0] + control_delta.dx.get() as i64,
                    cursor[1] + control_delta.dy.get() as i64,
                ];
                let end = [
                    control[0] + anchor_delta.dx.get() as i64,
                    control[1] + anchor_delta.dy.get() as i64,
                ];
                edges.push(Edge {
                    start: cursor,
                    control: Some(control),
                    end,
                });
                cursor = end;
            }
        }
    }
    for event in path(&edges, scale, true).iter() {
        match event {
            lyon_tessellation::path::Event::Begin { at } => raster.move_to(at.x, at.y),
            lyon_tessellation::path::Event::Line { to, .. } => raster.line_to(to.x, to.y),
            lyon_tessellation::path::Event::Quadratic { ctrl, to, .. } => {
                raster.quad_to(ctrl.x, ctrl.y, to.x, to.y);
            }
            lyon_tessellation::path::Event::Cubic {
                ctrl1, ctrl2, to, ..
            } => raster.cubic_to(ctrl1.x, ctrl1.y, ctrl2.x, ctrl2.y, to.x, to.y),
            lyon_tessellation::path::Event::End { .. } => raster.close(),
        }
    }
    Ok((surfaces, raster.finish()))
}
