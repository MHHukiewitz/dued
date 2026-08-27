from __future__ import annotations

from dued.parse import parse_source

__all__ = ["complexity"]


def complexity(source: bytes, language: str = "python", path_suffix: str = ".py") -> tuple[int, int, int]:
    extracted = parse_source(language, path_suffix, source)
    symbol = extracted.symbols[0]
    return symbol.cyclomatic, symbol.cognitive, symbol.nesting
