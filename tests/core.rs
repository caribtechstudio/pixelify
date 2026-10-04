use image::{Rgba, RgbaImage};
use pixelify_core::{effects, export, project, Document};
use std::{fs, io::Read};

fn image() -> RgbaImage {
    RgbaImage::from_fn(12, 8, |x, y| {
        if x > 3 && x < 9 && y > 1 && y < 7 {
            Rgba([230, 30, 20, 255])
        } else {
            Rgba([20, 100, 210, 255])
        }
    })
}

#[test]
fn pixelation_preserves_size_and_creates_blocks() {
    let source = image();
    let result = effects::pixelate(&source, 4);
    assert_eq!(result.dimensions(), source.dimensions());
    assert_eq!(result.get_pixel(0, 0), result.get_pixel(3, 3));
}
#[test]
fn background_removal_only_erases_border_connected_pixels() {
    let source = image();
    let result = effects::remove_background(&source, 10);
    assert_eq!(result.get_pixel(0, 0)[3], 0);
    assert_eq!(result.get_pixel(6, 4)[3], 255);
}
#[test]
fn mask_brush_changes_requested_alpha() {
    let mut source = image();
    effects::paint_mask(&mut source, 6, 4, 2, false);
    assert_eq!(source.get_pixel(6, 4)[3], 0);
    effects::paint_mask(&mut source, 6, 4, 2, true);
    assert_eq!(source.get_pixel(6, 4)[3], 255);
}
#[test]
fn project_round_trip_preserves_document() {
    let document = Document::from_image("test", image());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.pixelify");
    project::save(&document, &path).unwrap();
    let loaded = project::load(&path).unwrap();
    assert_eq!(loaded.flattened().unwrap(), document.flattened().unwrap());
}
#[test]
fn aseprite_export_has_required_magic_number() {
    let document = Document::from_image("test", image());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.aseprite");
    export::export_aseprite(&document, &path).unwrap();
    let data = fs::read(path).unwrap();
    assert_eq!(&data[4..6], &0xA5E0u16.to_le_bytes());
    assert_eq!(
        u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize,
        data.len()
    );
}

#[test]
fn invalid_layer_sizes_are_rejected_instead_of_panicking() {
    let mut document = Document::from_image("base", image());
    document.layers.push(pixelify_core::Layer {
        name: "bad".into(),
        pixels: RgbaImage::new(1, 1),
        visible: true,
        opacity: 255,
    });
    assert!(document.flattened().is_none());
    let directory = tempfile::tempdir().unwrap();
    assert!(export::export_image(&document, &directory.path().join("test.png")).is_err());
}

#[test]
fn named_palette_preserves_alpha_and_uses_only_swatches() {
    let source = RgbaImage::from_pixel(2, 2, Rgba([240, 20, 40, 128]));
    let palette = [[0, 0, 0], [255, 255, 255]];
    let result = effects::map_palette(&source, &palette);
    for p in result.pixels() {
        assert_eq!(p[3], 128);
        assert!(palette.contains(&[p[0], p[1], p[2]]));
    }
}

#[test]
fn tonal_palette_preserves_light_dark_and_transparent_pixels() {
    let mut source = RgbaImage::new(3, 1);
    source.put_pixel(0, 0, Rgba([0, 0, 0, 128]));
    source.put_pixel(1, 0, Rgba([255, 255, 255, 255]));
    source.put_pixel(2, 0, Rgba([64, 32, 16, 0]));
    let palette = [[15, 56, 15], [155, 188, 15]];
    let result = effects::map_tonal_palette(&source, &palette);
    assert_eq!(*result.get_pixel(0, 0), Rgba([15, 56, 15, 128]));
    assert_eq!(*result.get_pixel(1, 0), Rgba([155, 188, 15, 255]));
    assert_eq!(result.get_pixel(2, 0), source.get_pixel(2, 0));
    assert_eq!(effects::map_tonal_palette(&source, &[]), source);
}

#[test]
fn project_round_trip_preserves_animation_pixels_and_timing() {
    let mut doc = Document::from_image("animation", image());
    doc.capture_frame(230);
    doc.animation_frames[0].layers[0]
        .pixels
        .put_pixel(0, 0, Rgba([1, 2, 3, 4]));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("frames.pixelify");
    project::save(&doc, &path).unwrap();
    let restored = project::load(&path).unwrap();
    assert_eq!(restored.animation_frames.len(), 1);
    assert_eq!(restored.animation_frames[0].duration_ms, 230);
    assert_eq!(
        restored.animation_frames[0].layers[0].pixels,
        doc.animation_frames[0].layers[0].pixels
    );
}

#[test]
fn jpeg_export_flattens_transparency_onto_white() {
    let doc = Document::from_image(
        "transparent",
        RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0])),
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.jpg");
    export::export_image(&doc, &path).unwrap();
    let decoded = image::open(path).unwrap().to_rgb8();
    assert!(decoded
        .pixels()
        .all(|p| p[0] > 250 && p[1] > 250 && p[2] > 250));
}

