use crate::document::{Document, Layer};
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use thiserror::Error;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("project is malformed: {0}")]
    Invalid(String),
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u8,
    active_layer: usize,
    pixel_scale: u32,
    palette_size: u16,
    dither: bool,
    layers: Vec<LayerMeta>,
    #[serde(default)]
    frames: Vec<FrameMeta>,
    #[serde(default)]
    credits: String,
}
#[derive(Serialize, Deserialize)]
struct FrameMeta {
    duration_ms: u16,
    layers: Vec<LayerMeta>,
}
#[derive(Serialize, Deserialize)]
struct LayerMeta {
    name: String,
    visible: bool,
    opacity: u8,
    asset: String,
}

pub fn save(document: &Document, path: &Path) -> Result<(), ProjectError> {
    document.validate().map_err(ProjectError::Invalid)?;
    let file = File::create(path)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut layers = Vec::with_capacity(document.layers.len());
    for (index, layer) in document.layers.iter().enumerate() {
        let asset = format!("layers/{index}.png");
        archive.start_file(&asset, options)?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(layer.pixels.clone())
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)?;
        archive.write_all(&png)?;
        layers.push(LayerMeta {
            name: layer.name.clone(),
            visible: layer.visible,
            opacity: layer.opacity,
            asset,
        });
    }
    let mut frames = Vec::new();
    for (frame_index, frame) in document.animation_frames.iter().enumerate() {
        let mut frame_layers = Vec::new();
        for (index, layer) in frame.layers.iter().enumerate() {
            let asset = format!("frames/{frame_index}/{index}.png");
            archive.start_file(&asset, options)?;
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(layer.pixels.clone())
                .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)?;
            archive.write_all(&png)?;
            frame_layers.push(LayerMeta {
                name: layer.name.clone(),
                visible: layer.visible,
                opacity: layer.opacity,
                asset,
            });
        }
        frames.push(FrameMeta {
            duration_ms: frame.duration_ms,
            layers: frame_layers,
        });
    }
    let manifest = Manifest {
        version: 1,
        active_layer: document.active_layer,
        pixel_scale: document.pixel_scale,
        palette_size: document.palette_size,
        dither: document.dither,
        layers,
        frames,
        credits: document.credits.clone(),
    };
    archive.start_file("manifest.json", options)?;
    archive.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    archive.finish()?;
    Ok(())
}
pub fn load(path: &Path) -> Result<Document, ProjectError> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let mut content = Vec::new();
    archive
        .by_name("manifest.json")?
        .read_to_end(&mut content)?;
    let manifest: Manifest = serde_json::from_slice(&content)?;
    if manifest.version != 1 {
        return Err(ProjectError::Invalid(format!(
            "unsupported version {}",
            manifest.version
        )));
    }
    let mut layers = Vec::new();
    for meta in manifest.layers {
        let mut bytes = Vec::new();
        archive.by_name(&meta.asset)?.read_to_end(&mut bytes)?;
        let decoded = image::load_from_memory(&bytes)?.to_rgba8();
        layers.push(Layer {
            name: meta.name,
            pixels: decoded,
            visible: meta.visible,
            opacity: meta.opacity,
        });
    }
    if layers.is_empty() {
        return Err(ProjectError::Invalid("contains no layers".into()));
    }
    let mut animation_frames = Vec::new();
    for frame in manifest.frames {
        let mut frame_layers = Vec::new();
        for meta in frame.layers {
            let mut bytes = Vec::new();
            archive.by_name(&meta.asset)?.read_to_end(&mut bytes)?;
            frame_layers.push(Layer {
                name: meta.name,
                pixels: image::load_from_memory(&bytes)?.to_rgba8(),
                visible: meta.visible,
                opacity: meta.opacity,
            });
        }
        animation_frames.push(crate::document::AnimationFrame {
            layers: frame_layers,
            duration_ms: frame.duration_ms,
        });
    }
    let active_layer = manifest.active_layer.min(layers.len() - 1);
    let document = Document {
        layers,
        active_layer,
        pixel_scale: manifest.pixel_scale.max(1),
        palette_size: manifest.palette_size,
        dither: manifest.dither,
        animation_frames,
        credits: manifest.credits,
    };
    document.validate().map_err(ProjectError::Invalid)?;
    Ok(document)
}
