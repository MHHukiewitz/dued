from __future__ import annotations

from dataclasses import dataclass, field

from dued import _native
from dued._codec import loads

__all__ = ["Extracted", "Symbol", "parse_source"]


@dataclass
class Symbol:
    name: str
    kind: str
    start_line: int
    end_line: int
    signature: str
    docstring: str
    body: str
    cyclomatic: int
    cognitive: int
    nesting: int
    nargs: int
    is_public: bool
    is_entry: bool
    is_test: bool


@dataclass
class Extracted:
    symbols: list[Symbol] = field(default_factory=list)
    imports: list[str] = field(default_factory=list)
    calls: list[tuple[str, str]] = field(default_factory=list)
    import_modules: list[str] = field(default_factory=list)
    ast_nodes: int = 0


def parse_source(language: str, path_suffix: str, source: bytes) -> Extracted:
    data = loads(_native.parse_source(language, path_suffix, source))
    return Extracted(
        symbols=[Symbol(**row) for row in data["symbols"]],
        imports=list(data["imports"]),
        calls=[(a, b) for a, b in data["calls"]],
        import_modules=list(data["import_modules"]),
        ast_nodes=data["ast_nodes"],
    )
