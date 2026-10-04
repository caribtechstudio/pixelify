use crate::document::Document;
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use thiserror::Error;
use zip::{write::SimpleFileOptions, ZipWriter};

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("the document does not contain an image")]
    EmptyDocument,
    #[error("unsupported export extension: {0}")]
    UnsupportedExtension(String),
    #[error("invalid document: {0}")]
    InvalidDocument(String),
    #[error("Aseprite export is limited to 65,535 pixels and layers")]
    AsepriteLimit,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}

pub fn export_image(document: &Document, destination: &Path) -> Result<(), ExportError> {
    document.validate().map_err(ExportError::InvalidDocument)?;
    let image = document.flattened().ok_or(ExportError::EmptyDocument)?;
    let extension = destination
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let format = match extension.as_str() {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        "bmp" => ImageFormat::Bmp,
        "tga" => ImageFormat::Tga,
        "tiff" | "tif" => ImageFormat::Tiff,
        _ => return Err(ExportError::UnsupportedExtension(extension)),
    };
    if format == ImageFormat::Jpeg {
        let opaque = image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
            let p = image.get_pixel(x, y);
            let alpha = u32::from(p[3]);
            image::Rgb(std::array::from_fn(|c| {
                ((u32::from(p[c]) * alpha + 255 * (255 - alpha)) / 255) as u8
            }))
        });
        DynamicImage::ImageRgb8(opaque).save_with_format(destination, format)?;
    } else {
        DynamicImage::ImageRgba8(image).save_with_format(destination, format)?;
    }
    Ok(())
}

/// Exports a game-ready sprite sheet together with the provenance required by
/// the LPC assets used to create it.  Keeping credits beside the PNG makes a
/// later hand-off of a private project much less likely to lose attribution.
pub fn export_sprite_sheet_zip(document: &Document, destination: &Path) -> Result<(), ExportError> {
    document.validate().map_err(ExportError::InvalidDocument)?;
    let image = document.flattened().ok_or(ExportError::EmptyDocument)?;
    let file = fs::File::create(destination)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    archive.start_file("character.png", options)?;
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(image).write_to(&mut io::Cursor::new(&mut png), ImageFormat::Png)?;
    archive.write_all(&png)?;
    archive.start_file("CREDITS.txt", options)?;
    let credits = if document.credits.trim().is_empty() {
        "No third-party asset attribution was recorded for this document.\n"
    } else {
        &document.credits
    };
    archive.write_all(credits.as_bytes())?;
    archive.finish()?;
    Ok(())
}

/// Writes a valid RGBA `.aseprite` document with one frame and one cel per Pixelify layer.
/// Raw cel chunks are used instead of zlib compression for a compact, auditable encoder.
pub fn export_aseprite(document: &Document, destination: &Path) -> Result<(), ExportError> {
    document.validate().map_err(ExportError::InvalidDocument)?;
    let (width, height) = document.dimensions().ok_or(ExportError::EmptyDocument)?;
    if width > u16::MAX as u32
        || height > u16::MAX as u32
        || document.layers.len() > u16::MAX as usize
        || document.animation_frames.len() >= u16::MAX as usize
    {
        return Err(ExportError::AsepriteLimit);
    }
    let source_frames = std::iter::once((&document.layers, 100u16)).chain(
        document
            .animation_frames
            .iter()
            .map(|frame| (&frame.layers, frame.duration_ms)),
    );
    let frames: Vec<_> = source_frames
        .enumerate()
        .map(|(frame_index, (layers, duration))| {
            let mut chunks = Vec::new();
            if frame_index == 0 {
                for layer in layers {
                    chunks.push(layer_chunk(layer, false));
                }
            }
            for (index, layer) in layers.iter().enumerate() {
                chunks.push(cel_chunk(index as u16, &layer.pixels));
            }
            (chunks, duration)
        })
        .collect();
    let sizes: Vec<_> = frames
        .iter()
        .map(|(chunks, _)| 16 + chunks.iter().map(Vec::len).sum::<usize>())
        .collect();
    let total_size = 128 + sizes.iter().sum::<usize>();
    let mut out = Vec::with_capacity(total_size);
    write_u32(&mut out, total_size as u32);
    write_u16(&mut out, 0xA5E0);
    write_u16(&mut out, frames.len() as u16);
    write_u16(&mut out, width as u16);
    write_u16(&mut out, height as u16);
    write_u16(&mut out, 32);
    write_u32(&mut out, 1);
    write_u16(&mut out, 100);
    write_u32(&mut out, 0);
    write_u32(&mut out, 0);
    out.push(0);
    out.extend([0; 3]);
    write_u16(&mut out, 0);
    out.push(1);
    out.push(1);
    write_i16(&mut out, 0);
    write_i16(&mut out, 0);
    write_u16(&mut out, 16);
    write_u16(&mut out, 16);
    out.extend([0; 84]);
    for ((chunks, duration), size) in frames.into_iter().zip(sizes) {
        write_u32(&mut out, size as u32);
        write_u16(&mut out, 0xF1FA);
        write_u16(&mut out, chunks.len() as u16);
        write_u16(&mut out, duration);
        out.extend([0; 2]);
        write_u32(&mut out, 0);
        for chunk in chunks {
            out.extend(chunk);
        }
    }
    fs::write(destination, out)?;
    Ok(())
}

fn layer_chunk(layer: &crate::document::Layer, background: bool) -> Vec<u8> {
    let mut body = Vec::new();
    write_u16(
        &mut body,
        if layer.visible {
            1 | if background { 8 } else { 0 }
        } else {
            0
        },
    );
    write_u16(&mut body, 0);
    write_u16(&mut body, 0);
    write_u16(&mut body, 0);
    write_u16(&mut body, 0);
    write_u16(&mut body, 0);
    body.push(layer.opacity);
    body.extend([0; 3]);
    write_string(&mut body, &layer.name);
    chunk(0x2004, body)
}
fn cel_chunk(layer_index: u16, image: &RgbaImage) -> Vec<u8> {
    let mut body = Vec::new();
    write_u16(&mut body, layer_index);
    write_i16(&mut body, 0);
    write_i16(&mut body, 0);
    body.push(255);
    write_u16(&mut body, 0);
    write_i16(&mut body, 0);
    body.extend([0; 5]);
    write_u16(&mut body, image.width() as u16);
    write_u16(&mut body, image.height() as u16);
    body.extend(image.as_raw());
    chunk(0x2005, body)
}
fn chunk(kind: u16, body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 6);
    write_u32(&mut out, (body.len() + 6) as u32);
    write_u16(&mut out, kind);
    out.extend(body);
    out
}
fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.write_all(&value.to_le_bytes())
        .expect("Vec write cannot fail");
}
fn write_i16(out: &mut Vec<u8>, value: i16) {
    out.write_all(&value.to_le_bytes())
        .expect("Vec write cannot fail");
}
fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.write_all(&value.to_le_bytes())
        .expect("Vec write cannot fail");
}
fn write_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    write_u16(out, bytes.len().min(u16::MAX as usize) as u16);
    out.extend(&bytes[..bytes.len().min(u16::MAX as usize)]);
}
