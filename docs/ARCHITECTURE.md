# Architecture

Status: current implementation + target direction (Phase 0 baseline). User-facing documentation
is in `README.md`. This is the only developer document.

## 1. Current structure

| Crate | LOC | Owns / public API | Consumers | Depends on | Tests |
|---|---:|---|---|---|---:|
| `santhra-models` | 740 | Catalog, language metadata, data-dir resolution, SHA256/size verification: `catalog`, `entry`, `ModelKind`, `SpeechLanguage`, `matches_language_id`, `default_for_id`, `models_root`, `model_dir`, `verify_file`, `resolve` | `speech-sherpa`, `santhra-download`, CLI, desktop | `anyhow`, `sha2` | 7 |
| `speech-sherpa` | 565 | sherpa-onnx loading, streaming/offline ASR: `ModelFiles`, `AsrOptions`, `Transcriber`, `Events`, `transcribe` | `app-core`, CLI, desktop | `sherpa-onnx`, `santhra-models` | 4 |
| `audio-linux` | 498 | PipeWire capture, sink/app enumeration, mono downmix: `Target`, `Capture`, `spawn`, `list_targets`, `resolve_target`, `to_mono` | `app-core`, CLI, desktop | `pipewire`, `serde_json` | 8 |
| `caption-core` | 193 | Partial + last-N finals: `Captions`, `CaptionSnapshot`, `CaptionLine`, `CaptionEvent` | `app-core`, CLI, desktop | none | 6 |
| `app-core` | 189 | Capture + ASR threads, session lifecycle, messages: `Session`, `SessionOptions`, `Message` | CLI, desktop | `audio-linux`, `caption-core`, `speech-sherpa` | 1 |
| `santhra-download` | 151 | Downloads with progress/cancel: `download_all`, `Progress` | CLI, desktop | `santhra-models`, `ureq`, `sha2` | 2 |
| `santhra-cli` | 623 | `live`, `transcribe`, `transcribe-batch`, `models *` | end users | all core crates, `clap`, `serde_json` | via scripts |
| `santhra-desktop` | 2201 | Rune UI, settings, overlay, pickers | end users | all core crates, `rune`, `rune-kit`, `toml` | 15 |

Live path:

```text
target (default sink monitor, named sink monitor, or one app stream by object.serial)
  -> audio-linux capture thread: F32LE/16 kHz/mono negotiated by the graph
  -> bounded channel (64 x 100 ms)          drop + count on overflow
  -> app-core ASR thread: speech-sherpa OnlineRecognizer (greedy, endpointing on)
  -> caption-core: partial + last N finals
  -> CaptionSnapshot messages (GUI) or printed text (CLI)
```

Capture never blocks on inference; inference is only on the consumer side of the bounded
queue. The GUI polls `app-core::Session`, so the event loop runs neither capture nor inference.
Streaming models are transducers; Nemotron takes a per-stream `language` option which
`speech-sherpa` applies on every stream creation.

## 2. Dependency graph

```text
santhra-desktop ──> app-core ──> audio-linux (pipewire)
       │              │            caption-core
       │              └──────────> speech-sherpa ──> santhra-models
       │                                  │
       ├──> caption-core                  └──> sherpa-onnx 2.0.3
       ├──> speech-sherpa                       (sherpa-onnx-sys downloads prebuilt libs)
       ├──> santhra-models
       ├──> santhra-download ──> santhra-models, ureq
       └──> rune + rune-kit (git rev 4918969)

santhra-cli ──> app-core, audio-linux, caption-core, speech-sherpa,
                santhra-models, santhra-download, sherpa-onnx, clap
```

Platform-specific dependencies:

| Dependency | Scope | Notes |
|---|---|---|
| `pipewire` / `pipewire-sys` | Linux only | bindgen at build time |
| `pactl` | Linux runtime | app enumeration; missing binary → silently empty list |
| `rune`, `rune-kit` | desktop only | git rev `4918969` |
| `sherpa-onnx-sys` | all ASR consumers | downloads native libs into `target/sherpa-onnx-prebuilt` |
| `ureq` | `santhra-download` | HTTPS model downloads |

## 3. Target architecture

Layers: presentation (desktop, CLI, Android), application core (session/orchestration/events),
audio, speech, translation, transcript, platform adapters. Core crates must not depend on
desktop UI; platform code stays isolated; translation must not block ASR; capture must not
block on inference.

Workspace additions, in phase order:

