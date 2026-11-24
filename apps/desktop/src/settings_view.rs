use rune::tok::{self, type_};
use rune::{Cx, El, ScrollSpec, Tx, col, row, text};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Combobox, ComboboxOption, Field, Kit,
    Progress, SidebarRow, Slider, Switch, TextInput, Toolbar, root,
};
use santhra_models::{Language, catalog, entry, model_dir};

use crate::app::{
    DownloadStatus, FontChoice, Screen, SettingsTab, Shared, State, cancel_download, leaked_family,
    refresh_model, start_download, toggle_overlay, use_model,
};
use crate::fonts;
use crate::settings::{self, Settings};

pub fn view_settings(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let shared = Shared::get();
    let settings = shared.settings();
    let sinks = {
        let mut guard = shared.sinks.lock().unwrap();
        if guard.is_none() {
            *guard = Some(audio_linux::list_sinks());
        }
        guard.clone().unwrap_or_default()
    };
    let kit = Kit::neutral(true);
    let mut overlays = Vec::new();
    let content = match s.tab {
        SettingsTab::Recognition => recognition_tab(s, &settings, &kit, &sinks, &mut overlays),
        SettingsTab::Appearance => appearance_tab(s, &settings, &kit, &mut overlays),
        SettingsTab::About => about_tab(&kit),
    };
    let body = col()
        .key("settings")
        .w_full()
        .h_full()
        .fill(t.bg)
        .child(
            Toolbar::new()
                .title("Settings")
                .trailing(
                    Button::new("Done")
                        .variant(ButtonVariant::Contrast)
                        .on_press(done)
                        .build(&kit),
                )
                .build(&kit),
        )
        .child(
            row()
                .grow(1.0)
                .w_full()
                .px(16.0)
                .pb(16.0)
                .gap(16.0)
                .child(nav(s, &kit))
                .child(
                    col()
                        .grow(1.0)
                        .gap(16.0)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .child(content),
                ),
        );
    root(body, overlays)
}

fn nav(s: &State, kit: &Kit) -> El {
    col().w(150.0).gap(2.0).children(
        [
            SettingsTab::Recognition,
            SettingsTab::Appearance,
            SettingsTab::About,
        ]
        .into_iter()
        .map(|tab| {
            SidebarRow::new(tab_label(tab))
                .selected(s.tab == tab)
                .on_select(move |s: &mut State, _: &mut Tx| s.tab = tab)
                .build(kit)
        }),
    )
}

fn tab_label(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::Recognition => "Recognition",
        SettingsTab::Appearance => "Appearance",
        SettingsTab::About => "About",
    }
}

