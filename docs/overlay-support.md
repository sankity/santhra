# Overlay support

The caption overlay is an undecorated eframe viewport. The app exposes every FR-05 control, but the
compositor decides which ones it honors.

| Feature | Wayland (winit 0.30) | X11 |
|---|---|---|
| Floating window | yes | yes |
| Drag to move | yes (`xdg_toplevel.move`) | yes |
| Resize by dragging an edge or corner | yes (`xdg_toplevel.resize`) | yes |
| Transparent background | yes | yes |
| Click-through | yes (empty `wl_surface` input region) | yes |
| Always-on-top | **no** (`set_window_level` is a no-op) | yes |
| Restore position | **no** (no window-position protocol) | yes |

Verified on GNOME/Wayland, Intel UHD 620, wgpu/Vulkan. The settings window warns about the two
Wayland gaps when `WAYLAND_DISPLAY` is set.

Scrolling compositors (Umbriel, niri) open every new window tiled, and a Wayland client cannot ask
to float, so the overlay would open as a scroll column. Add a window rule to float it:

```toml
[[window_rule]]
match.title = "^Santhra captions$"
default_floating = true
```

The app's requested size (720×180 by default) is used as the floating size, and the width/height
sliders keep working after that.

Captions are anchored to the bottom edge: when the text is taller than the overlay, the oldest
lines are clipped off the top so the newest words stay readable.

The overlay keeps updating when the main window is minimized, covered, or unfocused; it is a
deferred viewport and repaints independently of the main window.

Move and resize need pointer input, so both are unavailable while click-through is enabled.

To check the X11-only features: log into an X11 session and run
`cargo run -p santhra-desktop`, then enable the overlay and toggle always-on-top.
