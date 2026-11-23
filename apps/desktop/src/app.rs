use std::time::Duration;

use app_core::{Message, Session};
use caption_core::CaptionSnapshot;
use eframe::egui::{self, Align, Color32, Layout, RichText};

use crate::settings::{self, Settings};

pub struct Voxera {
    settings: Settings,
    settings_open: bool,
    sinks: Option<Vec<audio_linux::Sink>>,
    session: Option<Session>,
    snapshot: CaptionSnapshot,
    status: String,
    model_ok: bool,
}

impl Voxera {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        let model_ok =
            speech_sherpa::ModelFiles::load(std::path::Path::new(&settings.model_dir)).is_ok();
        let start = settings.start_on_launch;
        let mut app = Self {
            settings,
            settings_open: false,
            sinks: None,
            session: None,
            snapshot: CaptionSnapshot::default(),
            status: "Idle".into(),
            model_ok,
        };
        if start {
            app.start();
        }
        app
    }

    fn start(&mut self) {
        if self.session.is_some() {
            return;
        }
        self.save_settings();
        match Session::start(self.settings.session_options()) {
            Ok(session) => {
                self.session = Some(session);
                self.status = "Listening".into();
            }
            Err(err) => self.status = format!("cannot start: {err:#}"),
        }
    }

    fn stop(&mut self) {
        if let Some(session) = &self.session {
            session.request_stop();
            self.status = "Stopping".into();
        }
    }

    fn poll(&mut self) {
        let mut messages = Vec::new();
        if let Some(session) = &self.session {
            while let Some(message) = session.try_recv() {
                messages.push(message);
            }
        }
        for message in messages {
            match message {
                Message::Captions(snapshot) => self.snapshot = snapshot,
                Message::Stopped { dropped } => {
                    self.status = if dropped > 0 {
                        format!("Idle, {dropped} audio chunks dropped")
                    } else {
                        "Idle".into()
                    };
                    self.session = None;
                }
                Message::Failed(err) => {
                    self.status = format!("error: {err}");
                    self.session = None;
                }
            }
        }
    }

    fn save_settings(&mut self) {
        if let Err(err) = self.settings.save() {
            self.status = format!("cannot save settings: {err:#}");
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| self.settings_body(ui));
        self.settings_open = open;
        if !open {
            self.save_settings();
        }
    }

    fn settings_body(&mut self, ui: &mut egui::Ui) {
        if self.sinks.is_none() {
            self.sinks = Some(audio_linux::list_sinks());
        }
        let sinks = self.sinks.clone().unwrap_or_default();

        egui::Grid::new("settings-grid")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label("Audio source");
                ui.vertical(|ui| {
                    ui.selectable_value(&mut self.settings.target, None, "Default sink");
                    for sink in &sinks {
                        ui.selectable_value(
                            &mut self.settings.target,
                            Some(sink.name.clone()),
                            &sink.description,
                        );
                    }
                });
                ui.end_row();

                ui.label("Model directory");
                ui.text_edit_singleline(&mut self.settings.model_dir);
                ui.end_row();

                ui.label("Threads");
                ui.add(egui::Slider::new(&mut self.settings.threads, 1..=8));
                ui.end_row();

                ui.label("Caption lines");
                ui.add(egui::Slider::new(&mut self.settings.max_lines, 1..=5));
                ui.end_row();

                ui.label("Font size");
                ui.add(egui::Slider::new(&mut self.settings.font_size, 12.0..=72.0));
                ui.end_row();

                ui.label("Text colour");
                let mut text = color(self.settings.text_rgba());
                if ui.color_edit_button_srgba(&mut text).changed() {
                    self.settings.text_color = settings::to_hex(text.to_srgba_unmultiplied());
                }
                ui.end_row();

                ui.label("Background");
                let mut background = color(self.settings.background_rgba());
                if ui.color_edit_button_srgba(&mut background).changed() {
                    self.settings.background_color =
                        settings::to_hex(background.to_srgba_unmultiplied());
                }
                ui.end_row();

                ui.label("Partial text");
                ui.checkbox(&mut self.settings.show_partial, "Show partial results");
                ui.end_row();

                ui.label("Startup");
                ui.checkbox(
                    &mut self.settings.start_on_launch,
                    "Start listening on launch",
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.label("Model and thread changes apply on the next start.");
        ui.label(format!("Config: {}", Settings::path().display()));
    }
}

impl eframe::App for Voxera {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let running = self.session.is_some();
                if ui
                    .add_enabled(!running, egui::Button::new("Start"))
                    .clicked()
                {
                    self.start();
                }
                if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                    self.stop();
                }
                ui.separator();
                ui.label(&self.status);
                if !self.model_ok {
                    ui.colored_label(Color32::RED, "model files not found");
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Settings").clicked() {
                        self.settings_open = true;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let fg = color(self.settings.text_rgba());
            let bg = color(self.settings.background_rgba());
            egui::Frame::NONE
                .fill(bg)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.spacing_mut().item_spacing.y = 6.0;
                    if self.snapshot.finals.is_empty() && self.snapshot.partial.is_empty() {
                        ui.label(
                            RichText::new("No captions yet. Press Start, then play something.")
                                .color(fg)
                                .size(18.0),
                        );
                    }
                    for line in &self.snapshot.finals {
                        ui.label(
                            RichText::new(&line.text)
                                .color(fg)
                                .size(self.settings.font_size),
                        );
                    }
                    if self.settings.show_partial && !self.snapshot.partial.is_empty() {
                        ui.label(
                            RichText::new(&self.snapshot.partial)
                                .color(fg)
                                .size(self.settings.font_size)
                                .italics(),
                        );
                    }
                });
        });

        self.settings_window(ui.ctx());
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.session.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl Drop for Voxera {
    fn drop(&mut self) {
        let _ = self.settings.save();
    }
}

fn color(rgba: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}
