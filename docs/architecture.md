# Architecture

Live path:

```
default sink input (PipeWire, stream.capture.sink=true)
  -> capture thread: F32LE/16 kHz/mono negotiated by the graph
  -> bounded channel (64 x 100 ms)          drop + count on overflow
  -> main thread: sherpa-onnx OnlineRecognizer (greedy, endpointing on)
  -> caption-core: partial + last N finals, emits CaptionEvent
  -> renderer (stdout now, egui/overlay later)
```

Capture never blocks on inference; inference runs only on the consumer side of a bounded queue.
