# Architecture

Live path:

```
default sink input (PipeWire, stream.capture.sink=true)
  -> capture thread: F32LE/16 kHz/mono negotiated by the graph
  -> bounded channel (64 x 100 ms)          drop + count on overflow
  -> main thread: sherpa-onnx OnlineRecognizer (greedy, endpointing on)
  -> partial / final text on stdout
```

Capture never blocks on inference; inference runs only on the consumer side of a bounded queue.
