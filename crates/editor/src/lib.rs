// SPDX-License-Identifier: GPL-3.0-or-later

//! A load-order-backed plugin editing document with undo and plugin output.

use std::fs;
use std::path::{Path, PathBuf};

use soe_esm::records::navm::{Geometry, NavMesh, Triangle};
use soe_esm::{Entry, GlobalFormId, GroupKind, LoadOrder, Plugin, Record, Subrecord, Tag};
use soe_papyrus::{pex, psc};
use thiserror::Error;

/// Errors produced while opening or editing a plugin document.
#[derive(Debug, Error)]
pub enum EditorError {
    /// A plugin could not be read from disk.
    #[error("could not read plugin {path}: {source}")]
    Io {
        /// Path to the plugin.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A plugin could not be parsed or added to the load order.
    #[error("plugin {name}: {message}")]
    Plugin { name: String, message: String },
    /// The active plugin is not part of the opened load order.
    #[error("active plugin {0} is not in the load order")]
    MissingActivePlugin(String),
    /// The requested form has no winning record.
    #[error("form {0} has no winning record")]
    MissingForm(GlobalFormId),
    /// The active plugin cannot represent this record safely yet.
    #[error("cannot edit form {form}: {reason}")]
    UnsupportedEdit {
        /// Form being edited.
        form: GlobalFormId,
        /// Limitation that prevented the edit.
        reason: &'static str,
    },
    /// A record contains multiple occurrences of the requested subrecord.
    #[error("form {form} has multiple {tag} subrecords")]
    RepeatedSubrecord {
        /// Form being edited.
        form: GlobalFormId,
        /// Subrecord tag that was repeated.
        tag: Tag,
    },
    /// Papyrus compilation or VMAD binding failed.
    #[error("Papyrus script could not be attached: {0}")]
    Papyrus(String),
    /// A NAVM edit would create invalid geometry or indices.
    #[error("cannot edit navmesh {form}: {reason}")]
    InvalidNavmesh {
        /// Navmesh being edited.
        form: GlobalFormId,
        /// Why the requested geometry was rejected.
        reason: &'static str,
    },
    /// Saving the active plugin failed.
    #[error("could not write plugin {path}: {source}")]
    Save {
        /// Destination path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
}

/// An opened load order and its editable active plugin.
pub struct EditorDocument {
    load_order: LoadOrder,
    active_name: String,
    active_index: usize,
    active_plugin: Plugin,
    undo: Vec<Plugin>,
}

impl EditorDocument {
    /// Open plugins in load-order sequence and select the active plugin.
    ///
    /// # Errors
    /// Returns an error if a file cannot be read or parsed, if the load order
    /// is invalid, or if `active_name` is not among the supplied paths.
    pub fn open(paths: &[PathBuf], active_name: &str) -> Result<Self, EditorError> {
        let mut load_order = LoadOrder::new();
        let mut active_index = None;
        for path in paths {
            let name = file_name(path)?;
            let bytes = fs::read(path).map_err(|source| EditorError::Io {
                path: path.clone(),
                source,
            })?;
            let plugin = Plugin::parse(&bytes).map_err(|error| EditorError::Plugin {
                name: name.clone(),
                message: error.to_string(),
            })?;
            let index = load_order
                .add(&name, plugin)
                .map_err(|error| EditorError::Plugin {
                    name: name.clone(),
                    message: error.to_string(),
                })?;
            if name.eq_ignore_ascii_case(active_name) {
                active_index = Some(index);
            }
        }
        let active_index = active_index
            .ok_or_else(|| EditorError::MissingActivePlugin(active_name.to_string()))?;
        let active_name = load_order.file(active_index).name.clone();
        let active_plugin = load_order.file(active_index).plugin.clone();
        Ok(Self {
            load_order,
            active_name,
            active_index,
            active_plugin,
            undo: Vec::new(),
        })
    }

    /// Return the opened load order.
    pub fn load_order(&self) -> &LoadOrder {
        &self.load_order
    }

    /// Search winning forms by optional record signature and editor ID text.
    ///
    /// An empty query matches every winning form that passes the signature
    /// filter. A non-empty query matches an ASCII case-insensitive substring
    /// of the record's `EDID`. The iterator yields form IDs only. Callers can
    /// retrieve the winning record when a result is selected instead of
    /// creating one Godot resource for every form in the load order.
    pub fn search_forms<'a>(
        &'a self,
        tag: Option<Tag>,
        query: &'a str,
    ) -> impl Iterator<Item = GlobalFormId> + 'a {
        self.load_order
            .winners()
            .filter_map(move |(id, record_ref)| {
                let record = self.load_order.record(record_ref);
                if tag.is_some_and(|expected| record.tag != expected) {
                    return None;
                }
                let matches = query.is_empty()
                    || record
                        .find(Tag(*b"EDID"))
                        .is_some_and(|editor_id| contains_ascii_case_insensitive(editor_id, query));
                matches.then_some(id)
            })
    }

