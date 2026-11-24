# Overlay support

The caption overlay is an undecorated Rune window. The compositor decides which window controls
it honours.

| Feature | Wayland | X11 |
|---|---|---|
| Floating window | with a compositor window rule | yes |
| Drag to move | yes (`xdg_toplevel.move`) | yes |
| Resize by dragging an edge or corner | **no** | **no** |
| Transparent background | yes | yes |
| Click-through | yes (empty `wl_surface` input region) | yes |
| Always-on-top | **no** (`set_window_level` is a no-op) | yes |
| Restore position | **no** (no window-position protocol) | yes |

Overlay width and height are configured in Settings and applied when the overlay opens. They are
not applied live: Rune has no interactive-resize API and this compositor ignores client-requested
Wayland resizes. Changing either value takes effect after closing and reopening the overlay.

Verified on Umbriel/Wayland with Rune. The settings window
warns about the two Wayland gaps when `WAYLAND_DISPLAY` is set.

Scrolling compositors (Umbriel, niri) open every new window tiled, and a Wayland client cannot ask
to float, so the overlay would open as a scroll column. Add a window rule to float it:

```toml
[[window_rule]]
match.title = "^Santhra captions$"
default_floating = true
```

Captions are anchored to the bottom edge: when the text is taller than the overlay, the oldest
lines are clipped off the top so the newest words stay readable.

The overlay keeps updating when the main window is minimized, covered, or unfocused; it is an
independent Rune window with its own retained tree and repaint loop.

Move and click-through need pointer input: dragging moves the window only while click-through is
off.

To check the X11-only features: log into an X11 session and run
`cargo run -p santhra-desktop`, then enable the overlay and toggle always-on-top.
