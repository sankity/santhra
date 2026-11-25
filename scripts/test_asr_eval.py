"""Unit tests for the Santhra ASR evaluation harness.

Run with: python3 -m unittest scripts/test_asr_eval.py

No model, network, or dataset access is required.
"""
from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import sys
import tarfile
import tempfile
import unittest
import wave
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))


def load_module(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / filename)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


metrics = load_module("santhra_asr_metrics", "asr_metrics.py")
evaluator = load_module("santhra_evaluator", "test-asr-language.py")


def wav_bytes(seconds: float = 0.5, sample_rate: int = 16_000) -> bytes:
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(sample_rate)
        audio.writeframes(b"\x00\x00" * int(sample_rate * seconds))
    return buffer.getvalue()


class MetricsTest(unittest.TestCase):
    def test_cer_substitution_insertion_deletion(self):
        self.assertAlmostEqual(metrics.cer("abc", "abd"), 1 / 3)
        self.assertAlmostEqual(metrics.cer("abc", "abcd"), 1 / 3)
        self.assertAlmostEqual(metrics.cer("abc", "ab"), 1 / 3)

    def test_empty_reference_has_no_cer(self):
        self.assertIsNone(metrics.cer("", "anything"))
        self.assertIsNone(metrics.cer(" , ", "anything"))

    def test_empty_hypothesis_scores_one(self):
        self.assertEqual(metrics.cer("abc", ""), 1.0)

    def test_normalization_ignores_case_punctuation_and_whitespace(self):
        self.assertEqual(metrics.cer("Hello, world!", "hello world"), 0.0)
        self.assertEqual(metrics.normalize("\ufb01le"), "file")

    def test_cjk_punctuation_and_spaces_do_not_change_cer(self):
        self.assertEqual(metrics.cer("你好，世界。", "你好 世界"), 0.0)

    def test_indic_marks_are_preserved(self):
        # "हिन्दी" and "हिंदी" differ in the nasal mark; they must not compare equal.
        self.assertGreater(metrics.cer("हिन्दी", "हिंदी"), 0.0)

    def test_wer_tokens(self):
        self.assertEqual(metrics.wer("the quick brown fox", "the quick fox", "english"), 0.25)

    def test_wer_not_applicable_without_word_boundaries(self):
        self.assertIsNone(metrics.wer("你好世界", "你好世界", "chinese"))
        self.assertIsNone(metrics.wer("こんにちは世界", "こんにちは世界", "japanese"))
        self.assertIsNone(metrics.wer("", "", "english"))

    def test_exact_match_and_sentence_error(self):
        self.assertTrue(metrics.exact_match("Hello!", "hello"))
        self.assertFalse(metrics.sentence_error("Hello!", "hello"))
        self.assertFalse(metrics.exact_match("hello world", "hello there"))
        self.assertTrue(metrics.sentence_error("hello world", "hello there"))

    def test_corpus_aggregation_differs_from_mean_of_samples(self):
        pairs = [("ab", "ab"), ("abcd", "abce")]
        corpus, edits, units = metrics.corpus_cer(pairs)
        self.assertEqual((edits, units), (1, 6))
        self.assertAlmostEqual(corpus, 1 / 6)
        per_sample = sum(metrics.cer(ref, hyp) for ref, hyp in pairs) / len(pairs)
        self.assertAlmostEqual(per_sample, 0.125)
        self.assertNotAlmostEqual(corpus, per_sample)

    def test_corpus_wer(self):
        corpus, edits, units = metrics.corpus_wer(
            [("the quick fox", "the quick fox"), ("a b c d", "a b c e")], "english"
        )
        self.assertEqual((edits, units), (1, 7))
        self.assertAlmostEqual(corpus, 1 / 7)

    def test_score_keeps_raw_text_via_caller(self):
        result = metrics.score("Hello!", "hello", "english")
        self.assertEqual(result["reference_normalized"], "hello")
        self.assertEqual(result["cer"], 0.0)
        self.assertTrue(result["exact_match"])


