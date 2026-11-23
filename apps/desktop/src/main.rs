mod app;
mod overlay;
mod settings;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title("Voxera")
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "Voxera",
        options,
        Box::new(|cc| Ok(Box::new(app::Voxera::new(cc)))),
    )
}