    /// Return the active plugin's file name.
    pub fn active_name(&self) -> &str {
        &self.active_name
    }

    /// Return the active plugin being edited.
    pub fn active_plugin(&self) -> &Plugin {
        &self.active_plugin
    }

    /// Replace or append one subrecord, creating an active-plugin override.
    ///
    /// This milestone accepts top-level STAT and GLOB records only. Existing
    /// linked fields are copied as authored, so forms requiring FormID
    /// remapping are rejected until the ESM renumbering work is available.
    ///
    /// # Errors
    /// Returns an error if the form is missing, the winning record is nested,
    /// its owner is not a master of the active plugin, or the field repeats.
    pub fn set_subrecord(
        &mut self,
        form: GlobalFormId,
        tag: Tag,
        data: Vec<u8>,
    ) -> Result<(), EditorError> {
        let existing = self.active_record(form).cloned();
        let (mut record, add_override) = match existing {
            Some(record) => (record, false),
            None => {
                let record_ref = self
                    .load_order
                    .winner_ref(form)
                    .ok_or(EditorError::MissingForm(form))?;
                let source = self.load_order.record(record_ref);
                if !is_top_level(
                    &self.load_order.file(record_ref.file).plugin,
                    record_ref.record,
                ) {
                    return Err(EditorError::UnsupportedEdit {
                        form,
                        reason: "nested record placement needs FMT-15 and FMT-26 FormID remapping",
                    });
                }
                if source.tag != Tag(*b"STAT") && source.tag != Tag(*b"GLOB") {
                    return Err(EditorError::UnsupportedEdit {
                        form,
                        reason: "record reference relocation is not available before FMT-15",
                    });
                }
                let mut record = source.clone();
                record.form_id = self.local_form_id(form)?;
                (record, true)
            }
        };

        let mut matching = record
            .subrecords
            .iter_mut()
            .filter(|subrecord| subrecord.tag == tag);
        if let Some(subrecord) = matching.next() {
            if matching.next().is_some() {
                return Err(EditorError::RepeatedSubrecord { form, tag });
            }
            subrecord.data = data;
        } else {
            record.subrecords.push(Subrecord::new(tag, data));
        }

        self.undo.push(self.active_plugin.clone());
        if add_override {
            self.active_plugin.push_top(record);
        } else if let Some(index) = self.active_record_index(form) {
            self.active_plugin.records[index] = record;
        }
        self.sync_record_count();
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Compile a PSC source and bind its declared auto properties to a record.
    ///
    /// The returned bytes are the Skyrim PEX output. This editor milestone
    /// writes scalar property bindings into an otherwise empty VMAD subrecord.
    ///
    /// # Errors
    /// Returns an error for unsupported source, invalid ownership, records
    /// without an existing active-plugin override, or records with VMAD data.
    pub fn compile_script_for_form(
        &mut self,
        form: GlobalFormId,
        source_name: &str,
        source: &str,
    ) -> Result<Vec<u8>, EditorError> {
        let pex_bytes = psc::compile(source_name, source)
            .map_err(|error| EditorError::Papyrus(error.to_string()))?;
        let script =
            pex::parse(&pex_bytes).map_err(|error| EditorError::Papyrus(error.to_string()))?;
        let object = script
            .objects
            .first()
            .ok_or_else(|| EditorError::Papyrus("compiled PEX has no object".into()))?;
        let vmad = encode_vmad(object)?;

        let record = self
            .load_order
            .winner(form)
            .ok_or(EditorError::MissingForm(form))?;
        if record.find(Tag(*b"VMAD")).is_some() {
            return Err(EditorError::Papyrus(
                "record already has VMAD data, preserving existing bindings is not supported"
                    .into(),
            ));
        }
        self.set_subrecord(form, Tag(*b"VMAD"), vmad)?;
        Ok(pex_bytes)
    }

    /// Compile and bind a script, then save the edited plugin.
    ///
    /// Returns the compiled PEX bytes and the plugin parsed from the written
    /// output.
    ///
    /// # Errors
    /// Returns an error if compilation, binding, or plugin output fails.
    pub fn save_with_script(
        &mut self,
        form: GlobalFormId,
        source_name: &str,
        source: &str,
        plugin_path: impl AsRef<Path>,
    ) -> Result<(Vec<u8>, Plugin), EditorError> {
        let pex_bytes = self.compile_script_for_form(form, source_name, source)?;
        let plugin = self.save(plugin_path)?;
        Ok((pex_bytes, plugin))
    }

    /// Undo the most recent edit, returning whether an edit was available.
    ///
    /// # Errors
    /// Returns an error if the restored plugin no longer forms a valid load order.
    pub fn undo(&mut self) -> Result<bool, EditorError> {
        let Some(previous) = self.undo.pop() else {
            return Ok(false);
        };
        self.active_plugin = previous;
        self.rebuild_load_order()?;
        Ok(true)
    }

    /// Add a triangle using three existing vertex indices.
    ///
    /// Triangle adjacency and the spatial grid are rebuilt before the record
    /// is changed. The edit is rejected if it would exceed NVNM index limits
    /// or create ambiguous edge adjacency.
    pub fn add_navmesh_triangle(
        &mut self,
        form: GlobalFormId,
        vertices: [u16; 3],
        flags: u16,
        cover_flags: u16,
    ) -> Result<(), EditorError> {
        self.edit_navmesh(form, |geometry| {
            if geometry.triangles.len() >= i16::MAX as usize {
                return Err("triangle count exceeds NVNM signed index capacity");
            }
            geometry.triangles.push(Triangle {
                vertices,
                edges: [-1; 3],
                flags: flags & !0x7,
                cover_flags,
            });
            Ok(())
        })
    }

    /// Delete a triangle and remove or remap its triangle-indexed metadata.
    pub fn delete_navmesh_triangle(
        &mut self,
        form: GlobalFormId,
        triangle: usize,
    ) -> Result<(), EditorError> {
        self.edit_navmesh(form, |geometry| {
            if triangle >= geometry.triangles.len() {
                return Err("triangle index is out of range");
            }
            geometry.triangles.remove(triangle);
            for tri in &mut geometry.triangles {
                for edge in 0..3 {
                    if tri.flags & (1 << edge) != 0 {
                        continue;
                    }
                    match usize::try_from(tri.edges[edge]) {
                        Ok(target) if target == triangle => tri.edges[edge] = -1,
                        Ok(target) if target > triangle => tri.edges[edge] -= 1,
                        _ => {}
                    }
                }
            }
            geometry
                .door_links
                .retain_mut(|link| match usize::try_from(link.triangle) {
                    Ok(target) if target == triangle => false,
                    Ok(target) if target > triangle => {
                        link.triangle -= 1;
                        true
                    }
                    _ => true,
                });
            geometry
                .cover_triangles
                .retain_mut(|target| match usize::try_from(*target) {
                    Ok(index) if index == triangle => false,
                    Ok(index) if index > triangle => {
                        *target -= 1;
                        true
                    }
                    _ => true,
                });
            Ok(())
        })
    }

    /// Move one vertex and rebuild the navmesh bounds and spatial grid.
    pub fn move_navmesh_vertex(
        &mut self,
        form: GlobalFormId,
        vertex: usize,
        position: [f32; 3],
    ) -> Result<(), EditorError> {
        self.edit_navmesh(form, |geometry| {
            let Some(target) = geometry.vertices.get_mut(vertex) else {
                return Err("vertex index is out of range");
            };
            *target = position;
            Ok(())
        })
    }

    fn edit_navmesh(
        &mut self,
        form: GlobalFormId,
        edit: impl FnOnce(&mut Geometry) -> Result<(), &'static str>,
    ) -> Result<(), EditorError> {
        let Some(index) = self.active_record_index(form) else {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "navmesh editing requires a NAVM record in the active plugin",
            });
        };
        let record = &self.active_plugin.records[index];
        if record.tag != Tag(*b"NAVM") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "form is not a NAVM record",
            });
        }
        let view = NavMesh::from_record(record).map_err(|_| EditorError::InvalidNavmesh {
            form,
            reason: "NVNM geometry could not be parsed",
        })?;
        let Some(mut geometry) = view.geometry else {
            return Err(EditorError::InvalidNavmesh {
                form,
                reason: "NAVM has no NVNM geometry",
            });
        };
        edit(&mut geometry).map_err(|reason| EditorError::InvalidNavmesh { form, reason })?;
        normalize_geometry(&mut geometry)
            .map_err(|reason| EditorError::InvalidNavmesh { form, reason })?;

        let mut updated = record.clone();
        let mut nvnm = updated
            .subrecords
            .iter_mut()
            .filter(|subrecord| subrecord.tag == Tag(*b"NVNM"));
        let Some(subrecord) = nvnm.next() else {
            return Err(EditorError::InvalidNavmesh {
                form,
                reason: "NAVM has no NVNM geometry",
            });
        };
        if nvnm.next().is_some() {
            return Err(EditorError::RepeatedSubrecord {
                form,
                tag: Tag(*b"NVNM"),
            });
        }
        subrecord.data = geometry.to_bytes();

        self.undo.push(self.active_plugin.clone());
        self.active_plugin.records[index] = updated;
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Write the active plugin to disk and parse the written bytes again.
    ///
    /// # Errors
    /// Returns an error if the destination cannot be written or the produced
    /// plugin bytes cannot be parsed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<Plugin, EditorError> {
        let path = path.as_ref();
        let bytes = self.active_plugin.to_bytes();
        fs::write(path, &bytes).map_err(|source| EditorError::Save {
            path: path.to_path_buf(),
            source,
        })?;
        Plugin::parse(&bytes).map_err(|error| EditorError::Plugin {
            name: path.display().to_string(),
            message: error.to_string(),
        })
    }

    fn active_record(&self, form: GlobalFormId) -> Option<&Record> {
        let index = self.active_record_index(form)?;
        self.active_plugin.records.get(index)
    }

    fn active_record_index(&self, form: GlobalFormId) -> Option<usize> {
        self.active_plugin
            .records
            .iter()
            .position(|record| self.load_order.resolve(self.active_index, record.form_id) == form)
    }

    fn local_form_id(&self, form: GlobalFormId) -> Result<u32, EditorError> {
        if usize::from(form.file) >= self.load_order.files().len() {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the form owner is outside this load order",
            });
        }
        if form.file as usize == self.active_index {
            return Ok(((self.active_plugin.header.masters.len() as u32) << 24) | form.object);
        }
        let master = self
            .active_plugin
            .header
            .masters
            .iter()
            .position(|name| {
                self.load_order
                    .file(form.file as usize)
                    .name
                    .eq_ignore_ascii_case(name)
            })
            .ok_or(EditorError::UnsupportedEdit {
                form,
                reason: "the active plugin does not list the owning plugin as a master",
            })?;
        Ok(((master as u32) << 24) | form.object)
    }

    fn rebuild_load_order(&mut self) -> Result<(), EditorError> {
        let mut rebuilt = LoadOrder::new();
        for (index, file) in self.load_order.files().iter().enumerate() {
            let plugin = if index == self.active_index {
                self.active_plugin.clone()
            } else {
                file.plugin.clone()
            };
            rebuilt
                .add(&file.name, plugin)
                .map_err(|error| EditorError::Plugin {
                    name: file.name.clone(),
                    message: error.to_string(),
                })?;
        }
        self.load_order = rebuilt;
        Ok(())
    }

    fn sync_record_count(&mut self) {
        let count = self.active_plugin.records.len() as u32;
        self.active_plugin.header.record_count = count;
        if let Some(hedr) = self
            .active_plugin
            .header_record
            .subrecords
            .iter_mut()
            .find(|subrecord| subrecord.tag == Tag(*b"HEDR"))
        {
            hedr.data[4..8].copy_from_slice(&count.to_le_bytes());
        }
    }
}