| Crate | Phase | Trigger |
|---|---|---|
| `transcript-core`, `santhra-storage` | 1 | transcript domain + SQLite persistence |
| `translation-core` | 2 | translation provider + queue |
| `audio-core`, `audio-windows`, `audio-macos`, `audio-android` | 4–6 | second audio backend |
| `santhra-export` | 1+ | only if export outgrows `transcript-core` |
| `santhra-settings` | 5+ | see Decisions |

Not created: `santhra-core` (collides with `app-core`), `speech-language-id` (sherpa-onnx
already ships LID), or any crate whose only content is an unused trait.

## 4. Pinned dependency capabilities

Verified against `Cargo.lock` and `apps/desktop/Cargo.toml`; later phases reuse these APIs.

### Rune `rune` / `rune-kit` rev `4918969`

| Capability | API | Used | Phase |
|---|---|---|---|
| Floating overlay | `rune::Overlay`, `OverlaySpec`, `OverlayOptions`, `OverlayHandle` | no (current: `WindowSpec`) | 5 |
| Overlay placement/level/resize | `OverlayHandle::{place, resize, show, hide}`, `OverlayLevel`, `ContentSizing` | no | 5 |
| Overlay capability reporting | `OverlaySupport`, `Availability`, `OverlayCapabilities` | no | 5 |
| Tray | `rune::Tray`, `TraySpec`, `TrayEvent` | no | 5 |
| Global shortcuts | `El::global_shortcut`, `global_shortcut_named` | no | 5 |
| Notifications / i18n | `rune-notify`, `rune-i18n` (separate crates, same rev) | no | 5 |
| List virtualization | `El::virtualize`, `El::virtual_rows`, `El::virtual_items` | no | 1.3 |

Caveats: `drag_resize_window` is ignored in overlays; `OverlayHandle::resize` works
programmatically. No verified Rune UI path for Android exists in this rev.

### sherpa-onnx 2.0.3

| Capability | API | Used | Phase |
|---|---|---|---|
| Streaming/offline ASR | `OnlineRecognizer`, `OfflineRecognizer`, `RecognizerResult` | yes | 0 |
| Segment timestamps | `RecognizerResult::{timestamps, start_time, is_final, segment}`, `enable_token_timestamps` | no | 0.5 |
| VAD | `VoiceActivityDetector`, `VadModelConfig` (Silero, TEN-VAD) | no | 3.2 |
| Language ID | `SpokenLanguageIdentification` (result: `lang` only, **no confidence**) | no | 3.1 |
| Punctuation / resampling | `OfflinePunctuation`, `LinearResampler` | no | 3 |
| Diarization | `OfflineSpeakerDiarization` (offline only) | no | 3 (P2) |

`sherpa-onnx-sys` downloads prebuilt archives into `target/sherpa-onnx-prebuilt` at build
time (including Android JNI libs), so CI needs network on a cold cache. `DOCS_RS=1` skips it.

## 5. Contracts

### Events (current)

```rust
// speech-sherpa
pub struct Events { pub partial: Option<String>, pub finals: Vec<String> }

// caption-core
pub enum CaptionEvent { Partial(String), Final { id: u64, text: String } }
pub struct CaptionLine { pub id: u64, pub text: String }
pub struct CaptionSnapshot { pub partial: String, pub finals: Vec<CaptionLine> }

// app-core
pub enum Message {
    Captions(CaptionSnapshot),
    Stopped { dropped: u64 },
    Failed(String),
}
```

No time is carried today. Phase 0.5 will add `start_ms`, `end_ms`, status and optional
confidence to the segment type and propagate it through `caption-core` and `app-core`.

### ASR boundary (current)

```rust
pub struct AsrOptions {
    pub chunk_ms: u32,          // hardcoded to 100 in app-core today
    pub threads: i32,
    pub provider: String,
    pub buffer_ms: u32,
    pub language: Option<String>,
}
impl Transcriber {
    pub fn new(model: &ModelFiles, opts: &AsrOptions) -> anyhow::Result<Self>;
    pub fn push(&mut self, samples: &[f32]) -> Events;
    pub fn finish(&mut self) -> Events;
}
```

### Design sketches (not compiled; implemented with their phase)

