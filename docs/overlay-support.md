# Overlay support

The caption overlay is an undecorated eframe viewport. The app exposes every FR-05 control, but the
compositor decides which ones it honors.

| Feature | Wayland (winit 0.30) | X11 |
|---|---|---|
| Floating window | yes | yes |
| Drag to move | yes (`xdg_toplevel.move`) | yes |
| Transparent background | yes | yes |
| Click-through | yes (empty `wl_surface` input region) | yes |
| Always-on-top | **no** (`set_window_level` is a no-op) | yes |
| Restore position | **no** (no window-position protocol) | yes |

Verified on GNOME/Wayland, Intel UHD 620, wgpu/Vulkan. The settings window warns about the two
Wayland gaps when `WAYLAND_DISPLAY` is set.

To check the X11-only features: log into an X11 session and run
`cargo run -p santhra-desktop`, then enable the overlay and toggle always-on-top.
