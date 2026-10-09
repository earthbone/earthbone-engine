// SPDX-License-Identifier: GPL-3.0-or-later

//! RND-32 surveys xEdit's placed model fields and REFR enable flags/XESP.

use std::collections::BTreeSet;

use super::*;
use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, Plugin};

const KINDS: [&[u8; 4]; 17] = [
    b"TREE", b"FLOR", b"LIGH", b"MSTT", b"SCOL", b"TACT", b"MISC", b"WEAP", b"ARMO", b"BOOK",
    b"INGR", b"ALCH", b"KEYM", b"SLGM", b"AMMO", b"SCRL", b"APPA",
];

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(object: u32, base: u32) -> Record {
    Record::new(tag::REFR, object)
        .sub(b"NAME", base.to_le_bytes())
        .sub(b"DATA", [0; 24])
}

fn parent(record: Record, object: u32, opposite: u32) -> Record {
    record.sub(
        b"XESP",
        [object.to_le_bytes(), opposite.to_le_bytes()].concat(),
    )
}

fn fixture() -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("meshes")).unwrap();
    let mut writer = super::fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let textures = writer.texture_set(&[]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader(0, 0, 0, textures, &[]);
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (vertices, triangles) = super::fixture::quad();
    let shape = writer.bs_tri_shape(
        "Quad",
        [0.0; 3],
        -1,
        shader,
        -1,
        soe_nif::vertex::attr::VERTEX,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    let collision = box_collision(&mut writer, root);
    let mut node = super::fixture::Buf::default();
    writer.av_object(
        &mut node,
        "Root",
        [0.0; 3],
        super::fixture::IDENTITY,
        1.0,
        collision,
        -1,
    );
    node.refs(&[shape]).u32(0);
    writer.fill(root, node.0);
    writer.roots = vec![root];
    std::fs::write(directory.path().join("meshes/quad.nif"), writer.finish()).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut references = Vec::new();
    for (index, kind) in KINDS.into_iter().enumerate() {
        let base = 0x800 + index as u32;
        plugin.push_top(
            Record::new(Tag::new(kind), base)
                .zsub(if kind == b"ARMO" { b"MOD2" } else { b"MODL" }, "quad.nif"),
        );
        references.push(reference(0x900 + index as u32, base));
    }
    plugin.push_top(reference(0xA00, 0x800));
    plugin.push_top(reference(0xA01, 0x800).with_flags(flags::INITIALLY_DISABLED));
    plugin.push_top(
        Record::new(tag::STAT, 0x820)
            .with_flags(1 << 23)
            .zsub(b"MODL", "quad.nif"),
    );
    plugin.push_top(Record::new(Tag::new(b"SOUN"), 0x821));
    plugin.push_top(Record::new(Tag::new(b"MISC"), 0x822).zsub(b"MODL", "missing.nif"));
    references.extend([
        reference(0x920, 0x800).with_flags(flags::INITIALLY_DISABLED),
        parent(
            reference(0x921, 0x800).with_flags(flags::INITIALLY_DISABLED),
            0xA00,
            0,
        ),
        parent(reference(0x922, 0x800), 0xA00, 1),
        parent(reference(0x923, 0x800), 0xA01, 0),
        parent(reference(0x924, 0x800), 0xA01, 1),
        reference(0x925, 0x820),
        reference(0x926, 0x821),
        reference(0x927, 0x822),
        reference(0x928, 0x800).sub(b"XOCP", [0; 16]),
        reference(0x929, 0x800).with_flags(flags::DELETED),
    ]);
    let cell = plugin.push_record(
        Record::new(tag::CELL, 0x8100)
            .zsub(b"EDID", "Rnd32Cell")
            .sub(b"DATA", [1, 0]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(0x8100));
    for reference in references {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(reference)));
    }
    let mut children = Group::new(GroupKind::CellChildren(0x8100));
    children.entries.push(Entry::Group(temporary));
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    cells
        .entries
        .extend([Entry::Record(cell), Entry::Group(children)]);
    plugin.groups.push(cells);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in &BASE_MASTERS[1..] {
        std::fs::write(
            directory.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory.path()).unwrap();
    (directory, source)
}

/// nif.xml bhk layouts, using the existing physics synthetic fixture's SSE body.
fn box_collision(writer: &mut super::fixture::NifWriter, root: i32) -> i32 {
    use super::fixture::Buf;
    let mut shape = Buf::default();
    shape
        .u32(1)
        .f32(0.05)
        .bytes(&[0; 8])
        .floats(&[1.0, 1.0, 1.0, 0.0]);
    let shape = writer.add("bhkBoxShape", shape.0);
    let mut body = Buf::default();
    body.i32(shape).u8(1).u8(0).u16(0).bytes(&[0; 44]);
    body.floats(&[0.0; 4])
        .floats(&[0.0, 0.0, 0.0, 1.0])
        .floats(&[0.0; 8]);
    body.bytes(&[0; 64])
        .floats(&[0.0, 0.1, 0.05, 1.0, 1.0, 0.5, 0.0, 0.4]);
    body.floats(&[104.4, 31.57, 0.15])
        .bytes(&[7, 1, 1, 1, 0, 0, 0, 0]);
    body.bytes(&[0; 12]).u32(0).u16(0);
    let body = writer.add("bhkRigidBodyT", body.0);
    let mut collision = Buf::default();
    collision.i32(root).u16(1).i32(body);
    writer.add("bhkCollisionObject", collision.0)
}

#[test]
fn rnd_32_streams_all_base_types_and_selects_their_collision_models() {
    let (_directory, source) = fixture();
    let cell = source.resolve_cell("Rnd32Cell").unwrap();
    let loaded = source.stream_cell_id(cell).unwrap();
    let drawn: BTreeSet<_> = loaded
        .surfaces
        .iter()
        .map(|surface| surface.reference)
        .collect();
    for (index, kind) in KINDS.into_iter().enumerate() {
        let reference = id(0x900 + index as u32);
        assert!(drawn.contains(&reference), "{kind:?} not built");
        assert_eq!(
            source.stream_reference(reference).unwrap().surfaces.len(),
            1
        );
        let colliders = source.placed_reference_colliders(reference).unwrap();
        assert_eq!(colliders.len(), 1, "{kind:?} lost its collision model");
        assert!(colliders[0].collider.is_static());
    }
    let (_, collision) = source.placed_cell_colliders(cell).unwrap();
    assert_eq!(collision.references, KINDS.len() + 5);
    assert_eq!(collision.failed, 1);
    assert_eq!(
        loaded.stats.containers_with_model, 0,
        "new kinds must not count as containers"
    );
}

#[test]
fn rnd_32_counts_markers_disable_causes_failures_and_outside_parents() {
    let (_directory, source) = fixture();
    let cell = source.resolve_cell("Rnd32Cell").unwrap();
    let loaded = source.stream_cell_id(cell).unwrap();
    let counts = &loaded.stats.reference_counts[&Tag::new(b"TREE")];
    assert_eq!(
        (
            counts.placed,
            counts.built,
            counts.initially_disabled,
            counts.enable_parent,
            counts.markers
        ),
        (8, 3, 1, 2, 1)
    );
    assert_eq!(counts.other.get("deleted"), Some(&1));
    assert_eq!(loaded.stats.reference_counts[&Tag::new(b"STAT")].markers, 1);
    assert_eq!(
        loaded.stats.reference_counts[&Tag::new(b"SOUN")]
            .other
            .get("no placed model"),
        Some(&1)
    );
    assert_eq!(
        loaded.stats.reference_counts[&Tag::new(b"MISC")]
            .other
            .get("mesh failure"),
        Some(&1)
    );
    assert!(source
        .stream_reference(id(0x928))
        .unwrap()
        .surfaces
        .is_empty());
    assert!(source
        .stream_reference(id(0x925))
        .unwrap()
        .surfaces
        .is_empty());
    let mut world = soe_game::World::from_shared_order(source.shared_order());
    world.set_reference_enabled(id(0xA00), false);
    world.set_reference_enabled(id(0x900), false);
    let counts = source.reference_counts(cell, &loaded, Some(&world));
    let trees = &counts[&Tag::new(b"TREE")];
    assert_eq!(
        trees.runtime_disabled, 2,
        "runtime Disable reaches a child outside the parent's cell"
    );
    assert_eq!(trees.enable_parent, 1);
    assert_eq!(
        trees.other.get("deferred"),
        Some(&1),
        "opposite child enabled at runtime awaits preparation"
    );
    assert!(world.reference_enabled(id(0x922)));
    assert!(!world.reference_enabled(id(0x921)));
    for counts in counts.values() {
        assert_eq!(
            counts.placed,
            counts.built
                + counts.initially_disabled
                + counts.enable_parent
                + counts.runtime_disabled
                + counts.markers
                + counts.other.values().sum::<usize>()
        );
    }
}

#[test]
fn rnd_32_installed_cells_account_for_every_placed_reference() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping RND-32 installed cells");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 4).unwrap();
    let mut manifest = Vec::new();
    for name in [
        "WhiterunBanneredMare",
        "WhiterunExterior04",
        "WhiterunExterior13",
        "WhiterunExterior17",
    ] {
        let cell = source.resolve_cell(name).unwrap();
        let loaded = source.stream_cell_id(cell).unwrap();
        let drawn: BTreeSet<_> = loaded
            .surfaces
            .iter()
            .filter(|surface| {
                source
                    .order
                    .winner(surface.reference)
                    .is_some_and(|record| record.tag == tag::REFR)
            })
            .map(|surface| surface.reference)
            .collect();
        println!("RND-32 {name} cell={cell}");
        for (kind, counts) in &loaded.stats.reference_counts {
            println!(
                "{kind} placed={} built={} initial={} parent={} runtime={} markers={} other={:?}",
                counts.placed,
                counts.built,
                counts.initially_disabled,
                counts.enable_parent,
                counts.runtime_disabled,
                counts.markers,
                counts.other
            );
            assert_eq!(
                counts.placed,
                counts.built
                    + counts.initially_disabled
                    + counts.enable_parent
                    + counts.runtime_disabled
                    + counts.markers
                    + counts.other.values().sum::<usize>()
            );
        }
        let mut objects = Vec::new();
        let mut expected = BTreeSet::new();
        for &id in source.cell_index().references(cell) {
            let owner = source.order.winner_ref(id).unwrap();
            let record = source.order.record(owner);
            if record.tag != tag::REFR {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            let base = source
                .order
                .winner(source.order.resolve(owner.file, reference.base))
                .unwrap();
            let enabled = soe_game::World::authored_reference_enabled(&source.order, id);
            assert_eq!(
                enabled,
                vanilla_enabled(&source.order, id, &mut BTreeSet::new()),
                "{name} {id} enable state differs from winning flags/XESP"
            );
            let model = models::render_model_for(base);
            let marker_only = loaded.stats.reference_skips.get(&id) == Some(&"marker model");
            if enabled && model.is_some() && !models::is_editor_reference(record) && !marker_only {
                expected.insert(id);
                assert!(
                    drawn.contains(&id),
                    "{name} {id} {} {model:?} was omitted",
                    base.tag
                );
            }
            if enabled && model.is_none() && !models::is_marker_base(base) {
                assert!(
                    matches!(base.tag.as_bytes(), b"ACTI" | b"IDLM" | b"LIGH" | b"SOUN"),
                    "{name} {id} unexpected model-less type {}",
                    base.tag
                );
            }
            if enabled
                && model.is_some()
                && !models::is_editor_reference(record)
                && !drawn.contains(&id)
            {
                println!(
                    "UNDRAWN {id} type={} edid={:?} model={model:?} reason={:?}",
                    base.tag,
                    base.editor_id(),
                    loaded.stats.reference_skips.get(&id)
                );
            }
            if enabled && matches!(base.tag.as_bytes(), b"TREE" | b"FLOR") && drawn.contains(&id) {
                for surface in loaded
                    .surfaces
                    .iter()
                    .filter(|surface| surface.reference == id)
                {
                    if surface.arrays.material.diffuse.is_some() {
                        let albedo = surface.textures.diffuse.as_ref().unwrap();
                        assert!(
                            !Arc::ptr_eq(albedo, &checker_albedo()),
                            "{name} {id} vegetation albedo did not resolve"
                        );
                    }
                }
                objects.push(serde_json::json!({"file": id.file, "object": id.object,
                    "kind": base.tag.to_string(), "position": reference.transform.unwrap().position}));
            }
        }
        assert_eq!(drawn, expected, "built count must equal model-bearing placed references minus markers and truly disabled references");
        assert_eq!(
            loaded
                .stats
                .reference_counts
                .values()
                .map(|counts| counts.built)
                .sum::<usize>(),
            drawn.len()
        );
        let counts: serde_json::Map<String, serde_json::Value> = loaded.stats.reference_counts.iter()
            .map(|(kind, counts)| (kind.to_string(), serde_json::json!({
                "placed": counts.placed, "built": counts.built,
                "initial": counts.initially_disabled, "parent": counts.enable_parent,
                "runtime": counts.runtime_disabled, "markers": counts.markers, "other": counts.other
            }))).collect();
        manifest.push(serde_json::json!({"cell": name, "id": cell.to_string(), "counts": counts, "objects": objects}));
        assert_eq!(loaded.stats.meshes_failed, 0, "{name} mesh failures");
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("rnd_32_whiterun.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    if let Some(godot) = std::env::var_os("SOE_GODOT_BIN") {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
        let output = std::process::Command::new(godot)
            .arg("--path")
            .arg(project)
            .args([
                "--resolution",
                "640x360",
                "--script",
                "rnd_32_references_test.gd",
            ])
            .output()
            .expect("start RND-32 after building the release extension and importing Godot");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success()
                && stdout
                    .contains("RESULT: PASS rnd_32_windowed_plains_show_placed_trees_and_flora"),
            "RND-32 windowed check failed\n{stdout}\n{stderr}"
        );
        println!("{stdout}");
    }
}

/// xEdit Initially Disabled and one-byte XESP flags, independent of World state.
fn vanilla_enabled(
    order: &LoadOrder,
    reference: GlobalFormId,
    chain: &mut BTreeSet<GlobalFormId>,
) -> bool {
    if !chain.insert(reference) {
        return false;
    }
    let Some(owner) = order.winner_ref(reference) else {
        return false;
    };
    let record = order.record(owner);
    if record.is_deleted() {
        return false;
    }
    if let Some(parent) = record.find(Tag::new(b"XESP")) {
        if parent.len() != 8 {
            return false;
        }
        let raw = u32::from_le_bytes(parent[..4].try_into().unwrap());
        if raw == 0 {
            return false;
        }
        return vanilla_enabled(order, order.resolve(owner.file, raw), chain)
            ^ (parent[4] & 1 != 0);
    }
    record.flags & flags::INITIALLY_DISABLED == 0
}
