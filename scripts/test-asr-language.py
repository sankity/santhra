#!/usr/bin/env python3
"""Evaluate one Santhra ASR model against reference-labelled FLEURS test audio.

FLEURS is read speech (CC-BY-4.0), not music or singing. Its per-language test
archive can be large, so downloading it requires an explicit opt-in.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import re
import struct
import subprocess
import sys
import tarfile
import unicodedata
import urllib.request
from pathlib import Path

# Keep this mapping aligned with Santhra's selectable speech languages. Explicit
# configs ensure the runner never substitutes a fixture from another language.
FLEURS_CONFIGS = {
    "english": "en_us",
    "chinese": "cmn_hans_cn",
    "spanish": "es_419",
    "hindi": "hi_in",
    "indonesia": "id_id",
    "french": "fr_fr",
    "german": "de_de",
    "japanese": "ja_jp",
    "portuguese": "pt_br",
    "russian": "ru_ru",
}
FLEURS_REVISION = "4683b04af03d2d9549064c7d72060a9a94bb6046"
FLEURS_ROOT = f"https://huggingface.co/datasets/google/fleurs/resolve/{FLEURS_REVISION}/data"
LICENSE = "CC-BY-4.0"


def fetch_bytes(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "santhra-asr-eval/1.0"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def read_references(tsv_bytes: bytes) -> dict[str, str]:
    text = tsv_bytes.decode("utf-8-sig")
    rows = list(csv.reader(io.StringIO(text), delimiter="\t"))
    if not rows:
        raise ValueError("FLEURS test.tsv is empty")
    header = [cell.strip().lower() for cell in rows[0]]
    has_header = any(name in header for name in ("filename", "file_name", "transcription"))
    if has_header:
        filename_index = next((header.index(name) for name in ("filename", "file_name", "path") if name in header), None)
        reference_index = next((header.index(name) for name in ("transcription", "text", "raw_transcription") if name in header), None)
        if filename_index is None or reference_index is None:
            raise ValueError(f"Unexpected FLEURS TSV columns: {header}")
        rows = rows[1:]
    else:
        filename_index, reference_index = 1, 3
    references = {}
    for row in rows:
        if len(row) > max(filename_index, reference_index):
            references[Path(row[filename_index]).name] = row[reference_index].strip()
    if not references:
        raise ValueError("No labelled test examples found in FLEURS TSV")
    return references


def read_wav_format(path: Path) -> tuple[int, int, int, int]:
    with path.open("rb") as audio:
        header = audio.read(12)
        if len(header) != 12 or header[:4] != b"RIFF" or header[8:] != b"WAVE":
            raise ValueError(f"Not a supported RIFF/WAVE file: {path}")

        while True:
            chunk_header = audio.read(8)
            if len(chunk_header) != 8:
                raise ValueError(f"WAV file has no format chunk: {path}")
            chunk_id, chunk_size = struct.unpack("<4sI", chunk_header)
            if chunk_id == b"fmt ":
                chunk = audio.read(chunk_size)
                if len(chunk) < 16:
                    raise ValueError(f"WAV format chunk is too short: {path}")
                audio_format, channels, sample_rate, _, _, bits_per_sample = struct.unpack_from("<HHIIHH", chunk)
                if audio_format == 0xFFFE and len(chunk) >= 40:
                    audio_format = struct.unpack_from("<H", chunk, 24)[0]
                return audio_format, channels, sample_rate, bits_per_sample

            audio.seek(chunk_size + (chunk_size & 1), 1)


def cache_fleurs(language: str, config: str, count: int, cache_root: Path) -> list[dict[str, str]]:
    out_dir = cache_root / language
    manifest_path = out_dir / "manifest.json"
    if manifest_path.is_file():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        examples = [item for item in manifest["examples"] if Path(item["wav"]).is_file()]
        if len(examples) >= count:
            return examples[:count]

    out_dir.mkdir(parents=True, exist_ok=True)
    tsv_url = f"{FLEURS_ROOT}/{config}/test.tsv"
    archive_url = f"{FLEURS_ROOT}/{config}/audio/test.tar.gz"
    references = read_references(fetch_bytes(tsv_url))
    wanted = list(references.items())[:count]
    wanted_names = {name for name, _ in wanted}
    extracted: dict[str, Path] = {
        name: out_dir / name for name in wanted_names if (out_dir / name).is_file()
    }

    if extracted.keys() < wanted_names:
        request = urllib.request.Request(archive_url, headers={"User-Agent": "santhra-asr-eval/1.0"})
        with urllib.request.urlopen(request, timeout=120) as response, tarfile.open(fileobj=response, mode="r|gz") as archive:
            for member in archive:
                name = Path(member.name).name
                if not member.isfile() or name not in wanted_names or name in extracted:
                    continue
                source = archive.extractfile(member)
                if source is None:
                    continue
                target = out_dir / name
                target.write_bytes(source.read())
                extracted[name] = target
                if extracted.keys() >= wanted_names:
                    break

    missing = wanted_names - extracted.keys()
    if missing:
        raise RuntimeError(f"FLEURS archive did not contain expected WAVs: {', '.join(sorted(missing))}")

    examples = []
    for name, reference in wanted:
        path = extracted[name]
        audio_format, channels, sample_rate, bits_per_sample = read_wav_format(path)
        if sample_rate != 16_000 or channels != 1 or audio_format not in {1, 3}:
            raise ValueError(
                f"Expected 16 kHz mono PCM or IEEE-float FLEURS WAV, got "
                f"format {audio_format}, {sample_rate} Hz, {channels} channels, "
                f"{bits_per_sample} bits: {path}"
            )
        examples.append({
            "wav": str(path.resolve()),
            "reference": reference,
            "source": f"FLEURS {config}/test/{name}",
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        })

    manifest = {
        "dataset": "google/fleurs",
        "revision": FLEURS_REVISION,
        "split": "test",
        "config": config,
        "license": LICENSE,
        "source_url": f"https://huggingface.co/datasets/google/fleurs/tree/{FLEURS_REVISION}/data/{config}",
        "examples": examples,
    }
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return examples


def normalize(text: str) -> str:
    text = unicodedata.normalize("NFKC", text).casefold()
    return "".join(char for char in text if not char.isspace() and not unicodedata.category(char).startswith("P"))


def edit_distance(reference: str, hypothesis: str) -> int:
    previous = list(range(len(hypothesis) + 1))
    for index, ref_char in enumerate(reference, 1):
        current = [index]
        for column, hyp_char in enumerate(hypothesis, 1):
            current.append(min(
                current[-1] + 1,
                previous[column] + 1,
                previous[column - 1] + (ref_char != hyp_char),
            ))
        previous = current
    return previous[-1]


def transcribe(cli: str, wav: str, language: str, model: str | None) -> str:
    command = [cli, "transcribe", wav, "--language", language, "--no-partial"]
    if model:
        command.extend(["--model", model])
    result = subprocess.run(command, text=True, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(f"Santhra CLI failed for {wav}:\n{result.stderr.strip()}")
    finals = re.findall(r"^\[final\]\s*(.*)$", result.stdout, flags=re.MULTILINE)
    hypothesis = " ".join(part.strip() for part in finals if part.strip())
    if not hypothesis:
        raise RuntimeError(f"No final transcript returned for {wav}; CLI output was:\n{result.stdout.strip()}")
    return hypothesis


def list_models(cli: str, language: str) -> list[dict[str, str | bool]]:
    result = subprocess.run(
        [cli, "models", "list", "--language", language],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"Could not list models:\n{result.stderr.strip()}")

    models = []
    for line in result.stdout.splitlines():
        columns = line.split()
        if len(columns) >= 6 and line.rstrip().endswith(" installed"):
            models.append({"id": columns[0], "installed": True})
        elif len(columns) >= 6 and line.rstrip().endswith(" not downloaded"):
            models.append({"id": columns[0], "installed": False})
    if not models:
        raise RuntimeError(f"Santhra CLI returned no compatible models for '{language}'.")
    return models


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    language_selection = parser.add_mutually_exclusive_group()
    language_selection.add_argument("--language", help="Santhra language id, for example indonesia")
    language_selection.add_argument("--all-languages", action="store_true", help="Run every FLEURS-backed language and report languages without verified fixtures")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--model", help="Test one model id")
    selection.add_argument("--all-models", action="store_true", help="Test every catalog model compatible with this language")
    parser.add_argument("--download-missing-models", action="store_true", help="With --all-models, download compatible models that are not installed")
    parser.add_argument("--cli", default="target/debug/santhra-cli", help="Path to a built santhra-cli executable")
    parser.add_argument("--samples", type=int, default=3, help="Number of labelled FLEURS test utterances (default: 3)")
    parser.add_argument("--max-cer", type=float, help="Optional failure threshold for aggregate CER (0.0 to 1.0)")
    parser.add_argument("--yes-download-large", action="store_true", help="Allow downloading the selected language's FLEURS test archive")
    parser.add_argument("--cache-dir", type=Path, default=Path.home() / ".cache/santhra/fleurs")
    args = parser.parse_args()

    if not args.all_languages and not args.language:
        parser.error("provide --language or --all-languages")
    if args.all_languages and not args.all_models:
        parser.error("--all-languages requires --all-models")
    if args.download_missing_models and not args.all_models:
        parser.error("--download-missing-models requires --all-models")
    if args.samples < 1:
        parser.error("--samples must be at least 1")
    if args.max_cer is not None and not 0 <= args.max_cer <= 1:
        parser.error("--max-cer must be between 0 and 1")

    if args.all_languages:
        print(
            f"Batch ASR: {len(FLEURS_CONFIGS)} selected languages x compatible catalog models. "
            "All selected languages have verified FLEURS reference fixtures."
        )
        command = [
            sys.executable,
            str(Path(__file__).resolve()),
            "--all-models",
            "--samples",
            str(args.samples),
            "--cli",
            args.cli,
            "--cache-dir",
            str(args.cache_dir),
        ]
        if args.max_cer is not None:
            command.extend(["--max-cer", str(args.max_cer)])
        if args.yes_download_large:
            command.append("--yes-download-large")
        if args.download_missing_models:
            command.append("--download-missing-models")

        failed_languages = []
        for language in FLEURS_CONFIGS:
            print(f"\n########## LANGUAGE: {language} ##########", flush=True)
            result = subprocess.run(command + ["--language", language], check=False)
            if result.returncode:
                failed_languages.append(language)
                print(f"[FAILED] Evaluation for {language} (exit {result.returncode})")

        print("\nAll selected languages have verified reference fixtures.")
        if failed_languages:
            print(f"\nLanguages with failed evaluations: {', '.join(failed_languages)}", file=sys.stderr)
            return 1
        return 0

    config = FLEURS_CONFIGS.get(args.language)
    if not config:
        supported = ", ".join(sorted(FLEURS_CONFIGS))
        print(f"No verified FLEURS test mapping for '{args.language}'. No fixture was substituted.\nFLEURS-backed language ids: {supported}", file=sys.stderr)
        return 2

    cached_manifest = args.cache_dir / args.language / "manifest.json"
    enough_cached = False
    if cached_manifest.is_file():
        try:
            cached = json.loads(cached_manifest.read_text(encoding="utf-8"))
            enough_cached = sum(Path(item["wav"]).is_file() for item in cached["examples"]) >= args.samples
        except (OSError, ValueError, KeyError):
            pass
    if not enough_cached and not args.yes_download_large:
        print(
            f"FLEURS {config}/test audio is an archive and may be hundreds of MB. "
            "It is cached after the first download. Review the FLEURS CC-BY-4.0 terms; "
            "rerun with --yes-download-large to continue, or provide a cached fixture.",
            file=sys.stderr,
        )
        return 2

    try:
        examples = cache_fleurs(args.language, config, args.samples, args.cache_dir)
        if args.all_models:
            models = list_models(args.cli, args.language)
            failed = []
            for model in models:
                if not model["installed"] and args.download_missing_models:
                    model_id = str(model["id"])
                    print(f"Downloading model {model_id}...")
                    download = subprocess.run([args.cli, "models", "download", model_id], check=False)
                    if download.returncode:
                        print(f"[FAILED] Download model {model_id} (exit {download.returncode})")
                        failed.append(model_id)
                        model["skip"] = True
                    else:
                        model["installed"] = True
            selected_models = [model for model in models if model["installed"]]
            if not selected_models:
                raise RuntimeError(
                    "No compatible models are installed. Use --download-missing-models to download them."
                )
            skipped = [str(model["id"]) for model in models if not model["installed"]]
            if skipped:
                print("Skipped models (not downloaded): " + ", ".join(skipped))
        else:
            selected_models = [{"id": args.model or "default", "installed": True}]
            failed = []

        report_models = []
        for model in selected_models:
            model_id = str(model["id"])
            print(f"\n=== {args.language} / {model_id} ===")
            total_errors = 0
            total_chars = 0
            correct = 0
            for index, example in enumerate(examples, 1):
                try:
                    hypothesis = transcribe(
                        args.cli,
                        example["wav"],
                        args.language,
                        None if model_id == "default" else model_id,
                    )
                except (OSError, RuntimeError) as error:
                    print(f"[FAILED] Sample {index}: {error}")
                    failed.append(f"{model_id}:{index}")
                    continue

                reference = normalize(example["reference"])
                decoded = normalize(hypothesis)
                errors = edit_distance(reference, decoded)
                exact = reference == decoded
                correct += int(exact)
                total_errors += errors
                total_chars += len(reference)
                print(f"[{'CORRECT' if exact else 'INCORRECT'}] Sample {index}/{len(examples)}")
                if not exact:
                    print(f"  Reference: {example['reference']}")
                    print(f"  Hypothesis: {hypothesis}")

            cer = total_errors / max(total_chars, 1)
            print(f"Summary: {correct}/{len(examples)} exact matches | CER {cer:.3f}")
            report_models.append({"model": model_id, "cer": cer, "exact_matches": correct})

        if len(report_models) > 1:
            print("\n=== Model comparison (lower CER is better) ===")
            for item in sorted(report_models, key=lambda row: row["cer"]):
                print(f"{item['model']}: {item['exact_matches']}/{len(examples)} CORRECT | CER {item['cer']:.3f}")

        if args.max_cer is not None:
            over_limit = [item["model"] for item in report_models if item["cer"] > args.max_cer]
            if over_limit:
                print(f"CER melewati batas {args.max_cer:.3f}: {', '.join(over_limit)}", file=sys.stderr)
                return 1
        if failed:
            print("One or more model/sample runs failed; see [FAILED] messages above.", file=sys.stderr)
            return 1
        return 0
    except (OSError, ValueError, RuntimeError, tarfile.TarError, KeyError) as error:
        print(f"ASR evaluation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("\nEvaluation interrupted by user.", file=sys.stderr)
        raise SystemExit(130)