fn contains_ascii_case_insensitive(value: &[u8], query: &str) -> bool {
    let value = value.strip_suffix(&[0]).unwrap_or(value);
    value
        .windows(query.len())
        .any(|window| window.eq_ignore_ascii_case(query.as_bytes()))
}

const EDGE_LINK_BITS: [u16; 3] = [1, 2, 4];

fn normalize_geometry(geometry: &mut Geometry) -> Result<(), &'static str> {
    if geometry.vertices.is_empty() || geometry.vertices.len() > u16::MAX as usize + 1 {
        return Err("vertex count exceeds NVNM unsigned index capacity");
    }
    if geometry.triangles.len() > i16::MAX as usize {
        return Err("triangle count exceeds NVNM signed index capacity");
    }
    if geometry
        .vertices
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err("vertex coordinates must be finite");
    }
    for triangle in &geometry.triangles {
        let [a, b, c] = triangle.vertices.map(usize::from);
        if a >= geometry.vertices.len()
            || b >= geometry.vertices.len()
            || c >= geometry.vertices.len()
        {
            return Err("triangle references a missing vertex");
        }
        let [a, b, c] = [
            geometry.vertices[a],
            geometry.vertices[b],
            geometry.vertices[c],
        ];
        let cross = [
            (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
            (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]),
        ];
        if cross
            .iter()
            .all(|component| component.abs() <= f32::EPSILON)
        {
            return Err("triangle vertices must not be collinear");
        }
    }

    let mut edge_owners = std::collections::BTreeMap::<(u16, u16), Vec<(usize, usize)>>::new();
    for (triangle_index, triangle) in geometry.triangles.iter().enumerate() {
        for edge in 0..3 {
            let a = triangle.vertices[edge];
            let b = triangle.vertices[(edge + 1) % 3];
            edge_owners
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((triangle_index, edge));
        }
    }
    for owners in edge_owners.values() {
        if owners.len() > 2 {
            return Err("more than two triangles share an edge");
        }
    }
    for (owner_index, triangle) in geometry.triangles.iter_mut().enumerate() {
        for (edge, &link_bit) in EDGE_LINK_BITS.iter().enumerate() {
            if triangle.flags & link_bit == 0 {
                triangle.edges[edge] = -1;
            }
            let neighbor = edge_owners
                .get(&{
                    let a = triangle.vertices[edge];
                    let b = triangle.vertices[(edge + 1) % 3];
                    (a.min(b), a.max(b))
                })
                .and_then(|owners| {
                    owners
                        .iter()
                        .find(|(other_index, _)| *other_index != owner_index)
                });
            if let Some((other_index, _)) = neighbor {
                triangle.edges[edge] =
                    i16::try_from(*other_index).map_err(|_| "triangle index overflow")?;
                triangle.flags &= !link_bit;
            }
        }
    }
    rebuild_grid(geometry)?;
    validate_metadata(geometry)?;
    Ok(())
}

