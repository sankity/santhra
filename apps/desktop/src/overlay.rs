use rune::{Cx, El, ScrollSpec, col, el, text};

use crate::app::{Shared, background_rgba, rgba};
use crate::settings::Settings;

pub fn view_overlay(_: &crate::app::State, _: &mut Cx<crate::app::State>) -> El {
    let shared = Shared::get();
    let settings = shared.settings();
    let snapshot = shared.snapshot.lock().unwrap().clone();

    let mut body = col().w_full().padding(12.0).gap(6.0);
    if snapshot.finals.is_empty() && snapshot.partial.is_empty() {
        body = body.child(
            text("No captions yet.")
                .wrap()
                .font(shared.caption_style(false))
                .color(rgba(settings.text_rgba())),
        );
    } else {
        for line in &snapshot.finals {
            body = body.child(caption(&line.text, shared, &settings, false));
        }
        if settings.show_partial && !snapshot.partial.is_empty() {
            body = body.child(caption(&snapshot.partial, shared, &settings, true));
        }
    }

    let scroll = col()
        .key("overlay-scroll")
        .w_full()
        .h_full()
        .min_h(0.0)
        .scroll(ScrollSpec::stick_bottom())
        .clip_rounded(0.0)
        .child(body);

    col()
        .key("overlay")
        .w_full()
        .h_full()
        .fill(background_rgba(&settings))
        .child(el().w_full().h_full().window_drag().child(scroll))
}

fn caption(content: &str, shared: &Shared, settings: &Settings, italic: bool) -> El {
    text(content.to_owned())
        .wrap()
        .font(shared.caption_style(italic))
        .color(rgba(settings.text_rgba()))
}
