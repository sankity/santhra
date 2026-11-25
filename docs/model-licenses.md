# Model licenses

| Model | Source | License | Size | Files used |
|---|---|---|---|---|
| Streaming Zipformer English 20M INT8 (`sherpa-onnx-streaming-zipformer-en-20M-2023-02-17`) | https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models (mirror: `csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17` on HF) | Apache-2.0 per model card | ~46 MB (int8 encoder/joiner + decoder + tokens) | `encoder-epoch-99-avg-1.int8.onnx`, `decoder-epoch-99-avg-1.onnx`, `joiner-epoch-99-avg-1.int8.onnx`, `tokens.txt` |
| Offline Paraformer Chinese INT8 (`paraformer-zh-offline`) | https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2023-03-28 | MIT per model card | ~223 MB | `model.int8.onnx`, `tokens.txt` |
| Streaming Paraformer Chinese INT8 (`paraformer-zh-online`) | https://huggingface.co/csukuangfj/sherpa-onnx-streaming-paraformer-zh | Apache-2.0 per model card | ~237 MB | `encoder.int8.onnx`, `decoder.int8.onnx`, `tokens.txt` |
| Whisper Tiny multilingual INT8 (`whisper-tiny-multilingual`) | https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny | MIT (upstream Whisper license) | ~103 MB | `tiny-encoder.int8.onnx`, `tiny-decoder.int8.onnx`, `tiny-tokens.txt` |
| Whisper Base multilingual INT8 (`whisper-base-multilingual`) | https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base | MIT (upstream Whisper license) | ~161 MB | `base-encoder.int8.onnx`, `base-decoder.int8.onnx`, `base-tokens.txt` |
| Whisper Small multilingual INT8 (`whisper-small-multilingual`) | https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small | MIT (upstream Whisper license) | ~375 MB | `small-encoder.int8.onnx`, `small-decoder.int8.onnx`, `small-tokens.txt` |

Whisper language support is restricted to the multilingual tokenizer languages. The `.en` Whisper variants are intentionally not listed because they only support English. Licenses and source cards must be re-verified before public redistribution.

## Evaluation audio

| Dataset | Source | License | Use |
|---|---|---|---|
| FLEURS test split (`google/fleurs`) | https://huggingface.co/datasets/google/fleurs | CC-BY-4.0 | Per-language speech evaluation with reference transcripts; not a music/lyrics benchmark. `scripts/test-asr-language.py` downloads only a selected language's test archive after explicit opt-in and stores extracted WAVs plus references, source identifiers, and SHA-256 hashes in the local cache. |