fn recognition_tab(
    s: &State,
    settings: &Settings,
    kit: &Kit,
    sinks: &[audio_linux::Sink],
    overlays: &mut Vec<El>,
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
    let (badge, variant, status, progress) = match &download {
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
                BadgeVariant::Subtle,
                format!("{}% — keep this window open", (ratio * 100.0) as u32),
                Some(ratio),
            )
        }
        DownloadStatus::Done(id) if id == target.id => (
            "READY",
            BadgeVariant::Accent,
            "Download verified".into(),
            None,
        ),
        DownloadStatus::Failed(err) => ("ERROR", BadgeVariant::Neutral, err.clone(), None),
        _ if installed => (
            "READY",
            BadgeVariant::Accent,
            "Installed and verified".into(),
            None,
        ),
        _ => (
            "NOT INSTALLED",
            BadgeVariant::Neutral,
            "Download to use this model".into(),
            None,
        ),
    };
    let active_label = active.map(|item| item.label).unwrap_or("Legacy folder");
    let needs_activate = pending.is_some_and(|item| Some(item.id) != active.map(|a| a.id));

    let mut actions = row().gap(8.0);
    if !installed {
        actions = actions.child(
            Button::new("Download")
                .on_press(download_selected)
                .build(kit),
        );
    }
    if matches!(download, DownloadStatus::Running { .. }) {
        actions = actions.child(
            Button::new("Cancel")
                .on_press(cancel_download_clicked)
                .build(kit),
        );
    }
    if needs_activate {
        actions = actions.child(
            Button::new("Activate")
                .variant(ButtonVariant::Contrast)
                .on_press(activate_selected)
                .build(kit),
        );
    }

    let mut summary = Card::new()
        .title("Active model", None)
        .description(format!(
            "{} • {} MB • buffer {} ms",
            target.language.label(),
            target.total_bytes() / 1_000_000,
            settings.buffer_ms
        ))
        .action(actions)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(Badge::new(badge).variant(variant).build(kit))
                .child(text(active_label).font(type_::BODY).color(t.text)),
        )
        .child(text(status).font(type_::LABEL).color(t.text_secondary));
    if let Some(ratio) = progress {
        summary = summary.child(
            Progress::new(Some(ratio))
                .label("Downloading")
                .show_value()
                .build(kit),
        );
    }

    let language_value = settings
        .language
        .clone()
        .unwrap_or_else(|| Language::English.as_str().to_owned());
    let language_options: Vec<ComboboxOption> = Language::ALL
        .into_iter()
        .map(|lang| ComboboxOption::new(lang.as_str(), lang.label()))
        .collect();
    let language_picker = Combobox::new(
        "language",
        &s.language_combo,
        &language_options,
        |s: &mut State| &mut s.language_combo,
    )
    .label("Language")
    .current(Some(language_value.as_str()))
    .on_choose(|s, id, _| {
        if let Some(lang) = Language::ALL.into_iter().find(|lang| lang.as_str() == id) {
            set_language(s, lang.as_str());
        }
    })
    .build(kit, overlays);

    let language = settings.language.as_deref().and_then(Language::parse);
    let mut model_options: Vec<ComboboxOption> = catalog()
        .iter()
        .filter(|item| {
            language.is_none_or(|lang| santhra_models::matches_language(item.language, lang))
        })
        .map(|item| ComboboxOption::new(item.id, item.label))
        .collect();
    if !model_options.iter().any(|option| option.id == target.id) {
        model_options.push(ComboboxOption::new(target.id, target.label));
    }
    let model_picker = Combobox::new("model", &s.model_combo, &model_options, |s: &mut State| {
        &mut s.model_combo
    })
    .label("Model")
    .current(Some(target.id))
    .on_choose(|s, id, _| {
        if let Some(item) = entry(id) {
            select_model(s, item.id);
        }
    })
    .build(kit, overlays);

    let mut source_options = vec![ComboboxOption::new("default", "Default sink")];
    for sink in sinks {
        source_options.push(ComboboxOption::new(
            sink.name.clone(),
            sink.description.clone(),
        ));
    }
    if let Some(current) = settings.target.as_deref()
        && !sinks.iter().any(|sink| sink.name == current)
    {
        source_options.push(ComboboxOption::new(current.to_owned(), current.to_owned()));
    }
    let source_picker = Combobox::new(
        "source",
        &s.source_combo,
        &source_options,
        |s: &mut State| &mut s.source_combo,
    )
    .label("Audio source")
    .current(Some(settings.target.as_deref().unwrap_or("default")))
    .on_choose(|s, id, _| {
        set_source(
            s,
            if id == "default" {
                None
            } else {
                Some(id.to_owned())
            },
        );
    })
    .build(kit, overlays);

    col()
        .gap(16.0)
        .child(summary.build(kit))
        .child(
            Card::new()
                .title("Choose model", None)
                .child(labelled(kit, "Language", language_picker))
                .child(labelled(kit, "Model", model_picker))
                .child(
                    text(if needs_activate {
                        "Press Activate to use the selected model."
                    } else {
                        "Changes to the active model apply on the next Start."
                    })
                    .font(type_::LABEL)
                    .color(t.text_secondary),
                )
                .build(kit),
        )
        .child(
            Card::new()
                .title("Audio", None)
                .child(labelled(kit, "Audio source", source_picker))
                .child(
                    row().child(
                        Button::new("Refresh model check")
                            .on_press(refresh_model)
                            .build(kit),
                    ),
                )
                .child(
                    Slider::new("threads", settings.threads as f32)
                        .range(1.0..=8.0)
                        .step(1.0)
                        .label("Threads")
                        .on_change(|s: &mut State, value, _| set_threads(s, value))
                        .build(kit),
                )
                .child(
                    Switch::new(settings.start_on_launch)
                        .label("Start listening on launch")
                        .on_toggle({
                            let value = settings.start_on_launch;
                            move |s: &mut State| set_start_on_launch(s, !value)
                        })
                        .build(kit),
                )
                .build(kit),
        )
}

