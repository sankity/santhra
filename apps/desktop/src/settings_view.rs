use rune::tok::{self, Theme, type_};
use rune::{Cx, El, ScrollSpec, col, el, row, text};

use crate::app::{
    FontChoice, Picker, Screen, Shared, State, leaked_family, refresh_model, rgba, toggle_overlay,
};
use crate::fonts;
use crate::settings::{self, Settings};
use crate::widgets::{button, section, slider, text_field, toggle_row};

const MAX_FONT_OPTIONS: usize = 60;

pub fn view_settings(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let shared = Shared::get();
    let settings = shared.settings();

    // Build menu rows only while the matching picker is open. fc-list reports ~2000 families.
    let (audio_options, audio_header) = if s.picker == Some(Picker::Source) {
        {
            let mut sinks = shared.sinks.lock().unwrap();
            if sinks.is_none() {
                *sinks = Some(audio_linux::list_sinks());
            }
        }
        let sinks = { shared.sinks.lock().unwrap().clone().unwrap_or_default() };
        let mut options = vec![option(t, "Default sink", settings.target.is_none(), |s| {
            set_source(s, None)
        })];
        for sink in sinks {
            let name = sink.name.clone();
            let selected = settings.target.as_deref() == Some(name.as_str());
            options.push(option(t, &sink.description, selected, move |s| {
                set_source(s, Some(name.clone()))
            }));
        }
        (options, el())
    } else {
        (Vec::new(), el())
    };

    let (font_options, font_header) = if s.picker == Some(Picker::Font) {
        let query = s.font_query.text.trim().to_lowercase();
        let selected = settings.font_family.as_deref().unwrap_or("");
        let mut names: Vec<&str> = fonts::families()
            .iter()
            .map(String::as_str)
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .collect();
        if !selected.is_empty() && !names.contains(&selected) {
            names.insert(0, selected);
        }
        let mut options = vec![option(
            t,
            "System default",
            settings.font_family.is_none(),
            |s| set_font(s, None),
        )];
        for name in names.into_iter().take(MAX_FONT_OPTIONS) {
            let selected = settings.font_family.as_deref() == Some(name);
            let name = name.to_owned();
            let label = name.clone();
            options.push(option(t, &label, selected, move |s| {
                set_font(s, Some(name.clone()))
            }));
        }
        (
            options,
            text_field("font-search", &s.font_query, |s| &mut s.font_query),
        )
    } else {
        (Vec::new(), el())
    };

    col()
        .key("settings")
        .w_full()
        .h_full()
        .fill(t.bg)
        .child(
            row()
                .h(44.0)
                .px(16.0)
                .center_y()
                .child(text("Settings").font(type_::TITLE).color(t.text)),
        )
        .child(
            col()
                .key("settings-body")
                .grow(1.0)
                .w_full()
                .px(16.0)
                .gap(16.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(section(
                    "Audio",
                    vec![
                        picker_row(
                            "Audio source",
                            settings
                                .target
                                .as_deref()
                                .unwrap_or("Default sink"),
                            Picker::Source,
                            s.picker == Some(Picker::Source),
                            toggle_picker(Picker::Source),
                            audio_options,
                            audio_header,
                        ),
                        labelled("Model directory", text_field("model-dir", &s.model_dir, |s| &mut s.model_dir)),
                        button("Refresh model check", refresh_model),
                        slider("threads", "Threads", settings.threads as f32, 1.0, 8.0, set_threads),
                        slider("lines", "Caption lines", settings.max_lines as f32, 1.0, 5.0, set_lines),
                    ],
                ))
                .child(section(
                    "Caption",
                    vec![
                        picker_row(
                            "Font",
                            settings.font_family.as_deref().unwrap_or("System default"),
                            Picker::Font,
                            s.picker == Some(Picker::Font),
                            toggle_picker(Picker::Font),
                            font_options,
                            font_header,
                        ),
                        slider("font-size", "Font size", settings.font_size, 12.0, 72.0, set_font_size),
                        labelled("Text colour", text_field("text-colour", &s.text_hex, |s| &mut s.text_hex)),
                        background_row(&settings),
                        toggle_row("Show partial results", settings.show_partial, move |s| set_show_partial(s, !settings.show_partial)),
                    ],
                ))
                .child(section(
                    "Startup",
                    vec![toggle_row(
                        "Start listening on launch",
                        settings.start_on_launch,
                        move |s| set_start_on_launch(s, !settings.start_on_launch),
                    )],
                ))
                .child(section(
                    "Overlay",
                    vec![
                        toggle_row("Show floating overlay", settings.overlay_enabled, toggle_overlay),
                        toggle_row("Always on top (X11 only)", settings.overlay_always_on_top, move |s| set_always_on_top(s, !settings.overlay_always_on_top)),
                        toggle_row("Click-through", settings.overlay_click_through, move |s| set_click_through(s, !settings.overlay_click_through)),
                        slider("opacity", "Opacity", settings.overlay_opacity, 0.1, 1.0, set_opacity),
                        slider("width", "Width (on open)", settings.overlay_width, 240.0, 1600.0, set_width),
                        slider("height", "Height (on open)", settings.overlay_height, 80.0, 600.0, set_height),
                    ],
                ))
                .child(
                    col()
                        .gap(4.0)
                        .child(
                            text("Model, thread, overlay size and font changes apply on the next start/open.")
                                .font(type_::LABEL)
                                .color(t.text_secondary),
                        )
                        .child(
                            text(format!("Config: {}", Settings::path().display()))
                                .font(type_::LABEL)
                                .color(t.text_secondary),
                        )
                        .child(
                            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                                text("Wayland: always-on-top and position restore are up to the compositor.")
                            } else {
                                text("")
                            }
                            .font(type_::LABEL)
                            .color(t.text_secondary),
                        ),
                ),
        )
        .child(
            row()
                .h(48.0)
                .px(16.0)
                .gap(8.0)
                .center_y()
                .child(button("Done", done)),
        )
}

