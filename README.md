# Santhra

Offline live captions for system audio on Linux, built with PipeWire and sherpa-onnx. Santhra
captures what your speakers play — no virtual microphone, no cloud service — and turns it into
streaming captions in a terminal or a desktop window.

Linux is the only target for now; Windows and Android are planned.

## Features

- Real-time partial captions plus the last N finalized lines.
- CLI for terminal use, desktop GUI with an optional floating overlay.
- Everything runs locally: no network calls at runtime.
- Caption state, capture, and inference are separate crates (see `docs/architecture.md`).

## Requirements

- Linux with a running PipeWire session.
- Rust 1.85+ (the workspace uses edition 2024).
- PipeWire development headers, for example `pipewire` on Arch and `libpipewire-0.3-dev`
  plus `pkg-config` on Debian/Ubuntu.
- An internet connection for the first build: `sherpa-onnx-sys` downloads its prebuilt
  libraries, and the speech model is fetched separately.

## Setup

```bash
git clone https://github.com/sankity/santhra.git
cd santhra
scripts/fetch-model.sh        # ~46 MB, streaming Zipformer English, Apache-2.0
```

## Usage

Terminal captions from the default output:

```bash
cargo run -p santhra-cli -- live
```

Other CLI entry points:

```bash
cargo run -p santhra-cli -- live --target <sink-name>   # capture a specific sink
cargo run -p santhra-cli -- transcribe path/to/file.wav  # transcribe a 16 kHz mono WAV
```

Useful flags: `--no-partial` (final segments only), `--max-lines <n>`, `--model-dir <path>`,
`--threads <n>`.

Desktop app:

```bash
cargo run -p santhra-desktop
```

Settings live in `~/.config/santhra/config.toml` and are written by the app. Delete that file
to reset. The caption font can be picked from the installed system fonts (fontconfig); fonts
dropped into `~/.local/share/fonts` show up after `fc-cache -f`.

### Floating overlay

The overlay is an undecorated, translucent caption window rendered by Rune. On Wayland it can
float, be dragged, and click through, but always-on-top and position restore are not supported by
winit. Overlay width and height are set in Settings and applied when the overlay opens; live
edge-resize is not available. `docs/overlay-support.md` has the full capability matrix.

## Testing

```bash
cargo test --all
bash scripts/live-smoke.sh    # plays a sample WAV through the default sink and checks the caption
```

Model-backed tests are ignored by default:

```bash
SANTHRA_MODEL_DIR=models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17 \
  cargo test -p speech-sherpa -- --ignored
```

## Documentation

- `docs/architecture.md` — crate boundaries and the live audio path.
- `docs/overlay-support.md` — overlay behaviour per backend.
- `docs/model-licenses.md` — speech model sources and licenses.

## License

GPL-3.0-or-later. The speech model and sherpa-onnx are Apache-2.0; details in
`docs/model-licenses.md`.