fn appearance_tab(s: &State, settings: &Settings, kit: &Kit, overlays: &mut Vec<El>) -> El {
    let families = fonts::families();
    let current_font = settings.font_family.as_deref().unwrap_or("default");
    let mut font_options: Vec<ComboboxOption> =
        vec![ComboboxOption::new("default", "System default")];
    for name in families {
        font_options.push(ComboboxOption::new(name.clone(), name.clone()));
    }
    if current_font != "default" && !families.iter().any(|name| name == current_font) {
        font_options.insert(1, ComboboxOption::new(current_font, current_font));
    }
    let font_picker = Combobox::new("font", &s.font_combo, &font_options, |s: &mut State| {
        &mut s.font_combo
    })
    .label("Font")
    .current(Some(current_font))
    .search_placeholder("Search fonts")
    .on_choose(|s, id, _| {
        set_font(
            s,
            if id == "default" {
                None
            } else {
                Some(id.to_owned())
            },
        );
    })
    .build(kit, overlays);

    let mut background_options: Vec<ComboboxOption> = settings::BACKGROUND_PRESETS
        .iter()
        .map(|(name, hex)| ComboboxOption::new(*hex, *name))
        .collect();
    let current_background = settings.background_color.clone();
    if !background_options
        .iter()
        .any(|option| option.id == current_background)
    {
        background_options.insert(0, ComboboxOption::new(current_background.clone(), "Custom"));
    }
    let background_picker = Combobox::new(
        "background",
        &s.background_combo,
        &background_options,
        |s: &mut State| &mut s.background_combo,
    )
    .label("Background")
    .current(Some(current_background.as_str()))
    .on_choose(|s, id, _| {
        if let Some((_, value)) = settings::BACKGROUND_PRESETS
            .iter()
            .find(|(_, value)| *value == id)
        {
            set_background(s, value);
        }
    })
    .build(kit, overlays);

    let text_colour = Field::new()
        .label("Text colour")
        .control(
            TextInput::new(&s.text_hex)
                .label("Text colour")
                .on_edit(|s: &mut State| &mut s.text_hex)
                .build(kit),
        )
        .build(kit);

    col()
        .gap(16.0)
        .child(
            Card::new()
                .title("Caption text", None)
                .child(labelled(kit, "Font", font_picker))
                .child(
                    Slider::new("font-size", settings.font_size)
                        .range(12.0..=72.0)
                        .step(1.0)
                        .label("Font size")
                        .on_change(|s: &mut State, value, _| set_font_size(s, value))
                        .build(kit),
                )
                .child(text_colour)
                .child(labelled(kit, "Background", background_picker))
                .child(
                    Slider::new("lines", settings.max_lines as f32)
                        .range(1.0..=5.0)
                        .step(1.0)
                        .label("Caption lines")
                        .on_change(|s: &mut State, value, _| set_lines(s, value))
                        .build(kit),
                )
                .child(
                    Switch::new(settings.show_partial)
                        .label("Show partial results")
                        .on_toggle({
                            let value = settings.show_partial;
                            move |s: &mut State| set_show_partial(s, !value)
                        })
                        .build(kit),
                )
                .build(kit),
        )
        .child(
            Card::new()
                .title("Overlay", None)
                .child(
                    Switch::new(settings.overlay_enabled)
                        .label("Show floating overlay")
                        .on_toggle(toggle_overlay)
                        .build(kit),
                )
                .child(
                    Switch::new(settings.overlay_always_on_top)
                        .label("Always on top (X11 only)")
                        .on_toggle({
                            let value = settings.overlay_always_on_top;
                            move |s: &mut State| set_always_on_top(s, !value)
                        })
                        .build(kit),
                )
                .child(
                    Switch::new(settings.overlay_click_through)
                        .label("Click-through")
                        .on_toggle({
                            let value = settings.overlay_click_through;
                            move |s: &mut State| set_click_through(s, !value)
                        })
                        .build(kit),
                )
                .child(
                    Slider::new("opacity", settings.overlay_opacity)
                        .range(0.1..=1.0)
                        .step(0.05)
                        .label("Opacity")
                        .on_change(|s: &mut State, value, _| set_opacity(s, value))
                        .build(kit),
                )
                .child(
                    Slider::new("width", settings.overlay_width)
                        .range(240.0..=1600.0)
                        .step(10.0)
                        .label("Width (on open)")
                        .on_change(|s: &mut State, value, _| set_width(s, value))
                        .build(kit),
                )
                .child(
                    Slider::new("height", settings.overlay_height)
                        .range(80.0..=600.0)
                        .step(10.0)
                        .label("Height (on open)")
                        .on_change(|s: &mut State, value, _| set_height(s, value))
                        .build(kit),
                )
                .build(kit),
        )
}

fn about_tab(kit: &Kit) -> El {
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
    col()
        .gap(16.0)
        .child(
            Card::new()
                .title("About", None)
                .child(text("Santhra").font(type_::TITLE).color(t.text))
                .child(
                    text("Live system-audio captions with a floating overlay.")
                        .font(type_::BODY)
                        .color(t.text_secondary),
                )
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
                )
                .build(kit),
        )
        .child(
            Card::new()
                .title("Model licences", None)
                .child(licences)
                .build(kit),
        )
}

fn labelled(kit: &Kit, label: &str, control: El) -> El {
    col()
        .gap(8.0)
        .w_full()
        .items_start()
        .child(
            text(label)
                .font(kit.ty.ui_medium.lh(16.0))
                .color(kit.c.text_base),
        )
        .child(control)
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
    s.pending_model = None;
    shared.publish();
}

fn select_model(s: &mut State, id: &'static str) {
    s.pending_model = Some(id);
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
    s.font_combo.close();
    s.source_combo.close();
    s.language_combo.close();
    s.model_combo.close();
    s.background_combo.close();
    shared.apply_settings();
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

fn set_font(_s: &mut State, name: Option<String>) {
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
    shared.publish();
}

fn set_source(_s: &mut State, name: Option<String>) {
    let shared = Shared::get();
    shared.settings.lock().unwrap().target = name;
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