fn menu(header: El, options: Vec<El>) -> El {
    let t = tok::theme(true);
    let height = ((options.len() * 28 + 40).min(260)) as f32;
    col()
        .w(340.0)
        .h(height)
        .radius(8.0)
        .fill(t.card)
        .border(1.0, t.edge)
        .padding(4.0)
        .gap(2.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(header)
        .children(options)
}

fn option(t: Theme, label: &str, selected: bool, press: impl Fn(&mut State) + 'static) -> El {
    row()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .fill(if selected { t.control_off } else { t.bg })
        .on_press(press)
        .child(el().size(8.0, 8.0).radius(4.0).fill(if selected {
            tok::accent(0, true)
        } else {
            t.control_off
        }))
        .child(text(label.to_owned()).font(type_::BODY).color(t.text))
}

fn picker_row(
    label: &str,
    value: &str,
    which: Picker,
    open: bool,
    toggle: impl Fn(&mut State) + 'static,
    options: Vec<El>,
    header: El,
) -> El {
    let t = tok::theme(true);
    col()
        .gap(4.0)
        .child(
            row()
                .h(32.0)
                .gap(12.0)
                .center_y()
                .on_press(toggle)
                .child(
                    text(label)
                        .w(112.0)
                        .font(type_::LABEL)
                        .color(t.text_secondary),
                )
                .child(text(value.to_owned()).font(type_::BODY).color(t.text))
                .child(
                    text(if open { "▴" } else { "▾" })
                        .font(type_::BODY)
                        .color(t.text_secondary),
                )
                .child(
                    el().key(match which {
                        Picker::Font => "font-picker-anchor",
                        Picker::Source => "source-picker-anchor",
                    })
                    .size(1.0, 1.0),
                ),
        )
        .child(if open { menu(header, options) } else { el() })
}

fn toggle_picker(which: Picker) -> impl Fn(&mut State) {
    move |s: &mut State| {
        s.picker = if s.picker == Some(which) {
            None
        } else {
            Some(which)
        };
    }
}

fn labelled(label: &str, control: El) -> El {
    let t = tok::theme(true);
    col()
        .gap(6.0)
        .child(text(label).font(type_::LABEL).color(t.text_secondary))
        .child(control)
}

fn background_row(settings: &Settings) -> El {
    let t = tok::theme(true);
    let current = settings.background_rgba();
    let mut row = row().gap(6.0).center_y();
    for (name, value) in settings::BACKGROUND_PRESETS {
        let swatch = settings::parse_rgba(value);
        let border = if swatch == current { 2.0 } else { 1.0 };
        let value: &'static str = value;
        row = row.child(
            el().h(22.0)
                .w(30.0)
                .radius(6.0)
                .fill(rgba(swatch))
                .border(border, if swatch == current { t.text } else { t.edge })
                .on_press(move |s: &mut State| set_background(s, value)),
        );
        let _ = name;
    }
    labelled("Background", row)
}

fn valid_hex(text: &str) -> bool {
    let hex = text.trim().trim_start_matches('#');
    matches!(hex.len(), 6 | 8) && u32::from_str_radix(hex, 16).is_ok()
}

fn done(s: &mut State) {
    let shared = Shared::get();
    {
        let mut settings = shared.settings.lock().unwrap();
        settings.model_dir = s.model_dir.text.clone();
        if valid_hex(&s.text_hex.text) {
            settings.text_color = settings::to_hex(settings::parse_rgba(&s.text_hex.text));
        }
    }
    shared.apply_settings();
    s.picker = None;
    s.screen = Screen::Main;
}

macro_rules! setting_fn {
    ($name:ident, $field:ident, $ty:ty) => {
        fn $name(s: &mut State, value: $ty) {
            let shared = Shared::get();
            shared.settings.lock().unwrap().$field = value;
            let _ = s;
            shared.publish();
        }
    };
}

fn set_threads(s: &mut State, value: f32) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().threads = value as i32;
    let _ = s;
    shared.publish();
}

