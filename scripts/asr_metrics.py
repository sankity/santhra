"""ASR accuracy metrics shared by the Santhra evaluation scripts.

Only punctuation and whitespace are discarded for the primary punctuation-insensitive
score. Unicode marks (Mn/Mc/Me) are preserved so Indic scripts, Japanese kana/kanji and
accented Latin stay distinct; nothing else is removed.

Word error rate is defined only for space-segmented writing systems. Chinese and
Japanese use CER as the primary metric because there is no agreed word tokenizer here.
"""
from __future__ import annotations

import unicodedata
from collections.abc import Iterable

# Languages whose scripts do not mark word boundaries with spaces.
UNSEGMENTED_LANGUAGES = frozenset({"chinese", "japanese"})


def normalize(text: str) -> str:
    """NFKC + casefold, then drop punctuation and whitespace only."""
    text = unicodedata.normalize("NFKC", text).casefold()
    return "".join(
        char
        for char in text
        if not char.isspace() and not unicodedata.category(char).startswith("P")
    )


def tokenize(text: str, language: str) -> list[str] | None:
    """Whitespace tokens with punctuation as separators.

    Returns None when word-level scoring is not applicable for the language.
    """
    if language in UNSEGMENTED_LANGUAGES:
        return None
    text = unicodedata.normalize("NFKC", text).casefold()
    separated = "".join(
        " " if unicodedata.category(char).startswith("P") else char for char in text
    )
    return separated.split()


def edit_distance(reference: Iterable, hypothesis: Iterable) -> int:
    """Levenshtein distance over any two sequences of comparable items."""
    previous = list(range(len(hypothesis) + 1))
    for row, ref_item in enumerate(reference, 1):
        current = [row]
        for column, hyp_item in enumerate(hypothesis, 1):
            current.append(
                min(
                    current[-1] + 1,
                    previous[column] + 1,
                    previous[column - 1] + (ref_item != hyp_item),
                )
            )
        previous = current
    return previous[-1]


def cer(reference: str, hypothesis: str) -> float | None:
    """Character error rate, or None when the normalized reference is empty."""
    ref = normalize(reference)
    if not ref:
        return None
    return edit_distance(ref, normalize(hypothesis)) / len(ref)


def wer(reference: str, hypothesis: str, language: str) -> float | None:
    """Word error rate, or None when the language/reference has no word units."""
    ref = tokenize(reference, language)
    if not ref:
        return None
    return edit_distance(ref, tokenize(hypothesis, language) or []) / len(ref)


def exact_match(reference: str, hypothesis: str) -> bool:
    return normalize(reference) == normalize(hypothesis)


def sentence_error(reference: str, hypothesis: str) -> bool:
    return not exact_match(reference, hypothesis)


def corpus_cer(pairs: Iterable[tuple[str, str]]) -> tuple[float | None, int, int]:
    """Corpus-level CER: total edit distance / total reference characters."""
    edits = units = 0
    for reference, hypothesis in pairs:
        ref = normalize(reference)
        if not ref:
            continue
        edits += edit_distance(ref, normalize(hypothesis))
        units += len(ref)
    return (edits / units if units else None), edits, units


def corpus_wer(pairs: Iterable[tuple[str, str]], language: str) -> tuple[float | None, int, int]:
    """Corpus-level WER: total word edits / total reference words."""
    edits = units = 0
    for reference, hypothesis in pairs:
        ref = tokenize(reference, language)
        if not ref:
            continue
        edits += edit_distance(ref, tokenize(hypothesis, language) or [])
        units += len(ref)
    return (edits / units if units else None), edits, units


def score(reference: str, hypothesis: str, language: str) -> dict[str, object]:
    """Per-sample scores. Raw text is never modified."""
    return {
        "reference_normalized": normalize(reference),
        "hypothesis_normalized": normalize(hypothesis),
        "cer": cer(reference, hypothesis),
        "wer": wer(reference, hypothesis, language),
        "exact_match": exact_match(reference, hypothesis),
        "sentence_error": sentence_error(reference, hypothesis),
    }
