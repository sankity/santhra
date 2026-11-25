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
santhra-cli live --language indonesia --model whisper-small-multilingual
```

Downloads come from official sherpa-onnx Hugging Face repositories into
`~/.local/share/santhra/models/<id>/` and are checked against SHA256 + size from the bundled
manifest. The same manager is available in the desktop app under Settings → Model, where you can
pick a language and one of its compatible models, watch progress, and cancel a download. Whisper
Tiny, Base and Small multilingual checkpoints are available for supported languages; larger
checkpoints trade disk/RAM and latency for a stronger model. Offline models buffer about
2.5 s and do not show partial captions. Model licences and source repositories are listed in
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
bash scripts/live-smoke.sh    # plays a sample WAV through the default sink and checks the caption
```

Model-backed tests are ignored by default. To run one batch across all languages with verified
FLEURS references and all catalog models compatible with each language, build the CLI and run one
evaluation command:

```bash
cargo build -p santhra-cli
python3 scripts/test-asr-language.py --all-languages --all-models --samples 3 \
  --yes-download-large --download-missing-models
```

The runner prints `CORRECT`/`INCORRECT` for each utterance and a model comparison ranked by CER
(lower is better). All 10 enabled languages have verified FLEURS references. This full batch is a large job:
`--download-missing-models` downloads compatible models that are not installed, and
`--yes-download-large` allows downloading the FLEURS test archives. Both require time, network, and
disk space. After the first run, cached audio and installed models are reused. Remove
`--download-missing-models` to test only installed models. Use `--samples N` to choose how many
utterances per language; `--model <id>` instead of `--all-models` tests one model for one language.
A `CORRECT` match ignores case, punctuation, and whitespace differences; FLEURS is read speech, not
songs, so it does not measure lyric recognition. The supported language list is intentionally limited
to English, Mandarin Chinese, Spanish, Hindi, Indonesian, French, German, Japanese, Portuguese, and
Russian; each has an explicit FLEURS test mapping.

The earlier model-backed loader/inference tests can also be run with:

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
