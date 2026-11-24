use rune::tok::{self, type_};
use rune::{Drag, El, Phase, TextEdit, col, el, row, text, text_input};

use crate::app::State;

pub fn button(label: impl Into<String>, press: fn(&mut State)) -> El {
    let t = tok::theme(true);
    el().h(30.0)
        .px(12.0)
        .center_y()
        .radius(8.0)
        .fill(t.control_off)
        .hover(t.hover)
        .press(t.press)
        .focusable(true)
        .on_press(press)
        .child(text(label).font(type_::LABEL).color(t.text))
}

pub fn toggle_row(label: &str, on: bool, flip: impl Fn(&mut State) + 'static) -> El {
    let t = tok::theme(true);
    let knob = el()
        .absolute(if on { 18.0 } else { 2.0 }, 2.0)
        .size(18.0, 18.0)
        .radius(9.0)
        .fill(t.knob);
    row()
        .h(32.0)
        .gap(12.0)
        .center_y()
        .role("switch")
        .checked(on)
        .focusable(true)
        .on_press(flip)
        .child(text(label).font(type_::BODY).color(t.text))
        .child(
            el().size(38.0, 22.0)
                .radius(11.0)
                .fill(if on {
                    tok::accent(0, true)
                } else {
                    t.control_off
                })
                .child(knob),
        )
}

/// A slider with no rune-kit dependency. `set` receives a value in `[min, max]`.
pub fn slider(
    key: &'static str,
    label: &str,
    v: f32,
    min: f32,
    max: f32,
    set: fn(&mut State, f32),
) -> El {
    let t = tok::theme(true);
    let (w, h, k, bar) = (200.0, 22.0, 16.0, 4.0);
    let travel = w - k;
    let at = ((v - min) / (max - min)).clamp(0.0, 1.0);
    row()
        .gap(10.0)
        .center_y()
        .child(
            text(label)
                .w(112.0)
                .font(type_::LABEL)
                .color(t.text_secondary),
        )
        .child(
            el().key(key)
                .role("slider")
                .label(label)
                .range_value(f64::from(at), 0.0, 1.0)
                .range_step(0.05)
                .on_step(move |s: &mut State, steps, _| {
                    set(s, (v + steps as f32 * (max - min) / 20.0).clamp(min, max));
                })
                .on_set_value(move |s: &mut State, value, _| {
                    set(s, min + value as f32 * (max - min));
                })
                .size(w, h)
                .on_drag(move |s: &mut State, d: &mut Drag, tx| match d.phase {
                    Phase::Down => d.data = at,
                    Phase::Move => {
                        let knob = k / 2.0 + d.data * travel;
                        let grab = if (d.start_x - knob).abs() <= k / 2.0 + 2.0 {
                            d.start_x - knob
                        } else {
                            0.0
                        };
                        let at = ((d.x - grab - k / 2.0) / travel).clamp(0.0, 1.0);
                        set(s, min + at * (max - min));
                        tx.instant();
                    }
                    Phase::Up => {}
                })
                .child(
                    el().size(w, bar)
                        .absolute(0.0, (h - bar) / 2.0)
                        .radius(bar / 2.0)
                        .fill(t.control_off),
                )
                .child(
                    el().size(k / 2.0 + at * travel, bar)
                        .absolute(0.0, (h - bar) / 2.0)
                        .radius(bar / 2.0)
                        .fill(tok::accent(0, true)),
                )
                .child(
                    el().size(k, k)
                        .absolute(at * travel, (h - k) / 2.0)
                        .radius(k / 2.0)
                        .fill(t.knob),
                ),
        )
        .child(
            text(format!("{v:.0}"))
                .w(52.0)
                .font(type_::MONO_CAPTION)
                .color(t.text_secondary),
        )
}

pub fn text_field(
    key: &'static str,
    edit: &TextEdit,
    access: impl Fn(&mut State) -> &mut TextEdit + 'static,
) -> El {
    let t = tok::theme(true);
    el().key(key)
        .h(32.0)
        .w_full()
        .px(8.0)
        .radius(8.0)
        .fill(t.control_off)
        .center_y()
        .child(
            text_input(edit, type_::BODY)
                .single_line()
                .color(t.text)
                .on_edit(access),
        )
}

pub fn section(title: &str, children: Vec<El>) -> El {
    let t = tok::theme(true);
    col()
        .gap(8.0)
        .child(text(title).font(type_::SECTION).color(t.text_secondary))
        .children(children)
}
