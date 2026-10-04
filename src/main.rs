mod app;
mod design;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Pixelify — Studio")
            .with_inner_size([1380.0, 900.0])
            .with_min_inner_size([1080.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pixelify",
        options,
        Box::new(|cc| Ok(Box::new(app::Studio::new(cc)))),
    )
}
