mod app;
mod fonts;
mod overlay;
mod settings;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title("Santhra")
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "Santhra",
        options,
        Box::new(|cc| Ok(Box::new(app::Santhra::new(cc)))),
    )
}
