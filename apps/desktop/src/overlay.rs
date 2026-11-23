use std::sync::{Arc, Mutex};
use std::time::Duration;

use caption_core::CaptionSnapshot;
use eframe::egui::{self, Color32, Sense, ViewportCommand, WindowLevel};

use crate::app::caption_row;
use crate::settings::Settings;

pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("santhra-overlay")
}

#[derive(Clone, Default)]
pub struct Style {
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub always_on_top: bool,
    pub click_through: bool,
    pub font_size: f32,
    pub text_color: [u8; 4],
    pub background_color: [u8; 4],
    pub show_partial: bool,
}

impl Style {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            width: settings.overlay_width,
            height: settings.overlay_height,
            opacity: settings.overlay_opacity,
            always_on_top: settings.overlay_always_on_top,
            click_through: settings.overlay_click_through,
            font_size: settings.font_size,
            text_color: settings.text_rgba(),
            background_color: settings.background_rgba(),
            show_partial: settings.show_partial,
        }
    }
}

#[derive(Default)]
pub struct Shared {
    pub snapshot: CaptionSnapshot,
    pub style: Style,
    pub pos: Option<[f32; 2]>,
}

impl Shared {
    pub fn update(&mut self, settings: &Settings, snapshot: &CaptionSnapshot) {
        self.style = Style::from_settings(settings);
        self.snapshot = snapshot.clone();
    }
}

pub fn bg_color(style: &Style) -> Color32 {
    let [red, green, blue, alpha] = style.background_color;
    let alpha = (alpha as f32 * style.opacity.clamp(0.1, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

pub fn fg_color(style: &Style) -> Color32 {
    let [red, green, blue, alpha] = style.text_color;
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

pub fn show(ctx: &egui::Context, shared: &Arc<Mutex<Shared>>, enabled: bool) {
    if !enabled {
        return;
    }

    let (style, pos) = {
        let state = shared.lock().unwrap();
        (state.style.clone(), state.pos)
    };

    let mut builder = egui::ViewportBuilder::default()
        .with_title("Santhra captions")
        .with_inner_size([style.width, style.height])
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_active(false)
        .with_window_level(if style.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        })
        .with_mouse_passthrough(style.click_through);
    if let Some(pos) = pos {
        builder = builder.with_position(pos);
    }

    // Deferred so the overlay keeps painting while the main window is covered or minimized.
    let shared = Arc::clone(shared);
    ctx.show_viewport_deferred(viewport_id(), builder, move |ui, _class| {
        let (style, caption) = {
            let state = shared.lock().unwrap();
            (state.style.clone(), state.snapshot.clone())
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg_color(&style)).inner_margin(12.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if !(caption.finals.is_empty() && caption.partial.is_empty()) {
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        if style.show_partial && !caption.partial.is_empty() {
                            caption_row(
                                ui,
                                &caption.partial,
                                style.font_size,
                                fg_color(&style),
                                true,
                            );
                        }
                        for line in caption.finals.iter().rev() {
                            caption_row(ui, &line.text, style.font_size, fg_color(&style), false);
                        }
                    });
                }
                let drag = ui.interact(
                    ui.max_rect(),
                    ui.scope_id().with("drag"),
                    Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if drag.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            });

        if let Some(rect) = ui.ctx().input(|input| input.viewport().outer_rect) {
            shared.lock().unwrap().pos = Some([rect.min.x, rect.min.y]);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_alpha_is_scaled_and_clamped() {
        let mut style = Style {
            background_color: [0, 0, 0, 0x80],
            ..Default::default()
        };
        style.opacity = 0.5;
        assert_eq!(bg_color(&style).to_srgba_unmultiplied()[3], 64);
        style.opacity = 2.0;
        assert_eq!(bg_color(&style).to_srgba_unmultiplied()[3], 128);
        style.opacity = 0.0;
        assert_eq!(bg_color(&style).to_srgba_unmultiplied()[3], 13);
    }
}