fn set_lines(s: &mut State, value: f32) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().max_lines = value as usize;
    let _ = s;
    shared.publish();
}

fn set_always_on_top(s: &mut State, value: bool) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().overlay_always_on_top = value;
    let _ = s;
    crate::apply_overlay_options();
    shared.publish();
}

fn set_click_through(s: &mut State, value: bool) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().overlay_click_through = value;
    let _ = s;
    crate::apply_overlay_options();
    shared.publish();
}

setting_fn!(set_show_partial, show_partial, bool);
setting_fn!(set_start_on_launch, start_on_launch, bool);
setting_fn!(set_opacity, overlay_opacity, f32);
setting_fn!(set_width, overlay_width, f32);
setting_fn!(set_height, overlay_height, f32);

fn set_font_size(s: &mut State, value: f32) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().font_size = value;
    shared.font.lock().unwrap().size = value;
    let _ = s;
    shared.publish();
}

fn set_font(s: &mut State, name: Option<String>) {
    let shared = Shared::get();
    let size = {
        let mut settings = shared.settings.lock().unwrap();
        settings.font_family = name.clone();
        settings.font_size
    };
    *shared.font.lock().unwrap() = FontChoice {
        family: name.as_deref().map(leaked_family),
        size,
    };
    s.picker = None;
    shared.publish();
}

fn set_source(s: &mut State, name: Option<String>) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().target = name;
    s.picker = None;
    shared.publish();
}

fn set_background(s: &mut State, value: &'static str) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().background_color = value.to_owned();
    let _ = s;
    shared.publish();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_parseable_hex_colours() {
        assert!(valid_hex("#FFFFFFFF"));
        assert!(valid_hex("101014E6"));
        assert!(!valid_hex("#FFF"));
        assert!(!valid_hex("#GGGGGGGG"));
    }
}
