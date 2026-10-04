fn main() -> Result<(), image::ImageError> {
    let mut icon = image::RgbaImage::from_fn(1024, 1024, |x, y| {
        let dx = (x as f32 - 512.0).abs();
        let dy = (y as f32 - 512.0).abs();
        let rounded =
            ((dx - 282.0).max(0.0).powi(2) + (dy - 282.0).max(0.0).powi(2)).sqrt() < 170.0;
        if rounded && dx < 452.0 && dy < 452.0 {
            image::Rgba([69, (88.0 + y as f32 * 0.025) as u8, 230, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    for (x, y) in [
        (0, 0),
        (1, 0),
        (2, 0),
        (0, 1),
        (2, 1),
        (0, 2),
        (1, 2),
        (0, 3),
    ] {
        for py in 264 + y * 125..264 + y * 125 + 109 {
            for px in 328 + x * 125..328 + x * 125 + 109 {
                icon.put_pixel(px, py, image::Rgba([255, 255, 255, 255]));
            }
        }
    }
    icon.save("target/app-icon.png")
}
