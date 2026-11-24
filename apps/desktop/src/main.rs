mod app;
mod fonts;
mod overlay;
mod settings;
mod settings_view;
mod widgets;

use rune::{AppSpec, Tx, WindowOptions, WindowSpec};

use app::{Shared, State};

fn main() {
    rune::set_app_id("santhra");
    let settings = Shared::get().settings();
    let main = WindowSpec::new("main", main_spec())
        .state(State::new("main"))
        .on_close(|s, _: &mut Tx| {
            let _ = s;
            app::on_close();
        });
    let mut windows = vec![main];
    if settings.overlay_enabled {
        windows.push(overlay_spec().state(State::new("overlay")));
    }
    rune::run_windows(|_| windows);
}

fn main_spec() -> AppSpec<State> {
    AppSpec {
        setup: Some(app::setup),
        poll: Some(app::poll_main),
        scenes: &[
            ("captions", app::scene_captions),
            ("settings", app::scene_settings),
            ("font-picker", app::scene_font_picker),
            ("source-picker", app::scene_source_picker),
            ("settings-captions", app::scene_settings_captions),
            ("settings-overlay", app::scene_settings_overlay),
            ("settings-about", app::scene_settings_about),
            ("settings-model", app::scene_settings_model),
            ("language-picker", app::scene_language_picker),
            ("model-picker", app::scene_model_picker),
        ],
        ..AppSpec::new(
            "Santhra",
            (900.0, 600.0),
            || State::new("main"),
            app::view_main,
        )
    }
}

fn overlay_spec() -> WindowSpec<State> {
    let settings = Shared::get().settings();
    let spec = AppSpec {
        setup: Some(app::setup),
        poll: Some(app::poll_readonly),
        scenes: &[("captions", app::scene_captions)],
        ..AppSpec::new(
            "Santhra captions",
            (settings.overlay_width, settings.overlay_height),
            || State::new("overlay"),
            overlay::view_overlay,
        )
    };
    WindowSpec::new("overlay", spec)
        .on_close(|s, _: &mut Tx| {
            let _ = s;
            let shared = Shared::get();
            if std::env::var_os("WAYLAND_DISPLAY").is_none() {
                let handle = { shared.overlay.lock().unwrap().clone() };
                if let Some(place) = handle.and_then(|h| h.placement()) {
                    shared.settings.lock().unwrap().overlay_pos =
                        Some([place.position.0 as f32, place.position.1 as f32]);
                    let settings = shared.settings();
                    let _ = settings.save();
                }
            }
            *shared.overlay.lock().unwrap() = None;
        })
        .options(WindowOptions {
            transparent: true,
            borderless: true,
            always_on_top: settings.overlay_always_on_top,
            click_through: settings.overlay_click_through,
            resizable: true,
            min_size: Some((240.0, 80.0)),
            max_size: Some((1600.0, 600.0)),
            ..Default::default()
        })
}

pub(crate) fn apply_overlay_options() {
    let shared = Shared::get();
    let settings = shared.settings();
    let handle = { shared.overlay.lock().unwrap().clone() };
    if let Some(handle) = handle {
        handle.set_always_on_top(settings.overlay_always_on_top);
        if let Err(err) = handle.set_click_through(settings.overlay_click_through) {
            eprintln!("click-through: {err}");
        }
    }
}

pub(crate) fn set_overlay_enabled(enabled: bool) {
    let shared = Shared::get();
    {
        let mut settings = shared.settings.lock().unwrap();
        if settings.overlay_enabled == enabled {
            return;
        }
        settings.overlay_enabled = enabled;
    }
    let settings = shared.settings();
    let _ = settings.save();
    if enabled {
        let handle = rune::open_window(overlay_spec());
        *shared.overlay.lock().unwrap() = Some(handle);
    } else {
        let handle = { shared.overlay.lock().unwrap().take() };
        if let Some(handle) = handle {
            handle.close();
        }
    }
    shared.publish();
}