class WavTest(unittest.TestCase):
    def test_duration_and_format(self):
        info = evaluator.parse_wav_info(io.BytesIO(wav_bytes(seconds=0.5)))
        self.assertEqual(info["sample_rate"], 16_000)
        self.assertEqual(info["channels"], 1)
        self.assertEqual(info["audio_format"], 1)
        self.assertAlmostEqual(info["duration_seconds"], 0.5, places=6)

    def test_rejects_non_riff(self):
        with self.assertRaises(evaluator.EvalError):
            evaluator.parse_wav_info(io.BytesIO(b"not a wav file at all"))

    def test_archive_scan_and_extract(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive_path = root / "test.tar.gz"
            data = wav_bytes(seconds=0.25)
            with tarfile.open(archive_path, "w:gz") as archive:
                member = tarfile.TarInfo("audio/000.wav")
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
            info = evaluator.archive_wav_info(archive_path)
            self.assertIn("000.wav", info)
            self.assertAlmostEqual(info["000.wav"]["duration_seconds"], 0.25, places=6)
            paths = evaluator.extract_wavs(archive_path, ["000.wav"], root / "audio")
            self.assertEqual(paths["000.wav"].read_bytes(), data)


class SamplingTest(unittest.TestCase):
    def candidates(self, count: int = 30) -> list[dict]:
        return [
            {
                "id": f"id_id/test/{index:03d}.wav",
                "duration_seconds": 1.0 + (index % 15),
            }
            for index in range(count)
        ]

    def test_same_seed_is_deterministic(self):
        first, distribution = evaluator.select_samples(self.candidates(), 6, 42, 5.0, 10.0)
        second, second_distribution = evaluator.select_samples(self.candidates(), 6, 42, 5.0, 10.0)
        self.assertEqual([item["id"] for item in first], [item["id"] for item in second])
        self.assertEqual(distribution, second_distribution)

    def test_different_seed_changes_selection(self):
        first, _ = evaluator.select_samples(self.candidates(), 6, 42, 5.0, 10.0)
        second, _ = evaluator.select_samples(self.candidates(), 6, 43, 5.0, 10.0)
        self.assertNotEqual([item["id"] for item in first], [item["id"] for item in second])

    def test_duration_buckets_are_filled(self):
        selected, distribution = evaluator.select_samples(self.candidates(), 9, 7, 5.0, 10.0)
        self.assertEqual(len(selected), 9)
        self.assertEqual(sum(distribution.values()), 9)
        self.assertEqual(set(distribution), {"short", "medium", "long"})
        for bucket in distribution.values():
            self.assertGreater(bucket, 0)

    def test_duplicate_ids_are_dropped(self):
        candidates = self.candidates(3) + [self.candidates(3)[0]]
        selected, _ = evaluator.select_samples(candidates, 10, 42, 5.0, 10.0)
        self.assertEqual(len(selected), 3)

    def test_short_bucket_records_actual_distribution(self):
        long_only = [{"id": f"x/{index}", "duration_seconds": 20.0} for index in range(4)]
        selected, distribution = evaluator.select_samples(long_only, 2, 42, 5.0, 10.0)
        self.assertEqual(len(selected), 2)
        self.assertEqual(distribution, {"short": 0, "medium": 0, "long": 2})


class ReferencesTest(unittest.TestCase):
    def test_deduplicates_and_keeps_first_reference(self):
        tsv = "filename\ttranscription\nrow/000.wav\tfirst\nrow/000.wav\tsecond\nrow/001.wav\tthird\n"
        references = evaluator.read_references(tsv.encode("utf-8"))
        self.assertEqual(len(references), 2)
        self.assertEqual(references[0], {"name": "000.wav", "reference": "first"})

    def test_headerless_layout(self):
        tsv = "1\trow/000.wav\tignored\thello\n"
        references = evaluator.read_references(tsv.encode("utf-8"))
        self.assertEqual(references, [{"name": "000.wav", "reference": "hello"}])


class CacheTest(unittest.TestCase):
    def write_manifest(self, root: Path, audio: Path, revision: str = "rev-1") -> dict:
        manifest = {
            "dataset": "google/fleurs",
            "revision": revision,
            "split": "test",
            "config": "id_id",
            "language": "indonesia",
            "seed": 42,
            "sampling": {"short_max_seconds": 5.0, "medium_max_seconds": 10.0},
            "samples": [
                {
                    "id": "id_id/test/000.wav",
                    "audio_path": str(audio),
                    "reference": "halo",
                    "duration_seconds": 0.5,
                    "sha256": hashlib.sha256(audio.read_bytes()).hexdigest(),
                }
            ],
        }
        (root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        return manifest

    def test_usable_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "id_id"
            root.mkdir()
            audio = root / "000.wav"
            audio.write_bytes(wav_bytes())
            self.write_manifest(root, audio)
            usable = evaluator.manifest_usable(root.parent, "id_id", 1, 42, "rev-1", 5.0, 10.0)
            self.assertIsNotNone(usable)

    def test_revision_mismatch_invalidates_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "id_id"
            root.mkdir()
            audio = root / "000.wav"
            audio.write_bytes(wav_bytes())
            self.write_manifest(root, audio, revision="rev-1")
            self.assertIsNone(
                evaluator.manifest_usable(root.parent, "id_id", 1, 42, "rev-2", 5.0, 10.0)
            )

    def test_hash_mismatch_invalidates_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "id_id"
            root.mkdir()
            audio = root / "000.wav"
            audio.write_bytes(wav_bytes())
            self.write_manifest(root, audio)
            audio.write_bytes(wav_bytes(seconds=0.25))
            self.assertIsNone(
                evaluator.manifest_usable(root.parent, "id_id", 1, 42, "rev-1", 5.0, 10.0)
            )

    def test_seed_change_invalidates_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "id_id"
            root.mkdir()
            audio = root / "000.wav"
            audio.write_bytes(wav_bytes())
            self.write_manifest(root, audio)
            self.assertIsNone(
                evaluator.manifest_usable(root.parent, "id_id", 1, 7, "rev-1", 5.0, 10.0)
            )


class CliTest(unittest.TestCase):
    def test_help_lists_stages_and_exits_zero(self):
        self.assertEqual(evaluator.main(["--help"]), 0)

    def test_requires_a_stage(self):
        with self.assertRaises(evaluator.EvalError):
            evaluator.main(["--language", "english"])

    def test_rejects_unknown_stage(self):
        with self.assertRaises(evaluator.EvalError):
            evaluator.main(["bogus"])

    def test_stage_requires_a_language_selection(self):
        with self.assertRaises(evaluator.EvalError):
            evaluator.main(["smoke"])


class RunDiscoveryTest(unittest.TestCase):
    def test_latest_run_skips_directories_without_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            older = output / "runs" / "20260101T000000Z-smoke-english"
            newer = output / "runs" / "20260102T000000Z-smoke-english"
            aborted = output / "runs" / "20260103T000000Z-smoke-english"
            older.mkdir(parents=True)
            newer.mkdir(parents=True)
            aborted.mkdir(parents=True)
            (older / "summary.json").write_text("{}", encoding="utf-8")
            (newer / "summary.json").write_text("{}", encoding="utf-8")
            self.assertEqual(evaluator.latest_run(output), newer)

    def test_no_completed_runs_is_an_error(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(evaluator.EvalError):
                evaluator.latest_run(Path(directory))


class BatchProtocolTest(unittest.TestCase):
    def record(self, **overrides) -> str:
        row = {
            "type": "sample",
            "index": 0,
            "id": "sample-1",
            "status": "success",
            "finals": ["hello world"],
            "elapsed_ms": 500.0,
            "partial_count": 2,
            "first_partial_ms": 100.0,
            "final_caption_ms": 480.0,
            "error": None,
        }
        row.update(overrides)
        return json.dumps(row)

    def test_parses_meta_samples_and_summary(self):
        text = "\n".join(
            [
                json.dumps({"type": "meta", "model_id": "m", "load_ms": 12.0}),
                self.record(),
                json.dumps({"type": "summary", "peak_rss_kb": 100}),
            ]
        )
        meta, samples, summary = evaluator.parse_batch_output(text)
        self.assertEqual(meta["model_id"], "m")
        self.assertEqual(len(samples), 1)
        self.assertEqual(summary["peak_rss_kb"], 100)

    def test_rejects_non_json_line(self):
        text = json.dumps({"type": "meta", "model_id": "m"}) + "\nlog: model loaded\n"
        with self.assertRaises(evaluator.EvalError):
            evaluator.parse_batch_output(text)

    def test_rejects_missing_meta(self):
        with self.assertRaises(evaluator.EvalError):
            evaluator.parse_batch_output(self.record())

    def test_rejects_unknown_type(self):
        text = "\n".join(
            [json.dumps({"type": "meta"}), json.dumps({"type": "mystery"})]
        )
        with self.assertRaises(evaluator.EvalError):
            evaluator.parse_batch_output(text)


class RowsTest(unittest.TestCase):
    def manifest_samples(self) -> list[dict]:
        return [
            {"id": "a.wav", "reference": "Hello, world!", "duration_seconds": 2.0},
            {"id": "b.wav", "reference": "second one", "duration_seconds": 4.0},
            {"id": "c.wav", "reference": "third", "duration_seconds": 1.0},
        ]

    def test_success_and_empty_statuses(self):
        records = [
            {"type": "sample", "id": "a.wav", "status": "success", "finals": ["hello world"], "elapsed_ms": 500.0},
            {"type": "sample", "id": "b.wav", "status": "empty_transcript", "finals": [], "elapsed_ms": 250.0},
        ]
        rows = evaluator.build_rows("run", "model", "english", self.manifest_samples(), records, "")
        self.assertEqual(rows[0]["status"], "success")
        self.assertEqual(rows[0]["hypothesis_raw"], "hello world")
        self.assertEqual(rows[0]["cer"], 0.0)
        self.assertAlmostEqual(rows[0]["rtf"], 0.25)
        self.assertEqual(rows[1]["status"], "empty_transcript")
        self.assertIsNone(rows[1]["cer"])
        self.assertEqual(rows[2]["status"], "process_error")
        self.assertIn("no result", rows[2]["error"])

    def test_empty_finals_on_success_are_not_success(self):
        records = [{"type": "sample", "id": "a.wav", "status": "success", "finals": ["  "], "elapsed_ms": 1.0}]
        rows = evaluator.build_rows("run", "model", "english", self.manifest_samples()[:1], records, "")
        self.assertEqual(rows[0]["status"], "empty_transcript")
        self.assertIsNone(rows[0]["cer"])

    def test_timeout_status_is_preserved(self):
        records = [{"type": "sample", "id": "a.wav", "status": "timeout", "finals": [], "error": "batch timeout"}]
        rows = evaluator.build_rows("run", "model", "english", self.manifest_samples()[:1], records, "")
        self.assertEqual(rows[0]["status"], "timeout")

    def test_failure_is_not_scored(self):
        records = [{"type": "sample", "id": "a.wav", "status": "model_error", "error": "boom"}]
        rows = evaluator.build_rows("run", "model", "english", self.manifest_samples()[:1], records, "")
        self.assertEqual(rows[0]["status"], "model_error")
        self.assertIsNone(rows[0]["cer"])

    def test_summary_respects_min_success_rate(self):
        records = [
            {"type": "sample", "id": "a.wav", "status": "success", "finals": ["hello world"], "elapsed_ms": 100.0},
            {"type": "sample", "id": "b.wav", "status": "success", "finals": ["second two"], "elapsed_ms": 200.0},
            {"type": "sample", "id": "c.wav", "status": "model_error", "error": "boom"},
        ]
        rows = evaluator.build_rows("run", "model", "english", self.manifest_samples(), records, "")
        meta = {
            "model_id": "model",
            "load_ms": 800.0,
            "model_files": [{"name": "m.onnx", "bytes": 1000}],
        }
        strict = evaluator.summarize_pair(
            "run", {"id": "model"}, "english", rows, meta, {"peak_rss_kb": 2048}, 0.9
        )
        self.assertEqual(strict["attempted"], 3)
        self.assertEqual(strict["success"], 2)
        self.assertEqual(strict["failed"], 1)
        self.assertAlmostEqual(strict["success_rate"], 2 / 3, places=5)
        self.assertFalse(strict["recommendable"])
        self.assertEqual(strict["model_bytes"], 1000)
        self.assertEqual(strict["peak_rss_kb"], 2048)

        relaxed = evaluator.summarize_pair(
            "run", {"id": "model"}, "english", rows, meta, None, 0.5
        )
        self.assertTrue(relaxed["recommendable"])


if __name__ == "__main__":
    unittest.main()
