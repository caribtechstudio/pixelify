use image::{imageops::FilterType, Rgba, RgbaImage};
use std::collections::VecDeque;

/// Map every visible pixel to the nearest RGB swatch, preserving alpha exactly.
pub fn map_palette(source: &RgbaImage, palette: &[[u8; 3]]) -> RgbaImage {
    if palette.is_empty() {
        return source.clone();
    }
    let mut output = source.clone();
    for pixel in output.pixels_mut().filter(|p| p[3] != 0) {
        if let Some(color) = palette.iter().min_by_key(|color| {
            (0..3)
                .map(|c| {
                    let delta = i32::from(pixel[c]) - i32::from(color[c]);
                    delta * delta
                })
                .sum::<i32>()
        }) {
            for c in 0..3 {
                pixel[c] = color[c];
            }
        }
    }
    output
}

/// Tonal palettes (Game Boy, monochrome) are ordered from darkest to lightest.
/// Match luminance rather than RGB hue so warm photos retain their light/shadow detail.
pub fn map_tonal_palette(source: &RgbaImage, palette: &[[u8; 3]]) -> RgbaImage {
    if palette.is_empty() {
        return source.clone();
    }
    let mut output = source.clone();
    for p in output.pixels_mut().filter(|p| p[3] != 0) {
        let luminance =
            0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
        let index = ((luminance / 255.0) * (palette.len() - 1) as f32).round() as usize;
        for c in 0..3 {
            p[c] = palette[index][c];
        }
    }
    output
}

/// Pixelate by downscaling and restoring the original size with nearest-neighbour sampling.
pub fn pixelate(source: &RgbaImage, block_size: u32) -> RgbaImage {
    let block_size = block_size.max(1);
    let (width, height) = source.dimensions();
    let small_w = (width / block_size).max(1);
    let small_h = (height / block_size).max(1);
    let small = image::imageops::resize(source, small_w, small_h, FilterType::Triangle);
    image::imageops::resize(&small, width, height, FilterType::Nearest)
}

/// Quantises RGB channels to a uniform palette. Alpha is never modified.
pub fn reduce_palette(source: &RgbaImage, colors: u16) -> RgbaImage {
    if colors < 2 {
        return source.clone();
    }
    let levels = (f32::from(colors).cbrt().round() as u16).clamp(2, 16);
    let step = 255.0 / f32::from(levels - 1);
    let mut output = source.clone();
    for pixel in output.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }
        for channel in 0..3 {
            pixel[channel] = ((f32::from(pixel[channel]) / step).round() * step).round() as u8;
        }
    }
    output
}

/// Remove a border-connected background based on the average border colour.
/// This deliberately preserves disconnected background-coloured detail inside a subject.
pub fn remove_background(source: &RgbaImage, tolerance: u8) -> RgbaImage {
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return source.clone();
    }
    let mut border = Vec::with_capacity(((width + height) * 2) as usize);
    for x in 0..width {
        border.push(*source.get_pixel(x, 0));
        border.push(*source.get_pixel(x, height - 1));
    }
    for y in 1..height.saturating_sub(1) {
        border.push(*source.get_pixel(0, y));
        border.push(*source.get_pixel(width - 1, y));
    }
    let mut mean = [0u32; 3];
    let mut count = 0u32;
    for pixel in border.into_iter().filter(|pixel| pixel[3] > 0) {
        for channel in 0..3 {
            mean[channel] += u32::from(pixel[channel]);
        }
        count += 1;
    }
    if count == 0 {
        return source.clone();
    }
    let target = [mean[0] / count, mean[1] / count, mean[2] / count];
    let threshold = u32::from(tolerance).pow(2) * 3;
    let similar = |p: &Rgba<u8>| -> bool {
        let distance: u32 = (0..3)
            .map(|c| {
                let delta = i32::from(p[c]) - target[c] as i32;
                (delta * delta) as u32
            })
            .sum();
        p[3] == 0 || distance <= threshold
    };
    let mut output = source.clone();
    let mut visited = vec![false; (width * height) as usize];
    let mut queue = VecDeque::new();
    for x in 0..width {
        queue.push_back((x, 0));
        queue.push_back((x, height - 1));
    }
    for y in 1..height.saturating_sub(1) {
        queue.push_back((0, y));
        queue.push_back((width - 1, y));
    }
    while let Some((x, y)) = queue.pop_front() {
        let index = (y * width + x) as usize;
        if visited[index] || !similar(output.get_pixel(x, y)) {
            continue;
        }
        visited[index] = true;
        output.get_pixel_mut(x, y)[3] = 0;
        if x > 0 {
            queue.push_back((x - 1, y));
        }
        if x + 1 < width {
            queue.push_back((x + 1, y));
        }
        if y > 0 {
            queue.push_back((x, y - 1));
        }
        if y + 1 < height {
            queue.push_back((x, y + 1));
        }
    }
    output
}

/// Paint alpha in a circular brush. `keep` restores opaque alpha, erase makes it transparent.
pub fn paint_mask(image: &mut RgbaImage, center_x: i32, center_y: i32, radius: u32, keep: bool) {
    let radius = radius as i32;
    let radius_squared = radius * radius;
    for y in (center_y - radius)..=(center_y + radius) {
        for x in (center_x - radius)..=(center_x + radius) {
            if x >= 0
                && y >= 0
                && x < image.width() as i32
                && y < image.height() as i32
                && (x - center_x).pow(2) + (y - center_y).pow(2) <= radius_squared
            {
                image.get_pixel_mut(x as u32, y as u32)[3] = if keep { 255 } else { 0 };
            }
        }
    }
}