```rust
// Audio — Phase 4, with audio-core
pub trait AudioBackend {
    fn capabilities(&self) -> AudioCapabilities;
    fn sources(&self) -> anyhow::Result<Vec<AudioSourceInfo>>;
    fn open(&self, source: &AudioSourceInfo) -> anyhow::Result<AudioStream>;
}
pub struct AudioCapabilities {
    pub system_audio: bool,
    pub per_application: bool,
    pub microphone: bool,
    pub resampling: bool,
}
// AudioStream must use a bounded queue and expose a dropped-frame counter.

// Translation — Phase 2, with translation-core
pub trait TranslationProvider {
    fn metadata(&self) -> &TranslationModelMetadata;
    fn translate(&mut self, request: TranslationRequest) -> anyhow::Result<TranslationResult>;
}
pub struct TranslationRequest {
    pub segment_id: String,
    pub source_language: String,
    pub target_language: String,
    pub text: String,
}

// Transcript — Phase 1, with transcript-core
pub struct TranscriptSession {
    pub id: String, pub title: String, pub created_at: i64, pub updated_at: i64,
    pub source_language: Option<String>, pub target_language: Option<String>,
    pub model_id: String, pub source_id: String, pub ended_at: Option<i64>,
}
pub struct TranscriptSegment {
    pub id: String, pub session_id: String,
    pub source_text: String, pub translated_text: Option<String>,
    pub source_language: Option<String>, pub target_language: Option<String>,
    pub start_ms: u64, pub end_ms: Option<u64>, pub speaker_id: Option<String>,
    pub status: SegmentStatus, pub confidence: Option<f32>,
    pub translation_status: TranslationStatus, pub updated_at: i64, pub revision: u32,
}
pub enum SegmentStatus { Partial, Final }
pub enum TranslationStatus { NotRequested, Pending, Completed, Failed }
```

Partials stay in memory; only finals are persisted.

## 6. Known coupling and gaps

| Coupling | Impact | Disposition |
|---|---|---|
| `app-core` → `audio-linux` unconditional | workspace cannot compile on Windows/macOS | Phase 4 |
| `settings.rs` → `rune::Color`, `rune_kit::Hsv`, `audio_linux::Target`, `app_core::SessionOptions` | settings not reusable by CLI/Android | phase 5+, see Decisions |
| Legacy `Language` (4) beside `SpeechLanguage` (10) in the catalog | registry refactor for categories/translation | Phase 2/3 |
| `chunk_ms = 100` hardcoded in `app-core` | blocks latency presets | Phase 0.5 |
| `caption-core` has no time/status | blocks persistence | Phase 0.5 |
| Downloads have no HTTP range resume | interrupted file restarts at byte 0 | Phase 2.4 |

Also missing: transcript storage/history/search/export (Phase 1), translation (Phase 2),
VAD/LID/punctuation/diarization integration (Phase 3), Mac/Windows/Android audio (Phase 4–6).
`start_on_launch` is stored and toggled but never read. `list_targets()` blocks up to 2 s on
the UI thread. The overlay uses a transparent `WindowSpec`, not `rune::Overlay`.
`models list --json` has no category filter, so non-ASR models would break the evaluator.

## 7. Decisions

1. **Modular core, platform adapters, pluggable engines (Phase 0).** Adopt the target layering,
   but no rewrite and no new crate in Phase 0. A crate appears only with a real second consumer:
   transcript/storage in Phase 1, translation in Phase 2, audio-core in Phase 4. Interface
   sketches above are design notes, not compiled code.
2. **Provider boundaries (Phase 0).** The current concrete types are the boundary: ASR =
   `speech-sherpa`, audio = `audio-linux`. Add `TranslationProvider`, `AudioBackend`, and
   transcript traits only with their phases and first consumer. No single-implementation traits.
3. **Settings stay in `santhra-desktop` (Phase 0).** Extraction requires: colours handled as
   plain hex outside the UI (already persisted as hex), `SessionOptions` moved to a UI-free
   domain crate, and a real second reader of the config file.
4. **Adopt `rune::Overlay` as the overlay target (decided Phase 0, migrate Phase 5).** It gives
   layer-shell Wayland placement, level/click-through and capability reporting. Keep the
   current `WindowSpec` overlay until then; drag-resize is ignored in overlays, programmatic
   `OverlayHandle::resize` works.
5. **CI is Linux-only until the audio abstraction exists (Phase 0).** `app-core` depends on
   `audio-linux`, so Windows/macOS jobs move to Phase 4 with the platform adapters. Linux CI
   installs `libpipewire-0.3-dev`, `libspa-0.2-dev`, `libclang-dev`, `pkg-config`.