fn rebuild_grid(geometry: &mut Geometry) -> Result<(), &'static str> {
    let divisor = geometry.grid.divisor;
    let cell_count = usize::try_from(divisor)
        .ok()
        .and_then(|side| side.checked_mul(side))
        .filter(|count| *count > 0 && *count == geometry.grid.cells.len())
        .ok_or("navmesh grid divisor does not match its cells")?;
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for vertex in &geometry.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let width = (max[0] - min[0]).max(1.0);
    let height = (max[1] - min[1]).max(1.0);
    let side = divisor as usize;
    let mut cells = vec![Vec::new(); cell_count];
    for (triangle_index, triangle) in geometry.triangles.iter().enumerate() {
        let points = triangle
            .vertices
            .map(|index| geometry.vertices[usize::from(index)]);
        let x0 = points
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let x1 = points
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let y0 = points
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let y1 = points
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let cell_x = |value: f32| {
            (((value - min[0]) / width * divisor as f32).floor() as usize).min(side - 1)
        };
        let cell_y = |value: f32| {
            (((value - min[1]) / height * divisor as f32).floor() as usize).min(side - 1)
        };
        for y in cell_y(y0)..=cell_y(y1) {
            for x in cell_x(x0)..=cell_x(x1) {
                cells[y * side + x]
                    .push(i16::try_from(triangle_index).map_err(|_| "triangle index overflow")?);
            }
        }
    }
    geometry.grid.size = [width / divisor as f32, height / divisor as f32];
    geometry.grid.min = min;
    geometry.grid.max = max;
    geometry.grid.cells = cells;
    Ok(())
}

