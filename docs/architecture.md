# Architecture

Crates:

| Crate | Owns |
|---|---|
| `audio-linux` | PipeWire capture (system audio), sink enumeration |
| `speech-sherpa` | sherpa-onnx model loading and streaming transcription |
| `caption-core` | caption state: partial text + last N finals, events |
| `app-core` | capture + ASR threads, session lifecycle, messages |

Live path:

```text
default sink input (PipeWire, stream.capture.sink=true)
  -> audio-linux capture thread: F32LE/16 kHz/mono negotiated by the graph
  -> bounded channel (64 x 100 ms)          drop + count on overflow
  -> app-core ASR thread: speech-sherpa OnlineRecognizer (greedy, endpointing on)
  -> caption-core: partial + last N finals
  -> CaptionSnapshot messages (GUI) or printed text (CLI)
```

Capture never blocks on inference; inference runs only on the consumer side of a bounded queue.
The GUI polls messages from `app-core::Session`, so the event loop never runs capture or inference.

Streaming models are transducers. The multilingual NeMo model (Nemotron) also takes a per-stream
`language` option; `speech-sherpa` applies it every time it creates a stream.

The desktop app renders one `CaptionSnapshot` on two surfaces: the main window and an optional
floating overlay viewport (`docs/overlay-support.md` lists which window features each backend honors).
