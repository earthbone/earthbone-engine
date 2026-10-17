// SPDX-License-Identifier: GPL-3.0-or-later

//! NORDIC's closed short sprites retain actual and declared frame counts.

#[test]
fn ui_30_cache_retains_closed_short_roots_while_strict_conversion_rejects_them(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 2;
    let mut bytes = Vec::new();
    swf::write_swf(&header, &[swf::Tag::ShowFrame, swf::Tag::End], &mut bytes)?;
    assert!(soe_swf_convert::convert(&bytes).is_err());
    let inputs = tempfile::tempdir_in("/tmp")?;
    let output = tempfile::tempdir_in("/tmp")?;
    std::fs::create_dir(inputs.path().join("interface"))?;
    std::fs::write(inputs.path().join("interface/fixture.swf"), bytes)?;
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(inputs.path())?;
    let cache = soe_swf_convert::build_cache(
        &vfs,
        &[],
        output.path(),
        &soe_swf_convert::Decompiler::Program("unused".into()),
    )?;
    let path = cache
        .movies
        .get("interface/fixture.swf")
        .ok_or("movie missing")?;
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(
        path.parent().ok_or("movie folder")?.join("movie.json"),
    )?)?;
    assert_eq!(data["timelines"]["0"]["declared_frames"], 2);
    assert_eq!(
        data["timelines"]["0"]["frames"]
            .as_array()
            .ok_or("frames missing")?
            .len(),
        1
    );
    Ok(())
}

#[test]
fn ui_30_actionless_matrix_preserves_the_occupied_depth_and_validates_its_bytes(
) -> Result<(), Box<dyn std::error::Error>> {
    let place = swf::Tag::PlaceObject(Box::new(swf::PlaceObject {
        version: 2,
        action: swf::PlaceObjectAction::Place(7),
        depth: 1,
        matrix: Some(swf::Matrix {
            tx: swf::Twips::from_pixels(20.0),
            ..swf::Matrix::IDENTITY
        }),
        color_transform: None,
        ratio: None,
        name: None,
        clip_depth: None,
        class_name: None,
        filters: None,
        background_color: None,
        blend_mode: None,
        clip_actions: None,
        has_image: false,
        is_bitmap_cached: None,
        is_visible: None,
        amf_data: None,
    }));
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 2;
    let mut bytes = Vec::new();
    swf::write_swf(
        &header,
        &[
            place,
            swf::Tag::ShowFrame,
            swf::Tag::Unknown {
                tag_code: 26,
                data: &[4, 1, 0, 0],
            },
            swf::Tag::ShowFrame,
            swf::Tag::End,
        ],
        &mut bytes,
    )?;
    let movie = soe_swf_convert::convert(&bytes)?;
    let timeline = movie.timelines.get(&0).ok_or("root missing")?;
    assert_eq!(timeline.frames[0].display_list[0].transform[4], 20.0);
    assert_eq!(
        timeline.frames[0].display_list,
        timeline.frames[1].display_list
    );
    assert!(movie.unsupported_tags.contains(&26));
    header.num_frames = 1;
    for payload in [&[4, 1, 0][..], &[4, 1, 0, 0, 255], &[4; 30]] {
        let mut bytes = Vec::new();
        swf::write_swf(
            &header,
            &[
                swf::Tag::Unknown {
                    tag_code: 26,
                    data: payload,
                },
                swf::Tag::ShowFrame,
                swf::Tag::End,
            ],
            &mut bytes,
        )?;
        assert!(soe_swf_convert::convert(&bytes).is_err());
    }
    Ok(())
}

#[test]
fn ui_30_closed_short_sprite_retains_its_authored_frames_without_padding(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut frames: Vec<_> = (0..200).map(|_| swf::Tag::ShowFrame).collect();
    frames.push(swf::Tag::End);
    let tags = [
        swf::Tag::DefineSprite(swf::Sprite {
            id: 24,
            num_frames: 250,
            tags: frames,
        }),
        swf::Tag::ShowFrame,
        swf::Tag::End,
    ];
    let mut bytes = Vec::new();
    swf::write_swf(&header, &tags, &mut bytes)?;
    let movie = soe_swf_convert::convert(&bytes)?;
    let timeline = movie.timelines.get(&24).ok_or("sprite missing")?;
    assert_eq!(timeline.declared_frames, 250);
    assert_eq!(timeline.frames.len(), 200);
    assert_eq!(timeline.frames.last().ok_or("no frame")?.index, 199);
    Ok(())
}

#[test]
fn ui_30_nordic_interface_movies_convert_with_closed_short_sprites(
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = std::env::var_os("SOE_TEST_NORDIC") else {
        return Ok(());
    };
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(std::path::Path::new(&root))?;
    let paths: Vec<_> = vfs
        .list("interface")
        .into_iter()
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("swf"))
        })
        .collect();
    assert_eq!(paths.len(), 35, "NORDIC 2.4.1 movie survey changed");
    let mut short = 0;
    let mut short_roots = 0;
    for path in paths {
        let movie = soe_swf_convert::convert_art(&vfs.read(path)?)
            .map_err(|error| format!("{path}: {error}"))?;
        if let Some(root) = movie
            .timelines
            .get(&0)
            .filter(|timeline| timeline.frames.len() < usize::from(timeline.declared_frames))
        {
            short_roots += 1;
            assert_eq!(path, "interface/loadingmenu.swf");
            assert_eq!(root.declared_frames, 2);
            assert_eq!(root.frames.len(), 1);
        }
        short += movie
            .timelines
            .values()
            .filter(|timeline| {
                timeline.id != 0 && timeline.frames.len() < usize::from(timeline.declared_frames)
            })
            .count();
    }
    assert_eq!(short, 4, "NORDIC 2.4.1 short-sprite survey changed");
    assert_eq!(short_roots, 1, "NORDIC 2.4.1 short-root survey changed");
    Ok(())
}
