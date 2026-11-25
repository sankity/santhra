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
`--threads <n>`, `--language <language-id>` (from the Settings language list), `--model <id>`,
`--buffer-ms <n>`. The supported recognition languages are English, Mandarin Chinese, Spanish,
Hindi, Indonesian, French, German, Japanese, Portuguese, and Russian. Language IDs are passed to
the selected model; automatic language detection is not currently offered.

Bundled models can be listed, downloaded and verified from the CLI:

```bash
santhra-cli models list [--language <language-id>]  # e.g. english, chinese, indonesia
santhra-cli models download <id>
santhra-cli models verify <id>
santhra-cli live --language indonesia --model whisper-tiny-id
```

Downloads come from official sherpa-onnx Hugging Face repositories into
`~/.local/share/santhra/models/<id>/` and are checked against SHA256 + size from the bundled
manifest. The same manager is available in the desktop app under Settings → Model, where you can
pick a language and one of its compatible models, watch progress, and cancel a download. Whisper
Tiny, Base and Small multilingual checkpoints are available for supported languages; larger
checkpoints trade disk/RAM and latency for a stronger model. `whisper-tiny-id` is a dedicated
Whisper tiny Indonesian fine-tune (Apache-2.0, fp32) offered for Indonesian only and, like the
other Whisper models, offline. Offline models buffer about
2.5 s and do not show partial captions. `nemotron-3-5-asr-560ms` is a streaming multilingual
alternative (~682 MB, OpenMDW-1.1): it picks the language per stream, emits punctuation, and
refreshes partial captions every 560 ms. It covers 39 locales but not Indonesian, so Indonesian
keeps its Whisper checkpoints. Model licences and source repositories are listed in
Settings → About.

Desktop app:

```bash
cargo run -p santhra-desktop
```

Settings live in `~/.config/santhra/config.toml` and are written by the app. Delete that file
to reset. Recognition language is limited to English, Mandarin Chinese, Spanish, Hindi, Indonesian,
French, German, Japanese, Portuguese, and Russian. Older saved language choices outside this set
fall back to English, with a compatible default model selected if needed. Model choices remain
limited to compatible ASR models in Santhra's catalog. The
caption font can be picked from the installed system fonts (fontconfig); fonts
dropped into `~/.local/share/fonts` show up after `fc-cache -f`.

### Floating overlay

The overlay is an undecorated, translucent caption window rendered by Rune. On Wayland it can
float, be dragged, and click through, but always-on-top and position restore are not supported by
winit. Overlay width and height are set in Settings and applied when the overlay opens; live
edge-resize is not available. `docs/overlay-support.md` has the full capability matrix.

## Testing

```bash
cargo test --all
bash scripts/test-eval.sh     # level 0 + 1 ASR checks (no network; level 1 needs a local model)
bash scripts/live-smoke.sh    # plays a sample WAV through the default sink and checks the caption
```

ASR evaluation is documented in `docs/evaluation.md`. In short:

```bash
cargo build -p santhra-cli
python3 scripts/test-asr-language.py smoke --language english --samples 3
python3 scripts/test-asr-language.py compare --latest
python3 -m unittest scripts/test_asr_eval.py
```

Stages are `smoke`, `screening`, `accuracy`, `streaming`, and `compare`; results go to
`evaluation/runs/<run-id>/`. Model and dataset downloads are always opt-in
(`--download-missing-models`, `--yes-download-large`).

A `CORRECT` match ignores case, punctuation, and whitespace differences. FLEURS is read
speech, not songs, so it does not measure lyric recognition. The supported language list
is intentionally limited to English, Mandarin Chinese, Spanish, Hindi, Indonesian,
French, German, Japanese, Portuguese, and Russian; each has an explicit FLEURS test
mapping and only compatible catalog models are tested.

## Documentation

- `docs/architecture.md` — crate boundaries and the live audio path.
- `docs/overlay-support.md` — overlay behaviour per backend.
- `docs/model-licenses.md` — speech model sources and licenses.

## License

GPL-3.0-or-later. The speech model and sherpa-onnx are Apache-2.0; details in
`docs/model-licenses.md`.
