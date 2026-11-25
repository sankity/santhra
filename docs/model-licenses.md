# Model licenses

| Model | Source | License | Size | Files used |
|---|---|---|---|---|
| Streaming Zipformer English 20M INT8 (`sherpa-onnx-streaming-zipformer-en-20M-2023-02-17`) | https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models (mirror: `csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17` on HF) | Apache-2.0 per model card | ~46 MB (int8 encoder/joiner + decoder + tokens) | `encoder-epoch-99-avg-1.int8.onnx`, `decoder-epoch-99-avg-1.onnx`, `joiner-epoch-99-avg-1.int8.onnx`, `tokens.txt` |
| Offline Paraformer Chinese INT8 (`paraformer-zh-offline`) | https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2023-03-28 | MIT per model card | ~223 MB | `model.int8.onnx`, `tokens.txt` |
| Streaming Paraformer Chinese INT8 (`paraformer-zh-online`) | https://huggingface.co/csukuangfj/sherpa-onnx-streaming-paraformer-zh | Apache-2.0 per model card | ~237 MB | `encoder.int8.onnx`, `decoder.int8.onnx`, `tokens.txt` |
| Whisper Tiny multilingual INT8 (`whisper-tiny-multilingual`) | https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny | MIT (upstream Whisper license) | ~103 MB | `tiny-encoder.int8.onnx`, `tiny-decoder.int8.onnx`, `tiny-tokens.txt` |
| Whisper Base multilingual INT8 (`whisper-base-multilingual`) | https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base | MIT (upstream Whisper license) | ~161 MB | `base-encoder.int8.onnx`, `base-decoder.int8.onnx`, `base-tokens.txt` |
| Whisper Small multilingual INT8 (`whisper-small-multilingual`) | https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small | MIT (upstream Whisper license) | ~375 MB | `small-encoder.int8.onnx`, `small-decoder.int8.onnx`, `small-tokens.txt` |
| Whisper Tiny Indonesian fp32 (`whisper-tiny-id`) | upstream https://huggingface.co/maleo-ai/whisper-tiny-id ; sherpa-onnx export re-hosted at https://huggingface.co/sankity/sherpa-onnx-whisper-tiny-id | Apache-2.0 (per upstream model card) | ~153 MB (fp32; int8 export measured worse) | `tiny-id-encoder.onnx`, `tiny-id-decoder.onnx`, `tiny-id-tokens.txt` |
| Nemotron 3.5 ASR 0.6B streaming multilingual INT8 (`nemotron-3-5-asr-560ms`) | upstream https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b ; sherpa-onnx export https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11 | OpenMDW-1.1 | ~682 MB | `encoder.int8.onnx`, `decoder.int8.onnx`, `joiner.int8.onnx`, `tokens.txt` |

Whisper language support is restricted to the multilingual tokenizer languages. The `.en` Whisper variants are intentionally not listed because they only support English. `whisper-tiny-id` is a Whisper tiny fine-tune offered for Indonesian only, with its ONNX export re-hosted by Santhra from the upstream model. Nemotron 3.5 ASR covers 39 language locales selected per stream; Indonesian is not one of them, so it is not offered for that model. Licenses and source cards must be re-verified before public redistribution.

## Evaluation audio

| Dataset | Source | License | Use |
|---|---|---|---|
| FLEURS test split (`google/fleurs`) | https://huggingface.co/datasets/google/fleurs | CC-BY-4.0 | Per-language speech evaluation with reference transcripts; not a music/lyrics benchmark. `scripts/test-asr-language.py` downloads only a selected language's test archive after explicit opt-in and stores extracted WAVs plus references, source identifiers, and SHA-256 hashes in the local cache. |