#[test]
fn lpc_catalog_composes_a_local_sheet_and_keeps_credits() {
    use pixelify_core::lpc::{Catalog, Selection, SHEET_HEIGHT, SHEET_WIDTH};
    let directory = tempfile::tempdir().unwrap();
    let definitions = directory.path().join("sheet_definitions/body");
    let sprites = directory.path().join("spritesheets/body/male");
    fs::create_dir_all(&definitions).unwrap();
    fs::create_dir_all(&sprites).unwrap();
    fs::write(
        definitions.join("body.json"),
        r#"{
          "name": "Test body", "type_name": "body", "animations": ["walk"],
          "layer_1": { "zPos": 10, "male": "body/male/" },
          "credits": [{ "file": "body/male", "authors": ["Test Artist"], "licenses": ["CC0"], "urls": ["https://example.invalid"] }]
        }"#,
    )
    .unwrap();
    let source = RgbaImage::from_pixel(SHEET_WIDTH, 4 * 64, Rgba([12, 34, 56, 255]));
    source.save(sprites.join("walk.png")).unwrap();

    let catalog = Catalog::open(directory.path()).unwrap();
    let document = catalog
        .compose(
            &[Selection {
                item_id: "body/body".into(),
                variant: "base".into(),
            }],
            "male",
        )
        .unwrap();
    assert_eq!(document.dimensions(), Some((SHEET_WIDTH, SHEET_HEIGHT)));
    assert_eq!(
        document.layers[0].pixels.get_pixel(0, 8 * 64),
        &Rgba([12, 34, 56, 255])
    );
    assert!(document.credits.contains("Test Artist"));
}

#[test]
fn lpc_standard_animation_layout_stays_inside_the_exported_sheet() {
    use pixelify_core::lpc::{standard_animations, SHEET_HEIGHT, SHEET_WIDTH};
    for animation in standard_animations() {
        assert!(
            !animation.cycle().is_empty(),
            "{} has no frames",
            animation.id()
        );
        assert!(
            animation.cycle().iter().all(|frame| *frame < 13),
            "{} exceeds the 13-column LPC sheet",
            animation.id()
        );
        assert!(
            (animation.row() + animation.row_count()) * 64 <= SHEET_HEIGHT,
            "{} exceeds the sheet height",
            animation.id()
        );
        assert!(SHEET_WIDTH == 13 * 64);
    }
}

#[test]
fn lpc_animation_preview_shows_the_expected_frames_and_directions() {
    use pixelify_core::lpc::{animation_preview, standard_animations, SHEET_HEIGHT, SHEET_WIDTH};
    let sheet = RgbaImage::from_fn(SHEET_WIDTH, SHEET_HEIGHT, |x, y| {
        Rgba([(y / 64) as u8, (x / 64) as u8, 0, 255])
    });
    for animation in standard_animations() {
        let cycle_index = animation.cycle().len() - 1;
        let frame = animation.cycle()[cycle_index];
        let preview = animation_preview(&sheet, animation.id(), cycle_index).unwrap();
        assert_eq!(preview.dimensions(), (64 * animation.row_count(), 64));
        for direction in 0..animation.row_count() {
            assert_eq!(
                *preview.get_pixel(direction * 64, 0),
                Rgba([(animation.row() + direction) as u8, frame as u8, 0, 255]),
                "{} direction {direction}",
                animation.id(),
            );
        }
    }
    assert!(animation_preview(&sheet, "missing", 0).is_none());
}

#[test]
#[ignore = "requires the optional full assets/lpc checkout"]
fn full_local_lpc_library_composes_a_character_with_every_standard_preview() {
    use pixelify_core::lpc::{standard_animations, Catalog, Selection};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lpc");
    let catalog = Catalog::open(&root).expect("the local LPC checkout should open");
    let mut selections = vec![catalog.default_body().expect("a body definition")];
    selections.extend(
        catalog
            .items
            .iter()
            .filter(|item| {
                item.type_name != "body" && item.body_types.iter().any(|body| body == "male")
            })
            .take(5)
            .map(|item| Selection {
                item_id: item.id.clone(),
                variant: item
                    .variants
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "base".into()),
            }),
    );
    let document = catalog
        .compose(&selections, "male")
        .expect("the selected compatible assets should compose");
    assert!(document.layers.len() >= 2);
    let sheet = document
        .flattened()
        .expect("all layers share the LPC sheet size");
    for animation in standard_animations() {
        let frame = animation.cycle()[0];
        let visible = (0..animation.row_count()).any(|direction| {
            (0..64).any(|y| {
                (0..64).any(|x| {
                    sheet.get_pixel(frame * 64 + x, (animation.row() + direction) * 64 + y)[3] > 0
                })
            })
        });
        assert!(visible, "{} should have visible pixels", animation.id());
    }
}

#[test]
fn lpc_archive_keeps_sprite_sheet_and_credits_together() {
    let mut document = Document::from_image("character", RgbaImage::new(8, 8));
    document.credits = "Asset: test\nAuthors: Test Artist\n".into();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("character.zip");
    export::export_sprite_sheet_zip(&document, &path).unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    assert!(archive.by_name("character.png").is_ok());
    let mut credits = String::new();
    archive
        .by_name("CREDITS.txt")
        .unwrap()
        .read_to_string(&mut credits)
        .unwrap();
    assert!(credits.contains("Test Artist"));
}
