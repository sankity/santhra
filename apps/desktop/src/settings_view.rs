use rune::tok::{self, type_};
use rune::{Cx, El, ScrollSpec, col, el, row, text};
use rune_kit::{Kit, Menu, MenuItem, root};
use santhra_models::{Language, catalog, entry, model_dir};

use crate::app::{
    DownloadStatus, FontChoice, Picker, Screen, SettingsTab, Shared, State, cancel_download,
    leaked_family, refresh_model, rgba, start_download, toggle_overlay, use_model,
};
use crate::fonts;
use crate::settings::{self, Settings};
use crate::widgets::{button, section, slider, text_field, toggle_row};

const MAX_FONT_OPTIONS: usize = 60;

type Item = (String, bool, Box<dyn for<'a> Fn(&'a mut State)>);

struct MenuCtx<'a> {
    kit: &'a Kit,
    overlays: &'a mut Vec<El>,
}

struct Items {
    audio: Vec<Item>,
    font: Vec<Item>,
    language: Vec<Item>,
    model: Vec<Item>,
}

pub fn view_settings(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let shared = Shared::get();
    let settings = shared.settings();

    let audio = if s.picker == Some(Picker::Source) {
        {
            let mut sinks = shared.sinks.lock().unwrap();
            if sinks.is_none() {
                *sinks = Some(audio_linux::list_sinks());
            }
        }
        let sinks = { shared.sinks.lock().unwrap().clone().unwrap_or_default() };
        let mut items: Vec<Item> = vec![(
            "Default sink".into(),
            settings.target.is_none(),
            Box::new(|s| set_source(s, None)),
        )];
        for sink in sinks {
            let name = sink.name.clone();
            let selected = settings.target.as_deref() == Some(name.as_str());
            items.push((
                sink.description,
                selected,
                Box::new(move |s| set_source(s, Some(name.clone()))),
            ));
        }
        items
    } else {
        Vec::new()
    };

    let font = if s.picker == Some(Picker::Font) {
        let query = s.font_query.text.trim().to_lowercase();
        let selected_font = settings.font_family.as_deref().unwrap_or("");
        let mut names: Vec<&str> = fonts::families()
            .iter()
            .map(String::as_str)
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .collect();
        if !selected_font.is_empty() && !names.contains(&selected_font) {
            names.insert(0, selected_font);
        }
        let mut items: Vec<Item> = vec![(
            "System default".into(),
            settings.font_family.is_none(),
            Box::new(|s| set_font(s, None)),
        )];
        for name in names.into_iter().take(MAX_FONT_OPTIONS) {
            let name = name.to_owned();
            let selected = settings.font_family.as_deref() == Some(name.as_str());
            items.push((
                name.clone(),
                selected,
                Box::new(move |s| set_font(s, Some(name.clone()))),
            ));
        }
        items
    } else {
        Vec::new()
    };

    let language: Vec<Item> = if s.picker == Some(Picker::Language) {
        Language::ALL
            .iter()
            .map(|lang| {
                let id = lang.as_str();
                (
                    lang.label().to_owned(),
                    settings.language.as_deref() == Some(id),
                    Box::new(move |s: &mut State| set_language(s, id))
                        as Box<dyn for<'a> Fn(&'a mut State)>,
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    let model: Vec<Item> = if s.picker == Some(Picker::Model) {
        let language = settings.language.as_deref().and_then(Language::parse);
        catalog()
            .iter()
            .filter(|item| {
                language.is_none_or(|lang| santhra_models::matches_language(item.language, lang))
            })
            .map(|item| {
                let id = item.id;
                let label = format!("{} — {}", item.label, item.language.label());
                let selected = settings.model_id.as_deref() == Some(id);
                (
                    label,
                    selected,
                    Box::new(move |s: &mut State| select_model(s, id))
                        as Box<dyn for<'a> Fn(&'a mut State)>,
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    let kit = Kit::neutral(true);
    let mut overlays = Vec::new();
    let body = {
        let mut ctx = MenuCtx {
            kit: &kit,
            overlays: &mut overlays,
        };
        let Items {
            audio,
            font,
            language,
            model,
        } = Items {
            audio,
            font,
            language,
            model,
        };
        let tabs = [
            SettingsTab::Recognition,
            SettingsTab::Appearance,
            SettingsTab::About,
        ];
        let mut sidebar = col().w(150.0).gap(4.0);
        for tab in tabs {
            sidebar = sidebar.child(tab_button(tab, s.tab == tab));
        }
        let content = match s.tab {
            SettingsTab::Recognition => {
                recognition_tab(s, &settings, &mut ctx, audio, language, model)
            }
            SettingsTab::Appearance => appearance_tab(s, &settings, &mut ctx, font),
            SettingsTab::About => about_tab(),
        };
        col()
            .key("settings")
            .w_full()
            .h_full()
            .fill(t.bg)
            .child(
                row()
                    .h(48.0)
                    .w_full()
                    .px(16.0)
                    .gap(12.0)
                    .center_y()
                    .justify_between()
                    .child(text("Settings").font(type_::TITLE).color(t.text))
                    .child(button("Done", done)),
            )
            .child(
                row()
                    .grow(1.0)
                    .w_full()
                    .px(16.0)
                    .pb(16.0)
                    .gap(16.0)
                    .child(sidebar)
                    .child(
                        col()
                            .grow(1.0)
                            .gap(16.0)
                            .scroll(ScrollSpec {
                                stick_bottom: false,
                            })
                            .child(content),
                    ),
            )
    };
    root(body, overlays)
}

fn tab_button(tab: SettingsTab, active: bool) -> El {
    let t = tok::theme(true);
    let label = match tab {
        SettingsTab::Recognition => "Recognition",
        SettingsTab::Appearance => "Appearance",
        SettingsTab::About => "About",
    };
    el().h(36.0)
        .px(12.0)
        .center_y()
        .radius(8.0)
        .fill(if active {
            tok::accent(0, true)
        } else {
            t.control_off
        })
        .on_press(move |s: &mut State| s.tab = tab)
        .child(
            text(label)
                .font(type_::LABEL)
                .color(if active { t.knob } else { t.text }),
        )
}

/// A dropdown built with Rune's Menu primitive: the menu lives in the overlay layer, anchored to
/// its trigger, with a scrim that dismisses it on outside clicks.
fn picker_row(
    ctx: &mut MenuCtx<'_>,
    key: &'static str,
    which: Picker,
    label: &str,
    value: &str,
    open: bool,
    items: Vec<Item>,
) -> El {
    let t = tok::theme(true);
    let trigger = row()
        .h(32.0)
        .gap(12.0)
        .center_y()
        .on_press(move |s: &mut State| {
            s.picker = if s.picker == Some(which) {
                None
            } else {
                Some(which)
            };
        })
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
        );

    let mut menu = Menu::new(key)
        .trigger(trigger)
        .open(open)
        .on_close(move |s: &mut State| {
            if s.picker == Some(which) {
                s.picker = None;
            }
        });
    for (label, checked, select) in items {
        menu = menu.item(MenuItem::new(label).checked(checked).on_select(select));
    }
    menu.build(ctx.kit, ctx.overlays)
}

fn recognition_tab(
    s: &State,
    settings: &Settings,
    ctx: &mut MenuCtx<'_>,
    audio: Vec<Item>,
    language: Vec<Item>,
    model: Vec<Item>,
) -> El {
    let t = tok::theme(true);
    let active = settings.model_id.as_deref().and_then(entry);
    let pending = s.pending_model.and_then(entry);
    let target = pending
        .or(active)
        .unwrap_or_else(|| santhra_models::default_for(Language::English));
    let installed = model_dir(target.id)
        .map(|dir| santhra_models::files_present(target, &dir))
        .unwrap_or(false);
    let download = { Shared::get().download.lock().unwrap().clone() };
    let (badge, status, progress_width) = match &download {
        DownloadStatus::Running {
            id, done, total, ..
        } if id == target.id => {
            let ratio = if *total == 0 {
                0.0
            } else {
                *done as f32 / *total as f32
            };
            (
                "DOWNLOADING",
                format!("{}% — keep this window open", (ratio * 100.0) as u32),
                (300.0 * ratio).max(2.0),
            )
        }
        DownloadStatus::Done(id) if id == target.id => ("READY", "Download verified".into(), 0.0),
        DownloadStatus::Failed(err) => ("ERROR", err.clone(), 0.0),
        _ if installed => ("READY", "Installed and verified".into(), 0.0),
        _ => ("NOT INSTALLED", "Download to use this model".into(), 0.0),
    };
    let active_label = active.map(|item| item.label).unwrap_or("Legacy folder");
    let needs_activate = pending.is_some_and(|item| Some(item.id) != active.map(|a| a.id));
    let mut actions = row().gap(8.0);
    if !installed {
        actions = actions.child(button("Download", download_selected));
    }
    if matches!(download, DownloadStatus::Running { .. }) {
        actions = actions.child(button("Cancel", cancel_download_clicked));
    }
    if needs_activate {
        actions = actions.child(button("Activate", activate_selected));
    }

    col()
        .gap(16.0)
        .child(
            col()
                .gap(8.0)
                .child(
                    row()
                        .gap(10.0)
                        .center_y()
                        .child(
                            el().h(22.0)
                                .px(8.0)
                                .radius(6.0)
                                .center_y()
                                .fill(if badge == "READY" {
                                    tok::accent(0, true)
                                } else {
                                    t.control_off
                                })
                                .child(text(badge).font(type_::SECTION).color(t.knob)),
                        )
                        .child(
                            text("Active model")
                                .font(type_::LABEL)
                                .color(t.text_secondary),
                        )
                        .child(text(active_label).font(type_::BODY).color(t.text)),
                )
                .child(
                    text(format!(
                        "{} • {} MB • buffer {} ms",
                        target.language.label(),
                        target.total_bytes() / 1_000_000,
                        settings.buffer_ms
                    ))
                    .font(type_::LABEL)
                    .color(t.text_secondary),
                )
                .child(text(status).font(type_::LABEL).color(t.text_secondary))
                .child(actions)
                .child(if progress_width > 0.0 {
                    progress_bar(progress_width, &t)
                } else {
                    el()
                }),
        )
        .child(
            col()
                .gap(4.0)
                .child(
                    text("Choose model")
                        .font(type_::SECTION)
                        .color(t.text_secondary),
                )
                .child(picker_row(
                    ctx,
                    "language-picker",
                    Picker::Language,
                    "Language",
                    settings.language.as_deref().unwrap_or("english"),
                    s.picker == Some(Picker::Language),
                    language,
                ))
                .child(picker_row(
                    ctx,
                    "model-picker",
                    Picker::Model,
                    "Model",
                    target.label,
                    s.picker == Some(Picker::Model),
                    model,
                ))
                .child(
                    text(if needs_activate {
                        "Press Activate to use the selected model."
                    } else {
                        "Changes to the active model apply on the next Start."
                    })
                    .font(type_::LABEL)
                    .color(t.text_secondary),
                ),
        )
        .child(section(
            "Audio",
            vec![
                picker_row(
                    ctx,
                    "source-picker",
                    Picker::Source,
                    "Audio source",
                    settings.target.as_deref().unwrap_or("Default sink"),
                    s.picker == Some(Picker::Source),
                    audio,
                ),
                button("Refresh model check", refresh_model),
                slider(
                    "threads",
                    "Threads",
                    settings.threads as f32,
                    1.0,
                    8.0,
                    set_threads,
                ),
                toggle_row("Start listening on launch", settings.start_on_launch, {
                    let value = settings.start_on_launch;
                    move |s| set_start_on_launch(s, !value)
                }),
            ],
        ))
}

fn progress_bar(width: f32, t: &tok::Theme) -> El {
    row()
        .gap(10.0)
        .center_y()
        .child(
            el().size(300.0, 8.0)
                .radius(4.0)
                .fill(t.control_off)
                .child(el().size(width, 8.0).radius(4.0).fill(tok::accent(0, true))),
        )
        .child(el())
}

fn appearance_tab(s: &State, settings: &Settings, ctx: &mut MenuCtx<'_>, font: Vec<Item>) -> El {
    col()
        .gap(16.0)
        .child(section(
            "Caption text",
            vec![
                labelled(
                    "Font search",
                    text_field("font-search", &s.font_query, |s| &mut s.font_query),
                ),
                picker_row(
                    ctx,
                    "font-picker",
                    Picker::Font,
                    "Font",
                    settings.font_family.as_deref().unwrap_or("System default"),
                    s.picker == Some(Picker::Font),
                    font,
                ),
                slider(
                    "font-size",
                    "Font size",
                    settings.font_size,
                    12.0,
                    72.0,
                    set_font_size,
                ),
                labelled(
                    "Text colour",
                    text_field("text-colour", &s.text_hex, |s| &mut s.text_hex),
                ),
                background_row(settings),
                slider(
                    "lines",
                    "Caption lines",
                    settings.max_lines as f32,
                    1.0,
                    5.0,
                    set_lines,
                ),
                toggle_row("Show partial results", settings.show_partial, {
                    let value = settings.show_partial;
                    move |s| set_show_partial(s, !value)
                }),
            ],
        ))
        .child(section(
            "Overlay",
            vec![
                toggle_row(
                    "Show floating overlay",
                    settings.overlay_enabled,
                    toggle_overlay,
                ),
                toggle_row(
                    "Always on top (X11 only)",
                    settings.overlay_always_on_top,
                    {
                        let value = settings.overlay_always_on_top;
                        move |s| set_always_on_top(s, !value)
                    },
                ),
                toggle_row("Click-through", settings.overlay_click_through, {
                    let value = settings.overlay_click_through;
                    move |s| set_click_through(s, !value)
                }),
                slider(
                    "opacity",
                    "Opacity",
                    settings.overlay_opacity,
                    0.1,
                    1.0,
                    set_opacity,
                ),
                slider(
                    "width",
                    "Width (on open)",
                    settings.overlay_width,
                    240.0,
                    1600.0,
                    set_width,
                ),
                slider(
                    "height",
                    "Height (on open)",
                    settings.overlay_height,
                    80.0,
                    600.0,
                    set_height,
                ),
            ],
        ))
}

fn about_tab() -> El {
    let t = tok::theme(true);
    let mut licences = col().gap(4.0);
    for item in catalog() {
        licences = licences.child(
            text(format!(
                "{} — {} ({})",
                item.label, item.license, item.source
            ))
            .font(type_::LABEL)
            .color(t.text_secondary),
        );
    }
    section(
        "About",
        vec![
            col()
                .gap(6.0)
                .child(text("Santhra").font(type_::TITLE).color(t.text))
                .child(
                    text("Live system-audio captions with a floating overlay.")
                        .font(type_::BODY)
                        .color(t.text_secondary),
                ),
            col()
                .gap(6.0)
                .child(
                    text(format!("Config: {}", Settings::path().display()))
                        .font(type_::LABEL)
                        .color(t.text_secondary),
                )
                .child(
                    text("Model, thread and overlay size changes apply on the next start/open.")
                        .font(type_::LABEL)
                        .color(t.text_secondary),
                )
                .child(
                    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                        text(
                            "Wayland: always-on-top and position restore depend on the compositor.",
                        )
                    } else {
                        text("X11: always-on-top and overlay position are restored.")
                    }
                    .font(type_::LABEL)
                    .color(t.text_secondary),
                ),
            col()
                .gap(6.0)
                .child(
                    text("Model licences")
                        .font(type_::SECTION)
                        .color(t.text_secondary),
                )
                .child(licences),
        ],
    )
}

fn set_language(s: &mut State, id: &'static str) {
    let shared = Shared::get();
    {
        let mut settings = shared.settings.lock().unwrap();
        settings.language = Some(id.to_owned());
        if let Some(language) = Language::parse(id) {
            let current = settings.model_id.as_deref().and_then(entry);
            if current.is_none_or(|item| item.language != language) {
                let default = santhra_models::default_for(language);
                settings.model_id = Some(default.id.into());
                settings.model_kind = Some(default.kind.as_str().into());
                settings.model_dir = model_dir(default.id)
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_else(|_| settings.model_dir.clone());
            }
        }
    }
    s.picker = None;
    s.pending_model = None;
    shared.publish();
}

fn select_model(s: &mut State, id: &'static str) {
    s.pending_model = Some(id);
    s.picker = None;
}

fn target_model(s: &State) -> &'static santhra_models::ModelEntry {
    let settings = Shared::get().settings();
    s.pending_model
        .and_then(entry)
        .or_else(|| settings.model_id.as_deref().and_then(entry))
        .unwrap_or_else(|| santhra_models::default_for(Language::English))
}

fn download_selected(s: &mut State) {
    start_download(target_model(s).id);
}

fn cancel_download_clicked(s: &mut State) {
    cancel_download();
    let _ = s;
}

fn activate_selected(s: &mut State) {
    let id = target_model(s).id;
    use_model(id);
    s.pending_model = None;
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
        if valid_hex(&s.text_hex.text) {
            settings.text_color = settings::to_hex(settings::parse_rgba(&s.text_hex.text));
        }
    }
    shared.apply_settings();
    s.picker = None;
    s.screen = Screen::Main;
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