fn validate_metadata(geometry: &Geometry) -> Result<(), &'static str> {
    let count = geometry.triangles.len();
    for triangle in &geometry.triangles {
        for (edge, &link_bit) in EDGE_LINK_BITS.iter().enumerate() {
            let raw = triangle.edges[edge];
            if triangle.flags & link_bit != 0 {
                if usize::try_from(raw)
                    .ok()
                    .is_none_or(|index| index >= geometry.edge_links.len())
                {
                    return Err("triangle edge references a missing edge link");
                }
            } else if raw < -1 || usize::try_from(raw).is_ok_and(|index| index >= count) {
                return Err("triangle edge references a missing triangle");
            }
        }
    }
    if geometry
        .door_links
        .iter()
        .any(|link| usize::try_from(link.triangle).map_or(true, |index| index >= count))
        || geometry
            .cover_triangles
            .iter()
            .any(|index| usize::try_from(*index).map_or(true, |index| index >= count))
        || geometry
            .grid
            .cells
            .iter()
            .flatten()
            .any(|index| usize::try_from(*index).map_or(true, |index| index >= count))
    {
        return Err("triangle metadata references a missing triangle");
    }
    Ok(())
}

fn file_name(path: &Path) -> Result<String, EditorError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| EditorError::Plugin {
            name: path.display().to_string(),
            message: "plugin path has no UTF-8 file name".to_string(),
        })
}

