use caption_core::CaptionSnapshot;
use eframe::egui::{self, Color32, Sense, ViewportCommand, WindowLevel};

use crate::app::caption_labels;
use crate::settings::Settings;

pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("voxera-overlay")
}

pub fn bg_color(settings: &Settings) -> Color32 {
    let [red, green, blue, alpha] = settings.background_rgba();
    let alpha = (alpha as f32 * settings.overlay_opacity.clamp(0.1, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

pub fn show(ctx: &egui::Context, settings: &mut Settings, snapshot: &CaptionSnapshot) {
    if !settings.overlay_enabled {
        return;
    }

    let mut builder = egui::ViewportBuilder::default()
        .with_title("Voxera captions")
        .with_inner_size([settings.overlay_width, settings.overlay_height])
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_active(false)
        .with_window_level(if settings.overlay_always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        })
        .with_mouse_passthrough(settings.overlay_click_through);
    if let Some(pos) = settings.overlay_pos {
        builder = builder.with_position(pos);
    }

    ctx.show_viewport_immediate(viewport_id(), builder, |ui, _class| {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(bg_color(settings))
                    .inner_margin(12.0),
            )
            .show(ui, |ui| {
                caption_labels(ui, settings, snapshot);
                let drag = ui.interact(
                    ui.max_rect(),
                    ui.scope_id().with("drag"),
                    Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
            });
        if let Some(rect) = ui.ctx().input(|input| input.viewport().outer_rect) {
            settings.overlay_pos = Some([rect.min.x, rect.min.y]);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_alpha_is_scaled_and_clamped() {
        let mut settings = Settings {
            background_color: "#00000080".into(),
            ..Default::default()
        };
        settings.overlay_opacity = 0.5;
        assert_eq!(bg_color(&settings).to_srgba_unmultiplied()[3], 64);
        settings.overlay_opacity = 2.0;
        assert_eq!(bg_color(&settings).to_srgba_unmultiplied()[3], 128);
        settings.overlay_opacity = 0.0;
        assert_eq!(bg_color(&settings).to_srgba_unmultiplied()[3], 13);
    }
}
