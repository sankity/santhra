#!/usr/bin/env python3
"""Evaluate Santhra ASR models against reference-labelled FLEURS test audio.

Stages are subcommands: smoke, screening, accuracy, streaming, compare. FLEURS is read
speech (CC-BY-4.0), not music or singing. Its per-language test archive can be large, so
downloading it requires an explicit opt-in.

ASR inference always goes through `santhra-cli transcribe-batch`, which loads the model
once per batch and reuses the production `speech-sherpa` path.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import os
import platform
import shutil
import statistics
import struct
import subprocess
import sys
import tarfile
import urllib.request
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import asr_metrics as metrics  # noqa: E402

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
LICENSE = "CC-BY-4.0"
USER_AGENT = "santhra-asr-eval/2.0"

STAGES = ("smoke", "screening", "accuracy", "streaming", "compare")
DEFAULT_SAMPLES = {"smoke": 3, "screening": 25, "accuracy": 150, "streaming": 25}
BUCKETS = ("short", "medium", "long")
STREAMING_KINDS = frozenset({"zipformer", "paraformer-online", "nemo-streaming"})
REPO_ROOT = Path(__file__).resolve().parent.parent


class EvalError(RuntimeError):
    """A setup or execution problem that should stop the run."""


# ---------------------------------------------------------------------------
# Dataset
# ---------------------------------------------------------------------------


def fleurs_root(revision: str) -> str:
    return f"https://huggingface.co/datasets/google/fleurs/resolve/{revision}/data"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def download_file(url: str, destination: Path, force: bool = False) -> None:
    if destination.is_file() and not force:
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + ".part")
    print(f"downloading {url}", file=sys.stderr, flush=True)
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=120) as response, temporary.open("wb") as target:
        shutil.copyfileobj(response, target, 1 << 20)
    temporary.replace(destination)


def _skip(stream, size: int) -> None:
    try:
        stream.seek(size, 1)
    except (AttributeError, OSError, ValueError):
        remaining = size
        while remaining:
            block = stream.read(min(remaining, 1 << 20))
            if not block:
                raise EvalError("unexpected end of WAV chunk")
            remaining -= len(block)


def parse_wav_info(stream) -> dict[str, object]:
    """Read a RIFF/WAVE header from a seekable stream and return format + duration."""
    header = stream.read(12)
    if len(header) != 12 or header[:4] != b"RIFF" or header[8:] != b"WAVE":
        raise EvalError("not a supported RIFF/WAVE file")
    fmt: dict[str, object] | None = None
    while True:
        chunk_header = stream.read(8)
        if len(chunk_header) != 8:
            raise EvalError("WAV file has no data chunk")
        chunk_id, chunk_size = struct.unpack("<4sI", chunk_header)
        if chunk_id == b"fmt ":
            chunk = stream.read(chunk_size)
            if len(chunk) < 16:
                raise EvalError("WAV format chunk is too short")
            audio_format, channels, sample_rate, byte_rate, block_align, bits = struct.unpack_from(
                "<HHIIHH", chunk
            )
            if audio_format == 0xFFFE and len(chunk) >= 40:
                audio_format = struct.unpack_from("<H", chunk, 24)[0]
            fmt = {
                "audio_format": audio_format,
                "channels": channels,
                "sample_rate": sample_rate,
                "byte_rate": byte_rate,
                "block_align": block_align,
                "bits_per_sample": bits,
            }
            if chunk_size & 1:
                stream.read(1)
        elif chunk_id == b"data":
            if fmt is None:
                raise EvalError("WAV data chunk appears before the format chunk")
            byte_rate = int(fmt["byte_rate"]) or 1
            return {**fmt, "duration_seconds": round(chunk_size / byte_rate, 6)}
        else:
            _skip(stream, chunk_size + (chunk_size & 1))


def validate_audio(info: dict[str, object], path: Path) -> None:
    if info["sample_rate"] != 16_000 or info["channels"] != 1 or info["audio_format"] not in {1, 3}:
        raise EvalError(
            "expected 16 kHz mono PCM or IEEE-float FLEURS WAV, got "
            f"format {info['audio_format']}, {info['sample_rate']} Hz, "
            f"{info['channels']} channels, {info['bits_per_sample']} bits: {path}"
        )


def read_references(tsv_bytes: bytes) -> list[dict[str, str]]:
    text = tsv_bytes.decode("utf-8-sig")
    rows = list(csv.reader(io.StringIO(text), delimiter="\t"))
    if not rows:
        raise EvalError("FLEURS test.tsv is empty")
    header = [cell.strip().lower() for cell in rows[0]]
    has_header = any(name in header for name in ("filename", "file_name", "transcription"))
    if has_header:
        filename_index = next(
            (header.index(name) for name in ("filename", "file_name", "path") if name in header),
            None,
        )
        reference_index = next(
            (header.index(name) for name in ("transcription", "text", "raw_transcription") if name in header),
            None,
        )
        if filename_index is None or reference_index is None:
            raise EvalError(f"unexpected FLEURS TSV columns: {header}")
        rows = rows[1:]
    else:
        filename_index, reference_index = 1, 3

    references: list[dict[str, str]] = []
    seen: set[str] = set()
    for row in rows:
        if len(row) <= max(filename_index, reference_index):
            continue
        name = Path(row[filename_index]).name
        if not name or name in seen:
            continue
        seen.add(name)
        references.append({"name": name, "reference": row[reference_index].strip()})
    if not references:
        raise EvalError("no labelled test examples found in FLEURS TSV")
    return references


def archive_wav_info(archive_path: Path) -> dict[str, dict[str, object]]:
    info: dict[str, dict[str, object]] = {}
    with tarfile.open(archive_path, "r:gz") as archive:
        for member in archive:
            if not member.isfile() or not member.name.lower().endswith(".wav"):
                continue
            source = archive.extractfile(member)
            if source is None:
                continue
            try:
                info[Path(member.name).name] = parse_wav_info(source)
            except EvalError:
                continue
    return info


def extract_wavs(archive_path: Path, names: list[str], destination: Path) -> dict[str, Path]:
    destination.mkdir(parents=True, exist_ok=True)
    wanted = set(names)
    found: dict[str, Path] = {}
    with tarfile.open(archive_path, "r:gz") as archive:
        for member in archive:
            name = Path(member.name).name
            if name not in wanted or not member.isfile():
                continue
            source = archive.extractfile(member)
            if source is None:
                continue
            target = destination / name
            target.write_bytes(source.read())
            found[name] = target
            if len(found) == len(wanted):
                break
    return found


def stable_key(seed: int, sample_id: str) -> bytes:
    """Order key that is stable across Python versions and platforms."""
    return hashlib.sha256(f"{seed}:{sample_id}".encode("utf-8")).digest()


def bucket_of(duration: float, short_max: float, medium_max: float) -> str:
    if duration < short_max:
        return "short"
    if duration < medium_max:
        return "medium"
    return "long"


def select_samples(
    candidates: list[dict[str, object]],
    count: int,
    seed: int,
    short_max: float,
    medium_max: float,
) -> tuple[list[dict[str, object]], dict[str, int]]:
    """Deterministic, duration-aware selection. Duplicate ids keep their first entry."""
    unique: dict[str, dict[str, object]] = {}
    for item in candidates:
        unique.setdefault(str(item["id"]), item)
    ranked = sorted(unique.values(), key=lambda item: stable_key(seed, str(item["id"])))
    groups: dict[str, list[dict[str, object]]] = {name: [] for name in BUCKETS}
    for item in ranked:
        groups[bucket_of(float(item["duration_seconds"]), short_max, medium_max)].append(item)

    selected: list[dict[str, object]] = []
    while len(selected) < count:
        progressed = False
        for name in BUCKETS:
            if groups[name] and len(selected) < count:
                selected.append(groups[name].pop(0))
                progressed = True
        if not progressed:
            break
    selected.sort(key=lambda item: stable_key(seed, str(item["id"])))
    distribution = Counter(
        bucket_of(float(item["duration_seconds"]), short_max, medium_max) for item in selected
    )
    return selected, {name: distribution.get(name, 0) for name in BUCKETS}


def manifest_usable(
    cache_dir: Path,
    config: str,
    count: int,
    seed: int,
    revision: str,
    short_max: float,
    medium_max: float,
) -> dict | None:
    path = cache_dir / config / "manifest.json"
    if not path.is_file():
        return None
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    sampling = manifest.get("sampling", {})
    if (
        manifest.get("dataset") != "google/fleurs"
        or manifest.get("revision") != revision
        or manifest.get("split") != "test"
        or manifest.get("config") != config
        or manifest.get("seed") != seed
        or sampling.get("short_max_seconds") != short_max
        or sampling.get("medium_max_seconds") != medium_max
        or len(manifest.get("samples", [])) < count
    ):
        return None
    for sample in manifest["samples"]:
        audio = Path(sample["audio_path"])
        if not audio.is_file() or sha256_file(audio) != sample["sha256"]:
            return None
    return manifest


def ensure_dataset(
    cache_dir: Path,
    language: str,
    config: str,
    count: int,
    seed: int,
    revision: str,
    short_max: float,
    medium_max: float,
    force: bool = False,
) -> dict:
    root = cache_dir / config
    audio_dir = root / "audio"
    if not force:
        cached = manifest_usable(cache_dir, config, count, seed, revision, short_max, medium_max)
        if cached is not None:
            return cached

    root_url = fleurs_root(revision)
    tsv_path = root / "test.tsv"
    archive_path = root / "test.tar.gz"
    download_file(f"{root_url}/{config}/test.tsv", tsv_path, force)
    references = read_references(tsv_path.read_bytes())
    download_file(f"{root_url}/{config}/audio/test.tar.gz", archive_path, force)
    print(f"scanning {archive_path.name} for durations", file=sys.stderr, flush=True)
    info = archive_wav_info(archive_path)

    candidates = [
        {
            "id": f"{config}/test/{entry['name']}",
            "name": entry["name"],
            "reference": entry["reference"],
            "duration_seconds": info[entry["name"]]["duration_seconds"],
        }
        for entry in references
        if entry["name"] in info
    ]
    if len(candidates) < len(references):
        print(
            f"warning: {len(references) - len(candidates)} FLEURS WAVs could not be parsed",
            file=sys.stderr,
        )
    if not candidates:
        raise EvalError(f"no parseable WAV files in {archive_path}")

    selected, distribution = select_samples(candidates, count, seed, short_max, medium_max)
    paths = extract_wavs(archive_path, [str(item["name"]) for item in selected], audio_dir)
    missing = sorted(str(item["name"]) for item in selected if item["name"] not in paths)
    if missing:
        raise EvalError(f"FLEURS archive did not contain expected WAVs: {', '.join(missing)}")

    samples = []
    for item in sorted(selected, key=lambda item: stable_key(seed, str(item["id"]))):
        path = paths[str(item["name"])]
        with path.open("rb") as handle:
            audio_info = parse_wav_info(handle)
        validate_audio(audio_info, path)
        samples.append(
            {
                "id": item["id"],
                "audio_path": str(path.resolve()),
                "reference": item["reference"],
                "duration_seconds": audio_info["duration_seconds"],
                "sha256": sha256_file(path),
                "audio": {
                    "sample_rate": audio_info["sample_rate"],
                    "channels": audio_info["channels"],
                    "format": "pcm_s16le" if audio_info["audio_format"] == 1 else "pcm_f32le",
                },
            }
        )

    manifest = {
        "dataset": "google/fleurs",
        "revision": revision,
        "split": "test",
        "config": config,
        "language": language,
        "license": LICENSE,
        "seed": seed,
        "sampling": {
            "method": "sha256-rank-round-robin-buckets",
            "short_max_seconds": short_max,
            "medium_max_seconds": medium_max,
            "requested": count,
            "distribution": distribution,
        },
        "samples": samples,
    }
    root.mkdir(parents=True, exist_ok=True)
    (root / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return manifest


def load_manifest(path: Path) -> dict:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise EvalError(f"cannot load manifest {path}: {error}") from error
    if not manifest.get("samples"):
        raise EvalError(f"manifest {path} has no samples")
    for sample in manifest["samples"]:
        if not Path(sample["audio_path"]).is_file():
            raise EvalError(f"missing audio for sample {sample['id']}: {sample['audio_path']}")
    return manifest


# ---------------------------------------------------------------------------
# CLI + batch runner
# ---------------------------------------------------------------------------


def run_cli(cli: str, *args: object, timeout: float = 600) -> subprocess.CompletedProcess:
    return subprocess.run(
        [cli, *(str(arg) for arg in args)],
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )


def list_models(cli: str, language: str) -> list[dict]:
    result = run_cli(cli, "models", "list", "--language", language, "--json")
    if result.returncode:
        raise EvalError(f"could not list models for {language}:\n{result.stderr.strip()}")
    try:
        models = json.loads(result.stdout)
    except ValueError as error:
        raise EvalError(f"`models list --json` returned invalid JSON: {error}") from error
    if not isinstance(models, list):
        raise EvalError("`models list --json` did not return a JSON array")
    return models


def choose_models(cli: str, language: str, args) -> tuple[list[dict], list[str]]:
    catalog = list_models(cli, language)
    if not catalog:
        raise EvalError(f"Santhra CLI returned no compatible models for '{language}'.")
    skipped: list[str] = []
    if args.model:
        matches = [item for item in catalog if item["id"] == args.model]
        if not matches:
            raise EvalError(f"model '{args.model}' is unknown or not compatible with '{language}'")
        candidates = matches
    elif args.all_models:
        candidates = catalog
    else:
        default = [item for item in catalog if item.get("default")]
        if not default:
            default = [item for item in catalog if item.get("installed")] or catalog[:1]
        candidates = default[:1]

    selected = []
    for model in candidates:
        if model.get("installed"):
            selected.append(model)
            continue
        if not args.download_missing_models:
            skipped.append(f"{model['id']} (not downloaded)")
            continue
        print(f"Downloading model {model['id']}...", file=sys.stderr, flush=True)
        download = subprocess.run([cli, "models", "download", model["id"]], check=False)
        if download.returncode:
            skipped.append(f"{model['id']} (download failed, exit {download.returncode})")
        else:
            selected.append({**model, "installed": True})
    return selected, skipped


def parse_batch_output(text: str) -> tuple[dict, list[dict], dict | None]:
    """Parse the JSONL protocol of `santhra-cli transcribe-batch`."""
    meta: dict | None = None
    samples: list[dict] = []
    summary: dict | None = None
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        try:
            item = json.loads(line)
        except ValueError as error:
            raise EvalError(
                f"batch output line {number} is not JSON ({error}): {line[:200]}"
            ) from error
        if not isinstance(item, dict) or "type" not in item:
            raise EvalError(f"batch output line {number} has no record type: {line[:200]}")
        kind = item["type"]
        if kind == "meta":
            meta = item
        elif kind == "sample":
            samples.append(item)
        elif kind == "summary":
            summary = item
        else:
            raise EvalError(f"unknown batch record type {kind!r} on line {number}")
    if meta is None:
        raise EvalError("batch output has no meta record")
    return meta, samples, summary


def run_batch(
    cli: str,
    model_id: str,
    language: str,
    manifest_path: Path,
    output_path: Path,
    args,
) -> tuple[dict, list[dict], dict | None, str]:
    command = [
        cli,
        "transcribe-batch",
        "--model",
        model_id,
        "--language",
        language,
        "--manifest",
        str(manifest_path),
        "--output",
        str(output_path),
        "--threads",
        str(args.threads),
        "--provider",
        args.provider,
        "--chunk-ms",
        str(args.chunk_ms),
        "--buffer-ms",
        str(args.buffer_ms),
        "--timeout",
        str(args.timeout),
    ]
    if args.no_partial:
        command.append("--no-partial")
    process = subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    hard_limit = args.timeout + 60 if args.timeout else None
    try:
        stdout, stderr = process.communicate(timeout=hard_limit)
    except subprocess.TimeoutExpired:
        process.kill()
        stdout, stderr = process.communicate()
        stderr = f"{stderr}\nbatch exceeded the hard {hard_limit}s backstop and was killed"
    text = output_path.read_text(encoding="utf-8") if output_path.is_file() else stdout
    meta, samples, summary = parse_batch_output(text)
    return meta, samples, summary, stderr.strip()


def build_rows(
    run_id: str,
    model_id: str,
    language: str,
    manifest_samples: list[dict],
    batch_samples: list[dict],
    error_text: str,
) -> list[dict]:
    by_id = {str(item.get("id")): item for item in batch_samples}
    rows = []
    for sample in manifest_samples:
        sample_id = str(sample["id"])
        record = by_id.get(sample_id)
        if record is None:
            status = "process_error"
            finals: list[str] = []
            elapsed_ms = None
            error = error_text or "batch produced no result for this sample"
            partial_count = 0
        else:
            status = str(record.get("status", "process_error"))
            finals = [str(part) for part in record.get("finals") or []]
            elapsed_ms = record.get("elapsed_ms")
            error = record.get("error")
            partial_count = int(record.get("partial_count") or 0)
        hypothesis = " ".join(part.strip() for part in finals if part.strip())
        if status == "success" and not hypothesis:
            status = "empty_transcript"
        scored = metrics.score(sample["reference"], hypothesis, language) if status == "success" else {
            "reference_normalized": metrics.normalize(sample["reference"]),
            "hypothesis_normalized": metrics.normalize(hypothesis),
            "cer": None,
            "wer": None,
            "exact_match": False,
            "sentence_error": False,
        }
        inference_seconds = None if elapsed_ms is None else round(elapsed_ms / 1000.0, 6)
        duration = float(sample["duration_seconds"])
        rows.append(
            {
                "run_id": run_id,
                "model_id": model_id,
                "language": language,
                "sample_id": sample_id,
                "status": status,
                "reference_raw": sample["reference"],
                "hypothesis_raw": hypothesis,
                "hypothesis_finals": finals,
                "reference_normalized": scored["reference_normalized"],
                "hypothesis_normalized": scored["hypothesis_normalized"],
                "cer": scored["cer"],
                "wer": scored["wer"],
                "exact_match": scored["exact_match"],
                "sentence_error": scored["sentence_error"],
                "audio_duration_seconds": duration,
                "inference_time_seconds": inference_seconds,
                "rtf": None if inference_seconds is None else round(inference_seconds / duration, 6),
                "partial_count": partial_count,
                "error": error,
            }
        )
    return rows


def summarize_pair(
    run_id: str,
    model: dict,
    language: str,
    rows: list[dict],
    meta: dict,
    summary_record: dict | None,
    min_success_rate: float,
) -> dict:
    attempted = len(rows)
    counts = Counter(row["status"] for row in rows)
    success_rows = [row for row in rows if row["status"] == "success"]
    scorable = [row for row in success_rows if row["cer"] is not None]
    pairs = [(row["reference_raw"], row["hypothesis_raw"]) for row in scorable]
    corpus_cer, edit_distance, reference_chars = metrics.corpus_cer(pairs)
    corpus_wer, word_edits, reference_words = metrics.corpus_wer(pairs, language)
    cer_values = [row["cer"] for row in scorable]
    wer_values = [row["wer"] for row in scorable if row["wer"] is not None]
    times = [row["inference_time_seconds"] for row in success_rows if row["inference_time_seconds"]]
    rtfs = [row["rtf"] for row in success_rows if row["rtf"] is not None]
    success_rate = len(success_rows) / attempted if attempted else 0.0
    model_bytes = sum(int(file.get("bytes", 0)) for file in meta.get("model_files", []))
    return {
        "run_id": run_id,
        "language": language,
        "model_id": model["id"],
        "model_label": model.get("label"),
        "kind": model.get("kind"),
        "attempted": attempted,
        "success": len(success_rows),
        "empty_transcript": counts.get("empty_transcript", 0),
        "failed": counts.get("audio_error", 0) + counts.get("model_error", 0) + counts.get("process_error", 0),
        "timeout": counts.get("timeout", 0),
        "success_rate": round(success_rate, 6),
        "corpus_cer": None if corpus_cer is None else round(corpus_cer, 6),
        "mean_cer": round(statistics.fmean(cer_values), 6) if cer_values else None,
        "cer_edits": edit_distance,
        "cer_reference_chars": reference_chars,
        "corpus_wer": None if corpus_wer is None else round(corpus_wer, 6),
        "mean_wer": round(statistics.fmean(wer_values), 6) if wer_values else None,
        "wer_edits": word_edits,
        "wer_reference_words": reference_words,
        "wer_policy": "n/a (no word tokenizer)" if language in metrics.UNSEGMENTED_LANGUAGES else "whitespace tokens, punctuation as separator",
        "exact_match_rate": round(
            sum(1 for row in scorable if row["exact_match"]) / len(scorable), 6
        ) if scorable else None,
        "ser": round(
            sum(1 for row in scorable if row["sentence_error"]) / len(scorable), 6
        ) if scorable else None,
        "mean_inference_seconds": round(statistics.fmean(times), 6) if times else None,
        "median_inference_seconds": round(statistics.median(times), 6) if times else None,
        "mean_rtf": round(statistics.fmean(rtfs), 6) if rtfs else None,
        "median_rtf": round(statistics.median(rtfs), 6) if rtfs else None,
        "load_ms": meta.get("load_ms"),
        "peak_rss_kb": None if not summary_record else summary_record.get("peak_rss_kb"),
        "model_bytes": model_bytes,
        "min_success_rate": min_success_rate,
        "recommendable": success_rate >= min_success_rate,
    }


# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------


def first_cpu_model() -> str | None:
    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            if line.lower().startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return None


def meminfo_value(key: str) -> int | None:
    try:
        for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines():
            if line.startswith(f"{key}:"):
                return int(line.split()[1]) * 1024
    except (OSError, ValueError, IndexError):
        pass
    return None


def command_version(command: list[str]) -> str | None:
    if shutil.which(command[0]) is None:
        return None
    try:
        result = subprocess.run(command, text=True, capture_output=True, check=False, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout.strip() or None


def git_commit() -> str | None:
    if shutil.which("git") is None:
        return None
    try:
        result = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD"],
            text=True,
            capture_output=True,
            check=False,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout.strip() or None


def workspace_version() -> str | None:
    try:
        text = (REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8")
    except OSError:
        return None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("version") and "=" in stripped:
            return stripped.split("=", 1)[1].strip().strip('"')
    return None


def cargo_lock_version(crate: str) -> str | None:
    try:
        lines = (REPO_ROOT / "Cargo.lock").read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    for index, line in enumerate(lines):
        if line.strip() == f'name = "{crate}"':
            for following in lines[index + 1 : index + 4]:
                if following.strip().startswith("version"):
                    return following.split("=", 1)[1].strip().strip('"')
    return None


def environment() -> dict:
    uname = platform.uname()
    return {
        "os": {
            "system": uname.system,
            "release": uname.release,
            "version": uname.version,
            "machine": uname.machine,
        },
        "python": platform.python_version(),
        "cpu_model": first_cpu_model(),
        "cpu_count": os.cpu_count(),
        "ram_bytes": meminfo_value("MemTotal"),
        "rustc": command_version(["rustc", "--version"]),
        "cargo": command_version(["cargo", "--version"]),
        "git_commit": git_commit(),
        "santhra_version": workspace_version(),
        "sherpa_onnx_version": cargo_lock_version("sherpa-onnx"),
        "recorded_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
    }


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------


SUMMARY_FIELDS = [
    "run_id",
    "stage",
    "language",
    "model_id",
    "kind",
    "attempted",
    "success",
    "empty_transcript",
    "failed",
    "timeout",
    "success_rate",
    "corpus_cer",
    "mean_cer",
    "corpus_wer",
    "mean_wer",
    "exact_match_rate",
    "ser",
    "mean_inference_seconds",
    "median_inference_seconds",
    "mean_rtf",
    "median_rtf",
    "load_ms",
    "peak_rss_kb",
    "model_bytes",
    "min_success_rate",
    "recommendable",
]


def write_csv(path: Path, fieldnames: list[str], rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def format_optional(value, digits: int = 3) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float):
        return f"{value:.{digits}f}"
    return str(value)


def print_table(rows: list[dict]) -> None:
    header = f"{'language':<11} {'model':<28} {'ok':>4} {'success':>7} {'CER':>7} {'WER':>7} {'RTF':>7} {'load s':>7} {'size MB':>8}"
    print(header)
    print("-" * len(header))
    for row in rows:
        size = (row["model_bytes"] or 0) / 1_000_000
        print(
            f"{row['language']:<11} {row['model_id']:<28} "
            f"{row['success']:>4} {row['success_rate'] * 100:>6.1f}% "
            f"{format_optional(row['corpus_cer']):>7} {format_optional(row['corpus_wer']):>7} "
            f"{format_optional(row['mean_rtf']):>7} "
            f"{format_optional(None if row['load_ms'] is None else row['load_ms'] / 1000, 2):>7} "
            f"{size:>8.0f}"
        )


def print_views(rows: list[dict]) -> None:
    print("\n=== Accuracy ranking (lower CER is better; success rate shown) ===")
    for row in sorted(rows, key=lambda item: (item["corpus_cer"] is None, item["corpus_cer"] or 0)):
        flag = "" if row["recommendable"] else "  [below min success rate]"
        print(
            f"{row['language']:<11} {row['model_id']:<28} CER {format_optional(row['corpus_cer'])}"
            f"  success {row['success_rate'] * 100:.1f}%{flag}"
        )
    print("\n=== Efficiency ranking (lower is better; not an accuracy score) ===")
    for row in sorted(
        rows,
        key=lambda item: (item["mean_rtf"] is None, item["mean_rtf"] or 0, item["model_bytes"] or 0),
    ):
        print(
            f"{row['language']:<11} {row['model_id']:<28} RTF {format_optional(row['mean_rtf'])}"
            f"  load {format_optional(None if row['load_ms'] is None else row['load_ms'] / 1000, 2)}s"
            f"  peak RSS {format_optional(None if row['peak_rss_kb'] is None else row['peak_rss_kb'] / 1024, 0)} MB"
            f"  size {(row['model_bytes'] or 0) / 1_000_000:.0f} MB"
        )
    print("\n=== Live-caption suitability ===")
    for row in sorted(rows, key=lambda item: (item["language"], item["model_id"])):
        supported = row["kind"] in STREAMING_KINDS
        state = "streaming model; run the streaming stage" if supported else "not applicable (offline model)"
        print(f"{row['language']:<11} {row['model_id']:<28} {state}")
    print(f"\nFailure policy: a model is recommendable only at success rate >= {rows[0]['min_success_rate']:.2f}.")


def latest_run(output_dir: Path) -> Path:
    runs = sorted(
        path
        for path in (output_dir / "runs").glob("*")
        if (path / "summary.json").is_file()
    )
    if not runs:
        raise EvalError(f"no completed runs found under {output_dir / 'runs'}")
    return runs[-1]


def compare(args) -> int:
    output_dir = Path(args.output_dir).expanduser()
    run_dir = Path(args.run_id) if args.run_id else latest_run(output_dir)
    summary_path = run_dir / "summary.json" if run_dir.is_dir() else run_dir
    try:
        report = json.loads(summary_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise EvalError(f"cannot read {summary_path}: {error}") from error
    rows = report["pairs"]
    if not rows:
        print("No pairs in this run.", file=sys.stderr)
        return 1
    print(f"Run: {report['run_id']}  stage: {report['stage']}")
    print(f"Policy: {report['policy']}")
    print_table(rows)
    print_views(rows)
    write_csv(output_dir / "reports" / "model-comparison.csv", SUMMARY_FIELDS, rows)
    best_by_language = {}
    for row in rows:
        if not row["recommendable"] or row["corpus_cer"] is None:
            continue
        current = best_by_language.get(row["language"])
        if current is None or row["corpus_cer"] < current["corpus_cer"]:
            best_by_language[row["language"]] = row
    write_csv(
        output_dir / "reports" / "language-comparison.csv",
        SUMMARY_FIELDS,
        [best_by_language[language] for language in sorted(best_by_language)],
    )
    print(f"\nReports written to {output_dir / 'reports'}")
    return 0


# ---------------------------------------------------------------------------
# Run orchestration
# ---------------------------------------------------------------------------


def new_run_dir(output_dir: Path, stage: str, languages: list[str]) -> tuple[Path, str]:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_id = f"{stamp}-{stage}-{'+'.join(languages)}"
    path = output_dir / "runs" / run_id
    suffix = 2
    while path.exists():
        run_id = f"{stamp}-{stage}-{'+'.join(languages)}-{suffix}"
        path = output_dir / "runs" / run_id
        suffix += 1
    path.mkdir(parents=True, exist_ok=True)
    return path, run_id


def streaming_stub(languages: list[str], args) -> int:
    cli = args.cli
    print("Streaming evaluation is not implemented yet (phase F).")
    for language in languages:
        try:
            models = list_models(cli, language)
        except EvalError as error:
            print(f"{language}: cannot evaluate streaming ({error})", file=sys.stderr)
            continue
        for model in models:
            if model.get("kind") in STREAMING_KINDS:
                state = "TODO: streaming metrics are not implemented"
            else:
                state = f"not applicable ({model.get('kind')} is an offline model)"
            print(f"{language:<11} {model['id']:<28} {state}")
    print("\nChunked timing fields (first_partial_ms, partial_count) are already emitted by")
    print("`santhra-cli transcribe-batch`; the streaming stage will consume them later.")
    return 0


def print_top_level_help() -> None:
    print("usage: test-asr-language.py <stage> [options]")
    print()
    print(__doc__.splitlines()[0])
    print()
    print("stages:")
    print("  smoke      compatibility check, default 3 samples per model")
    print("  screening  25 samples per model/language")
    print("  accuracy   150 samples per model/language, keeps per-sample output")
    print("  streaming  phase F; reports not applicable/TODO for now")
    print("  compare    print and export accuracy/efficiency/live-caption views")
    print()
    print("Run 'test-asr-language.py <stage> --help' for that stage's options.")


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv or argv[0] in ("-h", "--help"):
        print_top_level_help()
        return 0
    if argv[0] not in STAGES:
        hint = (
            "a stage is required before options"
            if argv[0].startswith("-")
            else f"unknown stage {argv[0]!r}"
        )
        raise EvalError(f"{hint}; choose one of: {', '.join(STAGES)}")
    stage, rest = argv[0], argv[1:]

    if stage == "compare":
        return compare(build_compare_parser().parse_args(rest))

    args = build_stage_parser(stage).parse_args(rest)
    args.stage = stage

    if not args.all_languages and not args.language and not args.manifest:
        raise EvalError("provide --language, --all-languages or --manifest")
    if args.all_languages and args.manifest:
        raise EvalError("--all-languages cannot be combined with --manifest")
    if args.all_languages and not args.all_models:
        raise EvalError("--all-languages requires --all-models")
    if args.download_missing_models and not args.all_models:
        raise EvalError("--download-missing-models requires --all-models")
    if args.samples < 1:
        raise EvalError("--samples must be at least 1")
    if args.max_cer is not None and not 0 <= args.max_cer <= 1:
        raise EvalError("--max-cer must be between 0 and 1")
    if not 0 <= args.min_success_rate <= 1:
        raise EvalError("--min-success-rate must be between 0 and 1")

    manifests: dict[str, dict] = {}
    if args.manifest:
        manifest = load_manifest(Path(args.manifest).expanduser())
        manifest_language = str(manifest.get("language") or "")
        if not manifest_language:
            raise EvalError("--manifest must include a language field")
        languages = [manifest_language]
        manifests[manifest_language] = manifest
    else:
        languages = list(FLEURS_CONFIGS) if args.all_languages else [args.language]
    for language in languages:
        if language not in FLEURS_CONFIGS:
            supported = ", ".join(sorted(FLEURS_CONFIGS))
            raise EvalError(
                f"no verified FLEURS test mapping for '{language}'. No fixture was substituted.\n"
                f"FLEURS-backed language ids: {supported}"
            )

    if stage == "streaming":
        return streaming_stub(languages, args)

    cache_dir = Path(args.cache_dir).expanduser()
    output_dir = Path(args.output_dir).expanduser()
    run_dir, run_id = new_run_dir(output_dir, stage, languages)
    (run_dir / "config.json").write_text(
        json.dumps({"run_id": run_id, **vars(args)}, indent=2, default=str) + "\n",
        encoding="utf-8",
    )
    (run_dir / "environment.json").write_text(
        json.dumps(environment(), indent=2) + "\n", encoding="utf-8"
    )

    all_rows: list[dict] = []
    summaries: list[dict] = []
    failures: list[dict] = []

    for language in languages:
        manifest = manifests.get(language)
        config = str((manifest or {}).get("config") or FLEURS_CONFIGS[language])
        if manifest is None:
            cached = manifest_usable(
                cache_dir, config, args.samples, args.seed, args.dataset_revision,
                args.bucket_short, args.bucket_medium,
            )
            archive_present = (cache_dir / config / "test.tar.gz").is_file()
            if cached is None and not archive_present and not args.yes_download_large:
                print(
                    f"FLEURS {config}/test audio is an archive and may be hundreds of MB. "
                    "It is cached after the first download. Review the FLEURS CC-BY-4.0 terms; "
                    "rerun with --yes-download-large to continue, or provide a cached fixture.",
                    file=sys.stderr,
                )
                return 2
            manifest = cached or ensure_dataset(
                cache_dir,
                language,
                config,
                args.samples,
                args.seed,
                args.dataset_revision,
                args.bucket_short,
                args.bucket_medium,
                args.force_download,
            )
        manifests[language] = manifest
        manifest_path = run_dir / "manifests" / f"{config}.json"
        manifest_path.parent.mkdir(parents=True, exist_ok=True)
        manifest_path.write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        if len(languages) == 1:
            (run_dir / "manifest.json").write_text(
                json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )

        models, skipped = choose_models(args.cli, language, args)
        for reason in skipped:
            print(f"Skipped model {reason}", file=sys.stderr)
        if not models:
            print(
                f"No installed compatible models for '{language}'. "
                "Use --download-missing-models to download them.",
                file=sys.stderr,
            )
            continue

        for model in models:
            model_id = str(model["id"])
            print(f"\n=== {language} / {model_id} ===", flush=True)
            batch_output = run_dir / f"batch-{language}-{model_id}.jsonl"
            try:
                meta, batch_samples, batch_summary, stderr = run_batch(
                    args.cli, model_id, language, manifest_path, batch_output, args
                )
            except (OSError, EvalError) as error:
                rows = build_rows(run_id, model_id, language, manifest["samples"], [], str(error))
                summaries.append(
                    {
                        "run_id": run_id,
                        "stage": stage,
                        "language": language,
                        "model_id": model_id,
                        "kind": model.get("kind"),
                        "attempted": len(rows),
                        "success": 0,
                        "empty_transcript": 0,
                        "failed": len(rows),
                        "timeout": 0,
                        "success_rate": 0.0,
                        "corpus_cer": None,
                        "mean_cer": None,
                        "corpus_wer": None,
                        "mean_wer": None,
                        "exact_match_rate": None,
                        "ser": None,
                        "mean_inference_seconds": None,
                        "median_inference_seconds": None,
                        "mean_rtf": None,
                        "median_rtf": None,
                        "load_ms": None,
                        "peak_rss_kb": None,
                        "model_bytes": int(sum(int(f.get("bytes", 0)) for f in model.get("files", []))),
                        "min_success_rate": args.min_success_rate,
                        "recommendable": False,
                    }
                )
                all_rows.extend(rows)
                failures.append(
                    {"model_id": model_id, "language": language, "stage": "batch", "error": str(error)}
                )
                print(f"[FAILED] batch run: {error}", file=sys.stderr)
                continue

            rows = build_rows(run_id, model_id, language, manifest["samples"], batch_samples, stderr)
            all_rows.extend(rows)
            summary = summarize_pair(
                run_id, model, language, rows, meta, batch_summary, args.min_success_rate
            )
            summary["stage"] = stage
            summaries.append(summary)

            for row in rows:
                if row["status"] != "success":
                    failures.append(
                        {
                            "model_id": model_id,
                            "language": language,
                            "sample_id": row["sample_id"],
                            "status": row["status"],
                            "error": row["error"],
                        }
                    )

            transcript_dir = run_dir / "transcripts" / model_id / language
            transcript_dir.mkdir(parents=True, exist_ok=True)
            for row in rows:
                if row["hypothesis_raw"]:
                    name = Path(row["sample_id"]).with_suffix(".txt").name
                    (transcript_dir / name).write_text(
                        row["hypothesis_raw"] + "\n", encoding="utf-8"
                    )

            for row in rows:
                marker = "CORRECT" if row["exact_match"] else "INCORRECT"
                if row["status"] != "success":
                    marker = row["status"].upper()
                print(f"[{marker}] {row['sample_id']}")
                if row["status"] == "success" and not row["exact_match"]:
                    print(f"  Reference: {row['reference_raw']}")
                    print(f"  Hypothesis: {row['hypothesis_raw']}")
                elif row["status"] != "success":
                    print(f"  error: {row['error']}")
            print(
                f"Summary: {summary['success']}/{summary['attempted']} successful | "
                f"CER {format_optional(summary['corpus_cer'])} | "
                f"success {summary['success_rate'] * 100:.1f}%"
            )

    results_path = run_dir / "results.jsonl"
    with results_path.open("w", encoding="utf-8") as handle:
        for row in all_rows:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")
    (run_dir / "failures.json").write_text(
        json.dumps(failures, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    report = {
        "run_id": run_id,
        "stage": stage,
        "dataset": "google/fleurs",
        "revision": args.dataset_revision,
        "seed": args.seed,
        "policy": {
            "min_success_rate": args.min_success_rate,
            "accuracy_over": "successful inferences only; success rate reported separately",
            "normalization": "NFKC + casefold + drop punctuation and whitespace (marks preserved)",
            "wer": "whitespace tokenizer for space-segmented languages; n/a for chinese/japanese",
            "duration_buckets_seconds": {
                "short_max": args.bucket_short,
                "medium_max": args.bucket_medium,
            },
        },
        "pairs": summaries,
    }
    (run_dir / "summary.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print()
    print_table(summaries)
    if len(summaries) > 1:
        print_views(summaries)
    print(f"\nRun directory: {run_dir}")

    exit_code = 0
    if failures:
        print(
            f"{len(failures)} failed or empty sample(s); see {run_dir / 'failures.json'}",
            file=sys.stderr,
        )
        exit_code = 1
    if args.max_cer is not None:
        over = [
            row["model_id"]
            for row in summaries
            if row["corpus_cer"] is not None and row["corpus_cer"] > args.max_cer
        ]
        if over:
            print(
                f"CER above the {args.max_cer:.3f} limit: {', '.join(over)}", file=sys.stderr
            )
            exit_code = 1
    return exit_code


# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------


def build_stage_parser(stage: str) -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog=f"test-asr-language.py {stage}",
        description=__doc__.splitlines()[0],
    )
    language = parser.add_mutually_exclusive_group()
    language.add_argument("--language", help="Santhra language id, for example indonesia")
    language.add_argument(
        "--all-languages",
        action="store_true",
        help="Run every FLEURS-backed language (requires --all-models)",
    )
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--model", help="Test one model id")
    selection.add_argument(
        "--all-models",
        action="store_true",
        help="Test every catalog model compatible with the language",
    )
    parser.add_argument(
        "--download-missing-models",
        action="store_true",
        help="With --all-models, download compatible models that are not installed",
    )
    parser.add_argument(
        "--cli", default="target/debug/santhra-cli", help="Path to a built santhra-cli executable"
    )
    parser.add_argument(
        "--samples",
        type=int,
        default=DEFAULT_SAMPLES[stage],
        help=f"Number of FLEURS test utterances (default for {stage}: {DEFAULT_SAMPLES[stage]})",
    )
    parser.add_argument("--seed", type=int, default=42, help="Deterministic sample selection seed")
    parser.add_argument("--dataset-revision", default=FLEURS_REVISION, help="Pinned FLEURS revision")
    parser.add_argument("--manifest", type=Path, help="Use an existing dataset manifest")
    parser.add_argument(
        "--output-dir", type=Path, default=REPO_ROOT / "evaluation", help="Run artifact directory"
    )
    parser.add_argument(
        "--cache-dir", type=Path, default=Path.home() / ".cache/santhra/fleurs", help="FLEURS cache"
    )
    parser.add_argument("--force-download", action="store_true", help="Ignore cached dataset files")
    parser.add_argument(
        "--yes-download-large",
        action="store_true",
        help="Allow downloading the selected language's FLEURS test archive",
    )
    parser.add_argument("--timeout", type=int, default=600, help="Batch wall-clock timeout in seconds")
    parser.add_argument("--threads", type=int, default=2, help="ASR inference threads")
    parser.add_argument("--provider", default="cpu", help="sherpa-onnx execution provider")
    parser.add_argument("--chunk-ms", type=int, default=100, help="Audio chunk size fed to the model")
    parser.add_argument(
        "--buffer-ms", type=int, default=2500, help="Buffer size for offline models, in ms"
    )
    parser.add_argument("--no-partial", action="store_true", help="Only collect final captions")
    parser.add_argument("--bucket-short", type=float, default=5.0, help="Short audio upper bound (s)")
    parser.add_argument("--bucket-medium", type=float, default=10.0, help="Medium audio upper bound (s)")
    parser.add_argument("--max-cer", type=float, help="Failure threshold for aggregate CER (0.0 to 1.0)")
    parser.add_argument(
        "--min-success-rate",
        type=float,
        default=0.9,
        help="Success rate below which a model is not recommendable",
    )
    return parser


def build_compare_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="test-asr-language.py compare",
        description="Compare a completed evaluation run.",
    )
    parser.add_argument("--latest", action="store_true", help="Use the newest run (default)")
    parser.add_argument("--run-id", type=Path, help="Run directory or summary.json to compare")
    parser.add_argument(
        "--output-dir", type=Path, default=REPO_ROOT / "evaluation", help="Run artifact directory"
    )
    return parser


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except EvalError as error:
        print(f"ASR evaluation failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error
    except (OSError, tarfile.TarError) as error:
        print(f"ASR evaluation failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error
    except KeyboardInterrupt:
        print("\nEvaluation interrupted by user.", file=sys.stderr)
        raise SystemExit(130)
