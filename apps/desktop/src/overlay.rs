use std::sync::{Arc, Mutex};
use std::time::Duration;

use caption_core::CaptionSnapshot;
use eframe::egui::{
    self, Color32, Pos2, Rect, ResizeDirection, Sense, ViewportCommand, WindowLevel,
};

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
    pub font_family: Option<String>,
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
            font_family: settings.font_family.clone(),
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
    pub size: Option<[f32; 2]>,
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
        .with_resizable(true)
        .with_min_inner_size([240.0, 80.0])
        .with_max_inner_size([1600.0, 600.0])
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

        let window_rect = ui.max_rect();
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
                                style.font_family.as_deref(),
                            );
                        }
                        for line in caption.finals.iter().rev() {
                            caption_row(
                                ui,
                                &line.text,
                                style.font_size,
                                fg_color(&style),
                                false,
                                style.font_family.as_deref(),
                            );
                        }
                    });
                }
            });

        let drag = ui.interact(
            window_rect,
            ui.scope_id().with("drag"),
            Sense::click_and_drag(),
        );
        if drag.drag_started() {
            let origin = ui.input(|input| input.pointer.press_origin());
            match origin.and_then(|pos| resize_direction(window_rect, pos, RESIZE_BAND)) {
                Some(direction) => {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::BeginResize(direction));
                }
                None => {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
            }
        }
        let hover = ui.input(|input| input.pointer.hover_pos());
        if let Some(direction) =
            hover.and_then(|pos| resize_direction(window_rect, pos, RESIZE_BAND))
        {
            ui.ctx().set_cursor_icon(resize_cursor(direction));
        } else if drag.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if drag.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        let (outer_rect, inner_rect) = ui
            .ctx()
            .input(|input| (input.viewport().outer_rect, input.viewport().inner_rect));
        {
            let mut state = shared.lock().unwrap();
            if let Some(rect) = outer_rect {
                state.pos = Some([rect.min.x, rect.min.y]);
            }
            if let Some(rect) = inner_rect {
                state.size = Some([rect.width(), rect.height()]);
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    });
}

const RESIZE_BAND: f32 = 8.0;

fn resize_direction(rect: Rect, pos: Pos2, band: f32) -> Option<ResizeDirection> {
    if !rect.contains(pos) {
        return None;
    }
    let horizontal = if pos.x - rect.left() <= band {
        Some(-1)
    } else if rect.right() - pos.x <= band {
        Some(1)
    } else {
        None
    };
    let vertical = if pos.y - rect.top() <= band {
        Some(-1)
    } else if rect.bottom() - pos.y <= band {
        Some(1)
    } else {
        None
    };
    Some(match (horizontal, vertical) {
        (Some(-1), Some(-1)) => ResizeDirection::NorthWest,
        (Some(1), Some(-1)) => ResizeDirection::NorthEast,
        (Some(-1), Some(1)) => ResizeDirection::SouthWest,
        (Some(1), Some(1)) => ResizeDirection::SouthEast,
        (Some(-1), None) => ResizeDirection::West,
        (Some(1), None) => ResizeDirection::East,
        (None, Some(-1)) => ResizeDirection::North,
        (None, Some(1)) => ResizeDirection::South,
        (None, None) => return None,
        _ => unreachable!(),
    })
}

fn resize_cursor(direction: ResizeDirection) -> egui::CursorIcon {
    match direction {
        ResizeDirection::North => egui::CursorIcon::ResizeNorth,
        ResizeDirection::South => egui::CursorIcon::ResizeSouth,
        ResizeDirection::East => egui::CursorIcon::ResizeEast,
        ResizeDirection::West => egui::CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => egui::CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => egui::CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => egui::CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => egui::CursorIcon::ResizeSouthWest,
    }
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

    #[test]
    fn resize_bands_map_to_directions() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 200.0));
        let band = RESIZE_BAND;
        assert_eq!(
            resize_direction(rect, Pos2::new(2.0, 2.0), band),
            Some(ResizeDirection::NorthWest)
        );
        assert_eq!(
            resize_direction(rect, Pos2::new(398.0, 198.0), band),
            Some(ResizeDirection::SouthEast)
        );
        assert_eq!(
            resize_direction(rect, Pos2::new(200.0, 1.0), band),
            Some(ResizeDirection::North)
        );
        assert_eq!(
            resize_direction(rect, Pos2::new(399.0, 100.0), band),
            Some(ResizeDirection::East)
        );
        assert_eq!(
            resize_direction(rect, Pos2::new(200.0, 199.0), band),
            Some(ResizeDirection::South)
        );
        assert_eq!(resize_direction(rect, Pos2::new(200.0, 100.0), band), None);
    }
}