fn encode_vmad(object: &pex::Object) -> Result<Vec<u8>, EditorError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&5u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    write_vmad_string(&mut bytes, &object.name)?;
    bytes.push(0);
    let property_count = u16::try_from(object.properties.len())
        .map_err(|_| EditorError::Papyrus("too many script properties".into()))?;
    bytes.extend_from_slice(&property_count.to_le_bytes());
    for property in &object.properties {
        write_vmad_string(&mut bytes, &property.name)?;
        let property_type = if property.type_name.eq_ignore_ascii_case("string") {
            2
        } else if property.type_name.eq_ignore_ascii_case("int") {
            3
        } else if property.type_name.eq_ignore_ascii_case("float") {
            4
        } else if property.type_name.eq_ignore_ascii_case("bool") {
            5
        } else {
            1
        };
        bytes.push(property_type);
        bytes.push(1);
        match property_type {
            1 => bytes.extend_from_slice(&[0; 8]),
            2 => write_vmad_string(&mut bytes, "")?,
            3 => bytes.extend_from_slice(&0i32.to_le_bytes()),
            4 => bytes.extend_from_slice(&0f32.to_le_bytes()),
            5 => bytes.push(0),
            _ => return Err(EditorError::Papyrus("unsupported property type".into())),
        }
    }
    Ok(bytes)
}

fn write_vmad_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), EditorError> {
    let utf16: Vec<u16> = value.encode_utf16().collect();
    let length = u16::try_from(utf16.len())
        .map_err(|_| EditorError::Papyrus("VMAD string exceeds 65535 UTF-16 units".into()))?;
    bytes.extend_from_slice(&length.to_le_bytes());
    for unit in utf16 {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(())
}

fn is_top_level(plugin: &Plugin, record_index: usize) -> bool {
    plugin.groups.iter().any(|group| {
        matches!(group.kind, GroupKind::Top(_))
            && group
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Record(index) if *index == record_index))
    })
}
